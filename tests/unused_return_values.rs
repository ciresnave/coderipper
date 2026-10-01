mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::UnusedReturnValuesCheck;
use coderipper::finding::Finding;
use common::{git_repo_with, MANIFEST};

fn run(repo: &tempfile::TempDir) -> anyhow::Result<Vec<Finding>> {
    UnusedReturnValuesCheck.run(&CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    })
}

fn names(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings
        .iter()
        .map(|f| f.summary.split('`').nth(1).unwrap().to_string())
        .collect();
    v.sort();
    v
}

const LIB: &str = "\
pub fn always_ignored() -> i32 { 1 }
pub fn sometimes_ignored() -> i32 { 2 }
pub fn always_used() -> i32 { 3 }
pub fn never_called() -> i32 { 4 }
pub fn unit() {}
";

const MAIN: &str = "\
use fixture::always_ignored;
fn main() {
    always_ignored();
    fixture::sometimes_ignored();
    let x = fixture::sometimes_ignored();
    println!(\"{}\", x + fixture::always_used());
    fixture::unit();
}
";

#[test]
fn it_separates_always_ignored_from_sometimes_ignored_on_a_lib_plus_bin_package() {
    // A lib consumed by its own bin: the reachability check cannot analyze this shape at all
    // (E0603 after its pub->pub(crate) rewrite). This check needs no visibility rewrite.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("src/main.rs", MAIN),
    ]);
    let findings = run(&repo).unwrap();

    assert_eq!(
        names(&findings),
        vec!["always_ignored", "sometimes_ignored"]
    );
    let always = findings
        .iter()
        .find(|f| f.summary.contains("`always_ignored`"))
        .unwrap();
    // Review Focus: `use fixture::always_ignored;` must not count as a second use.
    assert!(
        always.summary.contains("every one of its 1 call site"),
        "{}",
        always.summary
    );
    let sometimes = findings
        .iter()
        .find(|f| f.summary.contains("`sometimes_ignored`"))
        .unwrap();
    assert!(
        sometimes.summary.contains("1 of its 2 call sites"),
        "{}",
        sometimes.summary
    );
    assert!(findings.iter().all(|f| f.positive_control.is_some()));
    assert!(findings.iter().all(|f| f.clone().validate().is_ok()));
}

#[test]
fn a_deliberate_let_underscore_discard_counts_as_a_use_not_an_ignore() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn f() -> i32 { 1 }\nfn main() { let _ = f(); _ = f(); }\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}

#[test]
fn a_function_that_only_discards_its_own_recursive_result_is_not_flagged() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn rec(n: i32) -> i32 { if n == 0 { 0 } else { rec(n - 1); 1 } }\nfn main() { println!(\"{}\", rec(3)); }\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}

#[test]
fn an_allowlisted_function_is_suppressed() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nsymbol = \"f\"\nreason = \"test\"\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}
#[test]
fn a_crate_that_allows_deprecated_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![allow(deprecated)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    let err = run(&repo).unwrap_err().to_string();
    assert!(err.contains("positive control"), "{err}");
}

#[test]
fn a_crate_that_allows_unused_must_use_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![allow(unused_must_use)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    assert!(run(&repo).is_err());
}

#[test]
fn deny_warnings_does_not_hide_findings_or_look_like_a_broken_build() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![deny(warnings)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    assert_eq!(names(&run(&repo).unwrap()), vec!["f"]);
}

#[test]
fn a_crate_that_does_not_compile_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn main() { let x: i32 = \"no\"; }\n"),
    ]);
    assert!(run(&repo).is_err());
}

#[test]
fn a_file_with_non_ascii_text_and_crlf_endings_is_analyzed_correctly() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "// héllo wörld\r\nfn f() -> i32 { 1 }\r\nfn main() { /* ünï */ f(); }\r\n",
        ),
    ]);
    assert_eq!(names(&run(&repo).unwrap()), vec!["f"]);
}

#[test]
fn skipped_function_shapes_never_produce_findings() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "\
struct B;
impl B { fn chain(&mut self) -> &mut Self { self } }
fn res() -> Result<i32, ()> { Ok(1) }
async fn asy() -> i32 { 1 }
#[must_use]
fn already() -> i32 { 1 }
fn main() {
    let mut b = B;
    b.chain();
    let _ = res();
    let _ = asy();
    let _ = already();
}
",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}
