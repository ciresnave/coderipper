// SPDX-License-Identifier: MIT OR Apache-2.0
//! TST-004: over-mocking heuristics in tests (the Rust reading, for `mockall`-style mocks).
//!
//! **What this judges:** every tracked `.rs` file outside vendored and fixture directories is parsed with `syn`, and every function
//! with a `#[test]` attribute (`#[test]`, `#[tokio::test]`, anything whose last path segment is `test`) is read. In its body it
//! counts three things:
//!
//! * **expectations**: calls of a method whose name starts with `expect_` (the `mockall` setup that sets up a mock's behaviour);
//! * **call-count checks**: calls of `.times(..)` (the `mockall` verification that a mock was called so many times);
//! * **assertions**: `assert!`, `assert_eq!`, `assert_ne!`, `debug_assert!` and the `assert_*` family.
//!
//! A test is reported when **either**
//!
//! 1. it sets up at least **four** expectations and at least **four times** as many expectations as assertions (a test that is mostly
//!    mock set-up and little check of an outcome), or
//! 2. it makes at least **three** call-count checks and no assertion at all (a test that only verifies who called whom).
//!
//! One **Low**-severity, Low-confidence finding per test, at the test. The rule is **contested** in the catalog (mockist teams verify
//! interactions on purpose), the thresholds are the rule's own defaults and are not configurable yet, so it runs only when named
//! (`coderipper check TST-004`).
//!
//! **What it cannot see (and the record says so):** whether a mocked collaborator is internal or a boundary the project does not own
//! (the catalog's "internal collaborators" half: a call-count check on an external port is exactly what a mock is for); mocks made by
//! other libraries (`mockito`, `wiremock`, `faux`, hand-written fakes) or whose set-up is spelled differently; a mock built in a
//! helper function the test calls; other languages. A file `syn` cannot parse is counted in a note, never read as clean.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::TST_004;

/// Expectations a test sets up before it is read as mock-heavy.
const MIN_EXPECTATIONS: usize = 4;
/// Expectations per assertion at which a mock-heavy test is reported.
const RATIO: usize = 4;
/// Call-count checks, with no assertion, at which an interaction-only test is reported.
const MIN_TIMES: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Why {
    MockHeavy {
        expectations: usize,
        assertions: usize,
    },
    InteractionOnly {
        checks: usize,
    },
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

#[derive(Default)]
struct Counts {
    expectations: usize,
    times: usize,
    assertions: usize,
}

impl<'ast> Visit<'ast> for Counts {
    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        let name = e.method.to_string();
        // `Result::expect_err` is the standard library's, not a mock set-up
        if name.starts_with("expect_") && name != "expect_err" {
            self.expectations += 1;
        } else if name == "times" {
            self.times += 1;
        }
        syn::visit::visit_expr_method_call(self, e);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = m
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if name.starts_with("assert") || name.starts_with("debug_assert") {
            self.assertions += 1;
        }
        syn::visit::visit_macro(self, m);
    }
}

fn judge_fn(attrs: &[syn::Attribute], block: &syn::Block) -> Option<Why> {
    if !attrs.iter().any(is_test_attr) {
        return None;
    }
    let mut c = Counts::default();
    c.visit_block(block);
    if c.expectations >= MIN_EXPECTATIONS && c.expectations >= RATIO * c.assertions.max(1) {
        return Some(Why::MockHeavy {
            expectations: c.expectations,
            assertions: c.assertions,
        });
    }
    if c.times >= MIN_TIMES && c.assertions == 0 {
        return Some(Why::InteractionOnly { checks: c.times });
    }
    None
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
}

impl Walk {
    fn push(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) {
        if let Some(why) = judge_fn(attrs, block) {
            self.sites.push(Site {
                line: u32::try_from(sig.span().start().line).unwrap_or(0),
                name: sig.ident.to_string(),
                why,
            });
        }
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.push(&f.attrs, &f.sig, &f.block);
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.push(&f.attrs, &f.sig, &f.block);
        syn::visit::visit_impl_item_fn(self, f);
    }
}

