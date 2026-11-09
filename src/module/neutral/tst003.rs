// SPDX-License-Identifier: MIT OR Apache-2.0
//! TST-003: no empty, trivial or non-calling tests (the Rust reading).
//!
//! **What this judges:** every tracked `.rs` file outside vendored and fixture directories is parsed with `syn`, and every function
//! with a `#[test]` attribute (`#[test]`, `#[tokio::test]`, `#[async_std::test]`, anything whose last path segment is `test`) is read,
//! wherever it sits: a `tests/` file, a `#[cfg(test)]` module, an inline module. A test is reported when its body
//!
//! 1. **is empty** (no statement at all; comments do not count), or
//! 2. **asserts only tautologies**: every assertion is `assert!(true)`, `assert!(!false)` or `assert_eq!(x, x)` with the same literal
//!    on both sides, and the body holds no other check and calls nothing, or
//! 3. **calls nothing and checks nothing**: no call, no method call, no macro, no `?` and no `unwrap`/`expect`: a body of
//!    `let` bindings and literals, which cannot fail and cannot exercise any code.
//!
//! A test that calls code and asserts nothing (a smoke test: "does not panic") is **not** reported, and neither is one whose body
//! declares an item (a nested `fn`, `use` or `type`: a compile-time reachability check). Tests marked `#[ignore]` (a
//! placeholder kept on purpose) and `#[should_panic]` (which check by panicking) are left alone. One **Medium**-severity finding per
//! test; the confidence is High for an empty body or a tautology and Medium for a body that calls nothing.
//!
//! **What it cannot see (and the record says so):** the second half of the catalog statement, a test that *claims* to test a unit but
//! never reaches it, needs per-test coverage and is not built; a test whose assertion compares a value to itself through a variable;
//! tests made by macros or by `rstest`/`proptest` blocks other than a plain `#[test]`; other languages. A file `syn` cannot parse is
//! counted in a note, never read as clean.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::TST_003;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Why {
    Empty,
    Tautology,
    NoCall,
}

pub(super) struct Site {
    pub line: u32,
    pub name: String,
    pub why: Why,
}

fn is_test_attr(a: &syn::Attribute) -> bool {
    a.path().segments.last().is_some_and(|s| s.ident == "test")
        && !a.path().is_ident("cfg")
        && matches!(a.meta, syn::Meta::Path(_))
}

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

/// What a test body does.
#[derive(Default)]
struct Body {
    asserts: usize,
    tautologies: usize,
    /// A call, a method call, a non-assert macro, `?`, `unwrap`, `expect`: anything that exercises code or can fail.
    acts: bool,
}

fn macro_name(m: &syn::Macro) -> String {
    m.path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

fn tokens_text(m: &syn::Macro) -> Vec<String> {
    // the arguments split at top-level commas, each with its whitespace removed
    let mut args = vec![String::new()];
    for t in m.tokens.clone() {
        if let proc_macro2::TokenTree::Punct(p) = &t {
            if p.as_char() == ',' {
                args.push(String::new());
                continue;
            }
        }
        args.last_mut().unwrap().push_str(&t.to_string());
    }
    args.retain(|a| !a.is_empty());
    args
}

fn is_literal_text(a: &str) -> bool {
    let c = a.chars().next().unwrap_or(' ');
    c.is_ascii_digit() || c == '"' || c == '\'' || a == "true" || a == "false"
}

impl<'ast> Visit<'ast> for Body {
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = macro_name(m);
        if name == "assert" || name == "assert_eq" || name == "assert_ne" || name == "debug_assert"
        {
            self.asserts += 1;
            let args = tokens_text(m);
            let tautology = match name.as_str() {
                "assert" | "debug_assert" => {
                    args.first().is_some_and(|a| a == "true" || a == "!false")
                }
                "assert_eq" => args.len() >= 2 && args[0] == args[1] && is_literal_text(&args[0]),
                _ => false,
            };
            if tautology {
                self.tautologies += 1;
            }
        } else {
            self.acts = true;
        }
        syn::visit::visit_macro(self, m);
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        self.acts = true;
        syn::visit::visit_expr_call(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        self.acts = true;
        syn::visit::visit_expr_method_call(self, e);
    }

    fn visit_expr_try(&mut self, e: &'ast syn::ExprTry) {
        self.acts = true;
        syn::visit::visit_expr_try(self, e);
    }
}

fn judge_fn(attrs: &[syn::Attribute], block: &syn::Block) -> Option<Why> {
    if !attrs.iter().any(is_test_attr)
        || has_attr(attrs, "ignore")
        || has_attr(attrs, "should_panic")
    {
        return None;
    }
    if block.stmts.is_empty() {
        return Some(Why::Empty);
    }
    let mut body = Body::default();
    body.visit_block(block);
    if body.asserts > 0 && body.tautologies == body.asserts && !body.acts {
        return Some(Why::Tautology);
    }
    // an item inside the body (a nested `fn`, `use`, `type`) is a compile-time check: the test passes by compiling
    let compile_only = block.stmts.iter().any(|s| matches!(s, syn::Stmt::Item(_)));
    if body.asserts == 0 && !body.acts && !compile_only {
        return Some(Why::NoCall);
    }
    None
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if let Some(why) = judge_fn(&f.attrs, &f.block) {
            self.sites.push(Site {
                line: u32::try_from(f.sig.span().start().line).unwrap_or(0),
                name: f.sig.ident.to_string(),
                why,
            });
        }
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if let Some(why) = judge_fn(&f.attrs, &f.block) {
            self.sites.push(Site {
                line: u32::try_from(f.sig.span().start().line).unwrap_or(0),
                name: f.sig.ident.to_string(),
                why,
            });
        }
        syn::visit::visit_impl_item_fn(self, f);
    }
}

