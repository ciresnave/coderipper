mod common;

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::UnusedParametersCheck;
use coderipper::finding::Finding;
use common::{git_repo_with, MANIFEST};

fn run(repo: &tempfile::TempDir) -> anyhow::Result<Vec<Finding>> {
    UnusedParametersCheck.run(&CheckContext::new(repo.path().to_path_buf()))
}

fn subjects(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

/// Every shape the check must report or must leave alone, in one crate. Lines matter: the
/// assertions below name subjects, and a wrong position would drop a finding.
const LIB: &str = "\
pub fn free(a: i32, b: i32) -> i32 { a }
pub fn underscore(_a: i32, c: i32) -> i32 { c }
pub fn local_unused() { let x = 1; }
pub struct S;
impl S {
    pub fn method(&self, p: i32, q: i32) -> i32 { q }
}
pub trait T {
    fn req(&self, r: i32) -> i32;
    fn dflt(&self, d: i32) -> i32 { 0 }
}
impl T for S {
    fn req(&self, r: i32) -> i32 { 1 }
}
pub fn closure() -> Vec<i32> { vec![1].into_iter().map(|e| 1).collect() }
#[allow(unused_variables)]
pub fn allowed_here(z: i32) {}
pub fn pat((a, b): (i32, i32)) -> i32 { a }
pub fn mutp(mut m: i32) {}
macro_rules! mk { ($n:ident) => { pub fn $n(arg: i32) {} }; }
mk!(made);
";

#[test]
fn it_reports_exactly_the_unused_parameters_of_free_functions_and_inherent_methods() {
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", LIB)]);
    let findings = run(&repo).unwrap();

    // Not reported, each for its own reason: `_a` (underscore convention), `x` (a local, not a
    // parameter), `r` twice and `d` (trait signatures), `e` (closure), `z` (explicit allow), `arg`
    // (macro-generated).
    assert_eq!(
        subjects(&findings),
        vec!["S::method::p", "free::b", "mutp::m", "pat::b"]
    );
    assert!(findings.iter().all(|f| f.positive_control.is_some()));
    assert!(findings.iter().all(|f| f.clone().validate().is_ok()));
    let free_b = findings
        .iter()
        .find(|f| f.subject.as_deref() == Some("free::b"))
        .unwrap();
    assert_eq!(free_b.location.as_ref().unwrap().line, Some(1));
    assert!(
        free_b.summary.contains("parameter `b` of `free`"),
        "{}",
        free_b.summary
    );
}

#[test]
fn a_lib_consumed_by_its_own_bin_is_analyzed_and_bin_parameters_count() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f(a: i32, b: i32) -> i32 { a }\n"),
        (
            "src/main.rs",
            "fn helper(n: i32) {}\nfn main() { println!(\"{}\", fixture::f(1, 2)); helper(1); }\n",
        ),
    ]);
    // src/main.rs is a separate crate root; its own parameter `n` is reported too.
    assert_eq!(subjects(&run(&repo).unwrap()), vec!["f::b", "helper::n"]);
}

#[test]
fn same_named_methods_on_different_types_are_distinct_findings() {
    // The fingerprint is qualified, so one allowlist entry cannot hide the other.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "pub struct S;\npub struct T;\nimpl S { pub fn new(a: i32) -> S { S } }\nimpl T { pub fn new(a: i32) -> T { T } }\n",
        ),
    ]);
    assert_eq!(
        subjects(&run(&repo).unwrap()),
        vec!["S::new::a", "T::new::a"]
    );
}

#[test]
fn a_crate_wide_allow_errors_instead_of_reporting_clean() {
    for allow in ["unused_variables", "unused"] {
        let src = format!("#![allow({allow})]\npub fn f(a: i32) {{}}\n");
        let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", &src)]);
        let err = run(&repo).unwrap_err().to_string();
        assert!(err.contains("positive control"), "allow({allow}): {err}");
    }
}

#[test]
fn deny_warnings_does_not_hide_findings() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "#![deny(warnings)]\npub fn f(a: i32) {}\n"),
    ]);
    assert_eq!(subjects(&run(&repo).unwrap()), vec!["f::a"]);
}

#[test]
fn a_crate_that_does_not_compile_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f(a: i32) { let x: i32 = \"no\"; }\n"),
    ]);
    assert!(run(&repo).is_err());
}

#[test]
fn non_ascii_text_and_crlf_do_not_shift_the_match() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "/* héllo ünï */ pub fn f(a: i32) {}\r\npub fn g(b: i32) {}\r\n",
        ),
    ]);
    assert_eq!(subjects(&run(&repo).unwrap()), vec!["f::a", "g::b"]);
}

#[test]
fn the_host_applies_the_allowlist_to_a_qualified_subject() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub struct S;\nimpl S { pub fn m(&self, p: i32) {} }\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"unused-parameters\"\nfile = \"src/lib.rs\"\nsymbol = \"S::m::p\"\nreason = \"kept for the trait impl to come\"\n",
        ),
    ]);
    let result = coderipper::run_checks(
        &CheckContext::new(repo.path().to_path_buf()),
        Tier::Fast,
        Some("unused-parameters"),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn deny_warnings_in_the_lib_does_not_hide_the_bins_parameters() {
    // With `#![deny(warnings)]` on the lib, its unused parameter becomes an error, cargo never
    // compiles the dependent bin, and the bin's own unused parameter would silently go unreported.
    // `--cap-lints=warn` keeps the lib building so the bin is analyzed too.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "#![deny(warnings)]\npub fn f(a: i32) {}\n"),
        (
            "src/main.rs",
            "fn helper(n: i32) {}\nfn main() { fixture::f(1); helper(1); }\n",
        ),
    ]);
    assert_eq!(subjects(&run(&repo).unwrap()), vec!["f::a", "helper::n"]);
}

#[test]
fn a_crate_wide_allow_in_a_bin_target_errors_even_though_the_lib_is_fine() {
    // Review finding: the sentinel lived only in lib.rs, so it proved nothing about a bin, which is
    // its own crate with its own crate-level attributes. `helper::n` was silently dropped.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f(a: i32) {}\n"),
        (
            "src/main.rs",
            "#![allow(unused_variables)]\nfn helper(n: i32) {}\nfn main() { helper(1); }\n",
        ),
    ]);
    let err = run(&repo).unwrap_err().to_string();
    assert!(
        err.contains("positive control") && err.contains("src/main.rs"),
        "{err}"
    );
}

#[test]
fn a_package_with_only_src_bin_targets_is_analyzed() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/bin/a.rs", "fn main() { h(1); }\nfn h(q: i32) {}\n"),
        (
            "src/bin/b/main.rs",
            "fn main() { k(1); }\nfn k(z: i32) {}\n",
        ),
    ]);
    assert_eq!(subjects(&run(&repo).unwrap()), vec!["h::q", "k::z"]);
}