/// The over-mocked tests of one file, or `None` when `syn` cannot parse it.
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
            let what = match s.why {
                Why::MockHeavy {
                    expectations,
                    assertions,
                } => format!(
                    "sets up {expectations} mock expectations for {assertions} assertion(s)"
                ),
                Why::InteractionOnly { checks } => {
                    format!("checks {checks} mock call counts and asserts nothing")
                }
            };
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Low,
                    view.name(),
                    format!("test `{}` {what}", s.name),
                    "A test that is mostly mock set-up, or that only verifies which collaborator was called, restates the implementation: it breaks on a harmless refactor and says little about behaviour. Prefer a fake at a boundary you own, assert on the outcome rather than the interaction, and use real components where the seam is yours. Mocks at a boundary you do not own are what mocks are for: this rule cannot tell the two apart.",
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

    fn whys(src: &str) -> Vec<(String, Why)> {
        sites(src)
            .unwrap()
            .into_iter()
            .map(|s| (s.name, s.why))
            .collect()
    }

    #[test]
    fn a_test_that_is_mostly_expectations_is_reported() {
        let src = r#"
#[test]
fn heavy() {
    let mut m = MockRepo::new();
    m.expect_a().returning(|| 1);
    m.expect_b().returning(|| 2);
    m.expect_c().returning(|| 3);
    m.expect_d().returning(|| 4);
    run(&m);
    assert_eq!(1, 1);
}
"#;
        assert_eq!(
            whys(src),
            [(
                "heavy".to_string(),
                Why::MockHeavy {
                    expectations: 4,
                    assertions: 1
                }
            )]
        );
    }

    #[test]
    fn a_test_that_only_checks_call_counts_is_reported() {
        let src = r#"
#[tokio::test]
async fn interactions() {
    let mut m = MockBus::new();
    m.expect_x().times(1);
    m.expect_y().times(2);
    m.expect_z().times(1);
    go(&m).await;
}
"#;
        assert_eq!(
            whys(src),
            [(
                "interactions".to_string(),
                Why::InteractionOnly { checks: 3 }
            )]
        );
    }

    #[test]
    fn enough_assertions_few_expectations_and_non_tests_are_not_reported() {
        let src = r#"
#[test]
fn balanced() {
    let mut m = MockRepo::new();
    m.expect_a().returning(|| 1);
    m.expect_b().returning(|| 2);
    m.expect_c().returning(|| 3);
    m.expect_d().returning(|| 4);
    assert_eq!(run(&m), 10);
    assert!(ok());
    assert!(ok());
    assert!(ok());
}
#[test]
fn small() {
    let mut m = MockRepo::new();
    m.expect_a().times(1);
    m.expect_b().times(1);
    run(&m);
}
fn helper() {
    let mut m = MockRepo::new();
    m.expect_a(); m.expect_b(); m.expect_c(); m.expect_d();
}
"#;
        assert_eq!(whys(src), []);
    }

    #[test]
    fn expect_err_is_the_standard_librarys_not_a_mock_setup() {
        let src = "#[test]
fn t() {
    a().expect_err(\"x\"); b().expect_err(\"y\"); c().expect_err(\"z\"); d().expect_err(\"w\");
}
";
        assert_eq!(whys(src), []);
    }

    #[test]
    fn an_unparsable_file_is_noted_not_clean() {
        let v = judge(&RepoView::from_files(&[("src/lib.rs", "fn (((")]));
        assert!(matches!(v, Verdict::Noted(f, n) if f.is_empty() && n.len() == 1));
    }

    #[test]
    fn every_finding_validates() {
        let src =
            "#[test]\nfn t() {\n    m.expect_a(); m.expect_b(); m.expect_c(); m.expect_d();\n}\n";
        let Verdict::Findings(f) = judge(&RepoView::from_files(&[("tests/a.rs", src)])) else {
            panic!("expected findings")
        };
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        f.into_iter().next().unwrap().validate().unwrap();
    }
}
