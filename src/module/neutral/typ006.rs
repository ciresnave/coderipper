// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-006: no `unwrap` or `expect` on a production path without a written invariant (the Rust, syntactic half; **opt-in**).
//!
//! **What this judges:** every tracked `.rs` file that is production source (outside vendored trees, development tooling, test
//! directories and `build.rs`; items under `#[cfg(test)]` or `#[test]` are skipped) is parsed with `syn`. A method call `.unwrap()`
//! (no arguments) or `.expect(arg)` (one argument) is a site. A file with at least one **unjustified** site gets one **Medium**
//! finding at high confidence, at the first such site, with the count: the rule is reported per file because it fires widely, and
//! the number of sites is the evidence.
//!
//! **A site is justified in writing when:**
//! - a comment on the same line, up to the end of the statement, or alone on the line above **begins** with `INVARIANT`, `SAFETY`
//!   or `UNWRAP-OK` (after the comment marker), or
//! - it is an `expect("..")` whose literal message states the invariant: it contains one of `invariant`, `always`, `never`,
//!   `cannot`, `guaranteed`, `validated`, `checked`, `by construction`.
//!
//! `unwrap_or`, `unwrap_or_else`, `unwrap_or_default`, `unwrap_err` and `expect_err` are not sites.
//!
//! **Why opt-in:** it runs when asked for by id (`check TYP-006`) and in no profile, because it fires widely across the portfolio;
//! the corpus counts are in the changelog. It is also the static core of `ERR-003`, which is not built.
//!
//! **What it cannot see:** another language (the catalog rule also names TypeScript's `!`); whether the receiver is a `Result`, an
//! `Option` or a type of its own with an `unwrap` method (`syn` has no types); an `unwrap` inside a macro's tokens; whether a
//! stated invariant is true. A file `syn` cannot parse is counted in a note, never read as clean.

use super::rust_src::{
    comment_text, is_test_item, line_of, production_files, statement_end, MAX_BYTES,
};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{Expr, ExprMethodCall, ImplItemFn, ItemFn, ItemMod, Lit};

const ID: &str = crate::rule_id::TYP_006;

/// A comment that begins with one of these is the written invariant.
const MARKERS: &[&str] = &["INVARIANT", "SAFETY", "UNWRAP-OK"];

/// An `expect` message that contains one of these states the invariant.
const STATES_INVARIANT: &[&str] = &[
    "invariant",
    "always",
    "never",
    "cannot",
    "guaranteed",
    "validated",
    "checked",
    "by construction",
];

struct Scan<'a> {
    lines: &'a [&'a str],
    /// Sites with no written invariant: the line of each.
    open: Vec<u32>,
    /// All sites, justified or not.
    sites: usize,
}

fn begins_with_marker(comment: &str) -> bool {
    let body = comment.trim_start_matches(['/', '!', '*', ' ', '\t']);
    MARKERS.iter().any(|m| body.starts_with(m))
}

/// Whether a comment beginning with a marker sits on the previous line or on any of the lines `first..=last` (1-based).
fn marker_beside(lines: &[&str], first: u32, last: u32) -> bool {
    let first = first.max(1) as usize;
    let last = (last as usize).max(first);
    let prev = (first >= 2)
        .then(|| lines.get(first - 2).map(|l| l.trim_start()))
        .flatten()
        .filter(|l| l.starts_with("//") || l.starts_with("/*"));
    prev.is_some_and(begins_with_marker)
        || (first..=last).any(|n| {
            lines
                .get(n - 1)
                .and_then(|l| comment_text(l))
                .is_some_and(begins_with_marker)
        })
}

