// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`TypeScriptModule`]: the TypeScript checks behind the module interface (ROADMAP items 8 and 12), in process, on `tree-sitter`
//! with the `tree-sitter-typescript` grammar (the PM's parser ruling of 2026-11-21: tree-sitter for every language module).
//!
//! It reads the files git tracks at `HEAD` (never the disk), parses each TypeScript file and judges it. A file the grammar cannot
//! parse cleanly is never read as clean: its rule says so in a note. The first rule is `ERR-001` (errors are not swallowed): a `catch`
//! clause whose block holds nothing at all. A block that holds only a comment is the "record in a comment why dropping is safe"
//! the rule's remediation allows, so it is not reported.

use tree_sitter::{Language, Node, Parser};

use super::neutral::RepoView;
use super::{
    Capabilities, ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleClaim,
    RuleResult,
};
use crate::finding::{Confidence, Finding, Location, Severity};

/// The one rule this module answers so far.
const ERR_001: &str = "ERR-001";
/// The second rule: no non-null assertion (`x!`) on a production path without a written invariant. Opt-in, like its Rust twin.
const TYP_006: &str = "TYP-006";
/// The third rule: a number parse whose failure (`NaN`) is replaced on the spot by a literal (`parseInt(s) || 0`). Opt-in.
const ERR_011: &str = "ERR-011";

/// File extensions read as TypeScript source (JavaScript joins with its own module row, ROADMAP item 12).
const EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts"];

/// The rules this module claims, by id.
pub fn rule_ids() -> Vec<&'static str> {
    vec![ERR_001, TYP_006, ERR_011]
}

/// The TypeScript module: stateless, so it can be shared.
#[derive(Debug, Default, Clone, Copy)]
pub struct TypeScriptModule;

impl TypeScriptModule {
    /// A module over the tracked files of whatever project it is asked about.
    pub fn new() -> Self {
        Self
    }
}

/// A tracked path that is first-party TypeScript source: right extension, not a declaration file, not under a vendored or
/// dependency directory, not a test file (the same carve-outs the Rust rule makes for tests and fixtures).
fn is_source(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    if !EXTENSIONS.contains(&ext) || lower.ends_with(".d.ts") {
        return false;
    }
    let skipped_dir = |seg: &str| {
        matches!(
            seg,
            "node_modules"
                | "vendor"
                | "vendored"
                | "third_party"
                | "dist"
                | "build"
                | "tests"
                | "test"
                | "__tests__"
                | "e2e"
                | "fixtures"
                | "conformance"
        )
    };
    let mut parts = lower.split('/');
    let file = parts.next_back().unwrap_or("");
    if parts.any(skipped_dir) {
        return false;
    }
    !(file.contains(".test.") || file.contains(".spec."))
}

fn language_for(path: &str) -> Language {
    if path.to_ascii_lowercase().ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }
}