/// The trivial tests of one file, or `None` when `syn` cannot parse it.
pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut walk = Walk::default();
    walk.visit_file(&parsed);
    walk.sites.sort_by_key(|s| s.line);
    Some(walk.sites)
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !skipped(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust source file".into());
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        let Some(found) = view.read(path).and_then(|t| sites(&t)) else {
            unparsed.push(path);
            continue;
        };
        for s in found {
            let (what, confidence) = match s.why {
                Why::Empty => ("has an empty body", Confidence::High),
                Why::Tautology => (
                    "asserts only something that is always true",
                    Confidence::High,
                ),
                Why::NoCall => (
                    "calls nothing and checks nothing (only bindings and literals)",
                    Confidence::Medium,
                ),
            };
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    confidence,
                    view.name(),
                    format!("test `{}` {what}", s.name),
                    "A test that cannot fail passes forever and gives the impression of protection where there is none. Call the code under test and assert something about its observable behaviour, or delete the test (mark a placeholder `#[ignore]` with a reason).",
                )
                .location(Location::new(path.as_str(), Some(s.line)))
                .subject(s.name)
                .positive_control(format!(
                    "{} Rust source file(s) were parsed with the same walk that finds `#[test]` functions",
                    files.len()
                )),
            );
        }
    }
    let note = (!unparsed.is_empty()).then(|| {
        format!(
            "{ID}: {} Rust file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        )
    });
    match note {
        Some(n) => Verdict::Noted(findings, vec![n]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<(String, Why)> {
        sites(src)
            .unwrap()
            .into_iter()
            .map(|s| (s.name, s.why))
            .collect()
    }

    #[test]
    fn empty_tautological_and_non_calling_tests_are_reported() {
        let src = r#"
#[test]
fn empty() {}
#[test]
fn comment_only() {
    // todo
}
#[test]
fn taut() { assert!(true); assert_eq!(1, 1); }
#[test]
fn bindings() { let x = 1; let _y = x + 2; }
"#;
        assert_eq!(
            names(src),
            [
                ("empty".to_string(), Why::Empty),
                ("comment_only".to_string(), Why::Empty),
                ("taut".to_string(), Why::Tautology),
                ("bindings".to_string(), Why::NoCall),
            ]
        );
    }

    #[test]
    fn real_tests_smoke_tests_ignored_and_should_panic_are_not_reported() {
        let src = r#"
fn add(a: u8, b: u8) -> u8 { a + b }
#[test]
fn real() { assert_eq!(add(1, 2), 3); }
#[test]
fn smoke() { add(1, 2); }
#[test]
fn checks_with_unwrap() -> Result<(), String> { "1".parse::<u8>().map_err(|e| e.to_string())?; Ok(()) }
#[test]
#[ignore = "later"]
fn placeholder() {}
#[test]
#[should_panic]
fn boom() { let v: Vec<u8> = vec![]; let _ = v[0]; }
#[test]
fn mixed() { assert!(true); add(1, 1); }
fn not_a_test() {}
#[test]
fn compile_only() { fn _name(_: Option<u8>) {} }
"#;
        assert_eq!(names(src), []);
    }

    #[test]
    fn tests_in_cfg_test_modules_and_async_tests_are_read() {
        let src = r#"
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn empty_async() {}
    mod inner { #[test] fn nested() {} }
}
"#;
        assert_eq!(
            names(src),
            [
                ("empty_async".to_string(), Why::Empty),
                ("nested".to_string(), Why::Empty)
            ]
        );
    }

    #[test]
    fn an_unparsable_file_is_noted_not_clean() {
        let v = judge(&RepoView::from_files(&[("src/lib.rs", "fn (((")]));
        assert!(matches!(v, Verdict::Noted(f, n) if f.is_empty() && n.len() == 1));
    }

    #[test]
    fn every_finding_validates() {
        let v = judge(&RepoView::from_files(&[(
            "tests/a.rs",
            "#[test]\nfn t() {}\n",
        )]));
        let Verdict::Findings(f) = v else {
            panic!("expected findings")
        };
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        f.into_iter().next().unwrap().validate().unwrap();
    }
}