fn message_states_invariant(arg: &Expr) -> bool {
    let Expr::Lit(l) = arg else { return false };
    let Lit::Str(s) = &l.lit else { return false };
    let message = s.value().to_ascii_lowercase();
    STATES_INVARIANT.iter().any(|w| message.contains(w))
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_item(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_item(&f.attrs) {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        if !is_test_item(&f.attrs) {
            syn::visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast ExprMethodCall) {
        let site = (m.method == "unwrap" && m.args.is_empty())
            || (m.method == "expect" && m.args.len() == 1);
        if site {
            self.sites += 1;
            let line = line_of(m.method.span());
            let last = statement_end(self.lines, line);
            let justified = (m.method == "expect" && message_states_invariant(&m.args[0]))
                || marker_beside(self.lines, line, last);
            if !justified {
                self.open.push(line);
            }
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = production_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source outside tests, tooling and vendored trees".into(),
        );
    }
    let mut per_file: Vec<(String, Vec<u32>, usize)> = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let mut parsed = 0usize;
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        if text.len() > MAX_BYTES || !(text.contains("unwrap") || text.contains("expect")) {
            continue;
        }
        let Ok(file) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        parsed += 1;
        let lines: Vec<&str> = text.lines().collect();
        let mut scan = Scan {
            lines: &lines,
            open: Vec::new(),
            sites: 0,
        };
        scan.visit_file(&file);
        if !scan.open.is_empty() {
            per_file.push((path.clone(), scan.open, scan.sites));
        }
    }
    per_file.sort();
    let total = files.len();
    let out: Vec<Finding> = per_file
        .into_iter()
        .map(|(path, open, sites)| {
            let first = open[0];
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::High,
                view.name(),
                format!(
                    "{path}:{first}: {} `unwrap()` or `expect(..)` call(s) on a production path with no written invariant ({sites} such call(s) in the file in all)",
                    open.len()
                ),
                "Return the error to the caller, or write the invariant: a comment beginning `INVARIANT`, `SAFETY` or `UNWRAP-OK` on that line or the line above, or an `expect(\"..\")` message that says why it cannot fail.",
            )
            .location(Location::new(path.as_str(), Some(first)))
            .subject(view.name())
            .positive_control(format!(
                "{total} production Rust file(s) tracked, {parsed} of them parsed for `unwrap` and `expect`; this file has {} open site(s), the first at {path}:{first}",
                open.len()
            ))
        })
        .collect();
    if unparsed.is_empty() {
        Verdict::Findings(out)
    } else {
        let note = format!(
            "{} Rust file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed
                .iter()
                .take(5)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        );
        Verdict::Noted(out, vec![note])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    #[test]
    fn an_unwrap_and_an_expect_are_one_finding_per_file_with_the_count() {
        let src =
            "fn f(a: Option<u8>, b: Result<u8, ()>) {\n    a.unwrap();\n    b.expect(\"x\");\n}\n";
        let f = run(&[("src/lib.rs", src), ("src/clean.rs", "fn g() {}\n")]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("2 `unwrap()`"), "{}", f[0].summary);
        assert_eq!(f[0].location.as_ref().and_then(|l| l.line), Some(2));
    }

    #[test]
    fn a_written_invariant_justifies_the_site() {
        let src = "\
fn f(a: Option<u8>, b: Result<u8, ()>, c: Option<u8>, d: Option<u8>) {
    // INVARIANT: filled by the constructor
    a.unwrap();
    b.unwrap(); // UNWRAP-OK: parsed above
    c.expect(\"the slot is always filled by new()\");
    d.expect(\"x\"); // SAFETY: checked non-empty
}
";
        assert!(run(&[("src/lib.rs", src)]).is_empty());
    }

    #[test]
    fn an_ordinary_comment_is_not_a_written_invariant() {
        let src = "fn f(a: Option<u8>) {\n    // takes the value\n    a.unwrap();\n}\n";
        assert_eq!(run(&[("src/lib.rs", src)]).len(), 1);
    }

    #[test]
    fn the_safe_variants_tests_and_other_directories_are_not_sites() {
        let src = "\
fn f(a: Option<u8>, b: Result<u8, ()>) {
    a.unwrap_or(1);
    a.unwrap_or_default();
    b.clone().unwrap_or_else(|_| 2);
    b.unwrap_err();
}
#[test]
fn t() { Some(1).unwrap(); }
#[cfg(test)]
mod tests { fn u() { Some(1).unwrap(); } }
";
        assert!(run(&[("src/lib.rs", src)]).is_empty());
        let tests_dir = judge(&RepoView::from_files(&[(
            "tests/a.rs",
            "fn f() { Some(1).unwrap(); }\n",
        )]));
        assert!(matches!(tests_dir, Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_file_that_does_not_parse_is_noted_never_clean() {
        match judge(&RepoView::from_files(&[("src/lib.rs", "fn ( a.unwrap();")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(notes[0].contains("could not be parsed"), "{notes:?}");
            }
            _ => panic!("expected a Noted verdict"),
        }
    }
}