/// The 1-based lines of every `catch` clause in `root` whose block holds no statement and no comment.
fn empty_catches(root: Node<'_>) -> Vec<u32> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "catch_clause" {
            if let Some(body) = node.child_by_field_name("body") {
                if body.kind() == "statement_block" && body.named_child_count() == 0 {
                    found.push(u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_unstable();
    found
}

/// The 1-based lines of every non-null assertion (`value!`) in `root` that no comment justifies. A site is justified when a comment
/// on its own line, or alone on the line above, begins with `INVARIANT`, `SAFETY` or `UNWRAP-OK` (the Rust rule's markers).
fn unjustified_assertions(root: Node<'_>, text: &str) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let marker = |s: &str| {
        let t = s.trim_start().trim_start_matches(['/', '*', ' ']);
        ["INVARIANT", "SAFETY", "UNWRAP-OK"]
            .iter()
            .any(|m| t.starts_with(m))
    };
    let justified = |row: usize| {
        let same = lines
            .get(row)
            .and_then(|l| l.split_once("//"))
            .is_some_and(|(_, c)| marker(c))
            || lines
                .get(row)
                .and_then(|l| l.split_once("/*"))
                .is_some_and(|(_, c)| marker(c));
        let above = row > 0
            && lines.get(row - 1).is_some_and(|l| {
                let t = l.trim_start();
                (t.starts_with("//") || t.starts_with("/*")) && marker(t)
            });
        same || above
    };
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "non_null_expression" && !justified(node.start_position().row) {
            found.push(u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX));
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_unstable();
    found
}

/// Words that, in a comment on the line or the line above, say a fallback is meant (the Rust rule's list).
const FALLBACK_WORDS: &[&str] = &[
    "default",
    "fall back",
    "fallback",
    "lenient",
    "best effort",
    "on purpose",
    "deliberate",
    "intentional",
];

/// The 1-based lines of every `parseInt(..)`, `parseFloat(..)` or `Number(..)` call (also `Number.parseInt`/`parseFloat`) that is the
/// left operand of `||` with a literal on the right, so a parse failure becomes that literal, unless a comment on the line or the
/// line above says the fallback is meant.
fn parse_fallbacks(root: Node<'_>, text: &str) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let says_why = |row: usize| {
        let comment = |l: &str| {
            let l = l.to_ascii_lowercase();
            let c = l
                .find("//")
                .or_else(|| l.find("/*"))
                .map(|i| l[i..].to_string());
            c.is_some_and(|c| FALLBACK_WORDS.iter().any(|w| c.contains(w)))
        };
        let own = lines.get(row).is_some_and(|l| comment(l));
        let above = row > 0
            && lines.get(row - 1).is_some_and(|l| {
                let t = l.trim_start();
                (t.starts_with("//") || t.starts_with("/*")) && comment(t)
            });
        own || above
    };
    let is_parse = |n: Node<'_>| {
        if n.kind() != "call_expression" {
            return false;
        }
        let Some(f) = n.child_by_field_name("function") else {
            return false;
        };
        let name = &text[f.byte_range()];
        matches!(
            name,
            "parseInt" | "parseFloat" | "Number" | "Number.parseInt" | "Number.parseFloat"
        )
    };
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "binary_expression" {
            let op = node
                .child_by_field_name("operator")
                .map(|o| &text[o.byte_range()]);
            let (l, r) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            );
            if let (Some("||"), Some(l), Some(r)) = (op, l, r) {
                let literal = matches!(r.kind(), "number" | "string" | "true" | "false" | "null");
                if is_parse(l) && literal && !says_why(node.start_position().row) {
                    found.push(u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX));
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_unstable();
    found
}

/// What reading one repository for one rule found.
struct Read {
    findings: Vec<Finding>,
    files: usize,
    /// Files tree-sitter could not parse without an error node: read as far as they parse, and said so.
    unparsable: Vec<String>,
}

fn read_rule(view: &RepoView, rule: &str) -> Read {
    let mut out = Read {
        findings: Vec::new(),
        files: 0,
        unparsable: Vec::new(),
    };
    for path in view.paths().iter().filter(|p| is_source(p)) {
        let Some(text) = view.read(path) else {
            continue;
        };
        out.files += 1;
        let mut parser = Parser::new();
        if parser.set_language(&language_for(path)).is_err() {
            out.unparsable.push(path.clone());
            continue;
        }
        let Some(tree) = parser.parse(&text, None) else {
            out.unparsable.push(path.clone());
            continue;
        };
        if tree.root_node().has_error() {
            out.unparsable.push(path.clone());
        }
        if rule == TYP_006 {
            let sites = unjustified_assertions(tree.root_node(), &text);
            if let Some(first) = sites.first() {
                out.findings.push(
                    Finding::new(
                        TYP_006,
                        Severity::Medium,
                        Confidence::High,
                        view.name(),
                        format!(
                            "{} unjustified non-null assertion(s) (`!`), the first at line {first}",
                            sites.len()
                        ),
                        "Handle the absent case, propagate it, or write `// INVARIANT: ...` saying why the value cannot be absent.",
                    )
                    .location(Location::new(path.clone(), Some(*first)))
                    .subject(path.clone()),
                );
            }
            continue;
        }
        if rule == ERR_011 {
            for line in parse_fallbacks(tree.root_node(), &text) {
                out.findings.push(
                    Finding::new(
                        ERR_011,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        "a number parse whose failure is replaced on the spot by a literal (`||`)",
                        "Test the parse result with Number.isNaN and handle the failure, or write a comment saying the fallback is intended.",
                    )
                    .location(Location::new(path.clone(), Some(line)))
                    .subject(path.clone()),
                );
            }
            continue;
        }
        for line in empty_catches(tree.root_node()) {
            out.findings.push(
                Finding::new(
                    ERR_001,
                    Severity::High,
                    Confidence::High,
                    view.name(),
                    "a `catch` block whose body is only braces swallows the error",
                    "Handle the failure, rethrow it, or write a comment in the block saying why dropping it is safe here.",
                )
                .location(Location::new(path.clone(), Some(line)))
                .subject(path.clone()),
            );
        }
    }
    out
}

impl Module for TypeScriptModule {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "typescript",
            env!("CARGO_PKG_VERSION"),
            vec!["typescript".into()],
            vec!["package.json".into(), "tsconfig.json".into()],
            // reads tracked files and runs nothing of the project's
            Capabilities::new(false, false, false, false),
            vec![
                {
                    let mut claim = RuleClaim::native(ERR_011).only_languages(&["typescript"]);
                    claim.proof = Some("conformance-typescript/ERR-011".into());
                    claim
                },
                {
                    let mut claim = RuleClaim::native(TYP_006).only_languages(&["typescript"]);
                    claim.proof = Some("conformance-typescript/TYP-006".into());
                    claim
                },
                {
                    let mut claim = RuleClaim::native(ERR_001).only_languages(&["typescript"]);
                    // earned by `conformance-typescript/ERR-001` (tests/typescript_conformance.rs ties this string to it)
                    claim.proof = Some("conformance-typescript/ERR-001".into());
                    claim
                },
            ],
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let mut events = Vec::new();
        let mut summary = ModuleSummary {
            rules_requested: request.rules.len(),
            ..ModuleSummary::default()
        };
        let view = RepoView::from_git(&request.project_root);
        for rule in &request.rules {
            if ![ERR_001, TYP_006, ERR_011].contains(&rule.as_str()) {
                events.push(Event::RuleResult(RuleResult::error(
                    rule,
                    ErrorKind::Internal,
                    "the TypeScript module has no such rule",
                )));
                summary.rules_errored += 1;
                continue;
            }
            let view = match &view {
                Ok(view) => view,
                Err(e) => {
                    events.push(Event::RuleResult(RuleResult::error(
                        rule,
                        ErrorKind::Internal,
                        e.to_string(),
                    )));
                    summary.rules_errored += 1;
                    continue;
                }
            };
            let read = read_rule(view, rule);
            if read.files == 0 {
                events.push(Event::RuleResult(RuleResult::not_applicable(
                    rule,
                    "the repository tracks no first-party TypeScript source file",
                )));
                summary.rules_skipped += 1;
                continue;
            }
            let count = read.findings.len();
            let mut notes = vec![format!(
                "{} TypeScript file(s) read with tree-sitter",
                read.files
            )];
            if !read.unparsable.is_empty() {
                notes.push(format!(
                    "{} file(s) did not parse cleanly and were read only as far as they parse: {}",
                    read.unparsable.len(),
                    read.unparsable.join(", ")
                ));
            }
            summary.rules_ran += 1;
            summary.findings += count;
            events.extend(
                read.findings
                    .into_iter()
                    .map(|f| Event::Finding(Box::new(f))),
            );
            events.push(Event::RuleResult(RuleResult::ran_noted(
                rule, count, &notes,
            )));
        }
        events.push(Event::Summary(summary));
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(files: &[(&str, &str)]) -> Read {
        read_rule(&RepoView::from_files(files), ERR_001)
    }

    #[test]
    fn an_empty_catch_is_reported_at_its_line() {
        let r = read(&[(
            "src/a.ts",
            "function f() {\n  try {\n    g();\n  } catch (e) {\n  }\n}\n",
        )]);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(
            r.findings[0].location.as_ref().and_then(|l| l.line),
            Some(4)
        );
        r.findings[0].clone().validate().expect("a valid finding");
    }

    #[test]
    fn a_catch_with_a_statement_or_only_a_comment_is_not_reported() {
        let r = read(&[
            ("src/handled.ts", "try { g(); } catch (e) { log(e); }\n"),
            (
                "src/said.ts",
                "try { g(); } catch (e) { /* the cache is optional, a miss is fine */ }\n",
            ),
            (
                "src/optional.ts",
                "try { g(); } catch { throw new Error('x'); }\n",
            ),
        ]);
        assert!(r.findings.is_empty());
        assert_eq!(
            r.files, 3,
            "the three files were read, so the empty result is a verdict"
        );
    }

    #[test]
    fn a_tsx_file_and_a_nested_catch_are_read() {
        let r = read(&[(
            "ui/App.tsx",
            "export const A = () => { try { f(); } catch (e) { try { g(); } catch (e2) {} } return <div/>; };\n",
        )]);
        assert_eq!(r.findings.len(), 1, "only the inner catch is empty");
    }

    #[test]
    fn declaration_test_and_dependency_files_are_not_first_party_source() {
        for skipped in [
            "types/a.d.ts",
            "node_modules/x/index.ts",
            "src/a.test.ts",
            "src/a.spec.tsx",
            "tests/a.ts",
            "src/__tests__/a.ts",
            "src/a.js",
        ] {
            assert!(!is_source(skipped), "{skipped}");
        }
        assert!(is_source("src/a.ts") && is_source("src/a.mts") && is_source("web/B.tsx"));
    }

    #[test]
    fn a_file_that_does_not_parse_is_named_not_read_as_clean() {
        let r = read(&[("src/broken.ts", "function ( { try { } catch (e) {} \n")]);
        assert_eq!(r.unparsable, vec!["src/broken.ts"]);
    }

    fn read_typ(files: &[(&str, &str)]) -> Read {
        read_rule(&RepoView::from_files(files), TYP_006)
    }

    #[test]
    fn a_non_null_assertion_is_one_finding_per_file_naming_the_first() {
        let r = read_typ(&[(
            "src/a.ts",
            "const a = m.get('k')!;
const b = x!.y;
if (a !== b) {}
class C { f!: number }
",
        )]);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(
            r.findings[0].location.as_ref().and_then(|l| l.line),
            Some(1)
        );
        assert!(
            r.findings[0].summary.starts_with("2 unjustified"),
            "{}",
            r.findings[0].summary
        );
        r.findings[0].clone().validate().expect("valid");
    }

    #[test]
    fn a_stated_invariant_justifies_an_assertion() {
        let r = read_typ(&[(
            "src/a.ts",
            "// INVARIANT: set in init
const a = m.get('k')!;
const b = x!; // SAFETY: checked above
",
        )]);
        assert!(r.findings.is_empty());
        assert_eq!(r.files, 1);
    }

    fn read_parse(files: &[(&str, &str)]) -> Read {
        read_rule(&RepoView::from_files(files), ERR_011)
    }

    #[test]
    fn a_parse_replaced_by_a_literal_is_reported() {
        let r = read_parse(&[(
            "src/a.ts",
            "const a = parseInt(s, 10) || 0;
const b = Number(x) || 'n/a';
const c = parseFloat(y) || 1.5;
",
        )]);
        assert_eq!(r.findings.len(), 3);
        r.findings[0].clone().validate().expect("valid");
    }

    #[test]
    fn a_checked_parse_or_a_stated_fallback_or_a_non_parse_is_not_reported() {
        let r = read_parse(&[(
            "src/a.ts",
            "const a = Number.isNaN(n) ? 0 : n;
const b = parseInt(s) || 0; // default is intentional
// lenient on purpose
const c = Number(s) || 0;
const d = count || 0;
const e = parseInt(s) || other();
const f = parseInt(s) ?? 0;
",
        )]);
        assert!(
            r.findings.is_empty(),
            "{:?}",
            r.findings
                .iter()
                .map(|f| f.location.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(r.files, 1);
    }

    #[test]
    fn the_module_claims_err_001_for_typescript_only() {
        let hello = TypeScriptModule::new().describe().unwrap();
        assert_eq!(hello.rules.len(), 3);
        assert!(hello.rules[0].covers_language("typescript"));
        assert!(!hello.rules[0].covers_language("rust"));
    }
}
