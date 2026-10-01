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
fn a_crate_wide_allow_of_either_lint_cannot_hide_a_finding() {
    // Was an error ("blind crate") before the review: `--force-warn` now makes both lints fire
    // regardless of `allow`, so the crate is analyzable and the discard is reported.
    for allow in ["deprecated", "unused_must_use"] {
        let repo = git_repo_with(&[
            ("Cargo.toml", MANIFEST),
            (
                "src/main.rs",
                &format!(
                    "#![allow({allow})]
fn f() -> i32 {{ 1 }}
fn main() {{ f(); }}
"
                ),
            ),
        ]);
        assert_eq!(names(&run(&repo).unwrap()), vec!["f"], "allow({allow})");
    }
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

const LIB_WITH_VAL: &str = "pub fn val() -> i32 { 1 }
pub fn internal() { val(); }
";
const MAIN_USING_VAL_TWICE: &str =
    "fn main() { let x = fixture::val(); let y = fixture::val(); println!(\"{}\", x + y); }
";

fn summary_of(findings: &[Finding], name: &str) -> String {
    findings
        .iter()
        .find(|f| f.summary.contains(&format!("`{name}`")))
        .unwrap_or_else(|| panic!("no finding for {name}: {findings:?}"))
        .summary
        .clone()
}

#[test]
fn deny_warnings_in_the_lib_does_not_hide_the_bins_uses() {
    // Review finding (Critical): with `#![deny(warnings)]` on the lib, the tags make the lib fail
    // to build, cargo never compiles the dependent bin, and its uses were silently never counted:
    // a fn used 3 times (1 discarded) was reported as "discarded at every one of its 1 call site".
    let lib = format!(
        "#![deny(warnings)]
{LIB_WITH_VAL}"
    );
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", &lib),
        ("src/main.rs", MAIN_USING_VAL_TWICE),
    ]);
    let s = summary_of(&run(&repo).unwrap(), "val");
    assert!(s.contains("1 of its 3 call sites"), "{s}");
}

#[test]
fn a_cargo_lints_table_that_denies_warnings_does_not_hide_the_bins_uses() {
    let manifest = format!(
        "{MANIFEST}
[lints.rust]
warnings = \"deny\"
"
    );
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        ("src/lib.rs", LIB_WITH_VAL),
        ("src/main.rs", MAIN_USING_VAL_TWICE),
    ]);
    let s = summary_of(&run(&repo).unwrap(), "val");
    assert!(s.contains("1 of its 3 call sites"), "{s}");
}

#[test]
fn uses_inside_a_local_allow_deprecated_module_still_count() {
    // Review finding (Important): a module-level `#[allow(deprecated)]` silenced the use reports
    // inside it, so a fn consumed there looked discarded at every call site.
    let lib = "pub fn val() -> i32 { 1 }
pub fn discards() { val(); }
#[allow(deprecated)]
pub mod legacy {
    pub fn a() -> i32 { crate::val() }
    pub fn b() -> i32 { crate::val() }
}
";
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", lib)]);
    let s = summary_of(&run(&repo).unwrap(), "val");
    assert!(s.contains("1 of its 3 call sites"), "{s}");
}

#[test]
fn calls_from_a_local_macro_are_counted_per_invocation() {
    // Review finding (Important): rustc reports `deprecated` at the macro DEFINITION span for every
    // invocation, and span-based dedup collapsed three uses into one, so 3 discarded of 4 uses read
    // as "discarded at every call site".
    let main = "fn val() -> i32 { 1 }
macro_rules! call { () => { val(); }; }
fn main() { call!(); call!(); call!(); println!(\"{}\", val()); }
";
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", main)]);
    let s = summary_of(&run(&repo).unwrap(), "val");
    assert!(s.contains("3 of its 4 call sites"), "{s}");
}

#[test]
fn an_import_in_tests_examples_or_benches_is_not_a_call_site() {
    // Review finding (Important): only `src/` use-statements were recorded, so `use fixture::val;`
    // in an integration test counted as an extra use and a fn discarded everywhere read "mixed".
    let test_rs = "use fixture::val;
use fixture::{
    two,
};
#[test]
fn t() { val(); two(); }
";
    let lib = "pub fn val() -> i32 { 1 }
pub fn two() -> i32 { 2 }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("tests/t.rs", test_rs),
    ]);
    let findings = run(&repo).unwrap();
    for name in ["val", "two"] {
        let s = summary_of(&findings, name);
        assert!(s.contains("every one of its 1 call site"), "{name}: {s}");
    }
}
