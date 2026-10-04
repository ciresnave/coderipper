//! The three checks on a workspace MEMBER: the project is the member's directory, findings are about
//! that package only and carry package-relative paths, and the allowlist is the member's own.

mod common;

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::{ReachabilityCheck, UnusedParametersCheck, UnusedReturnValuesCheck};
use coderipper::finding::Finding;
use common::{git_repo_with, MANIFEST};
use std::path::Path;

const WORKSPACE: &str = "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n";

fn package(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
}

const A_LIB: &str = "\
pub fn used_by_b() -> i32 { 1 }
pub fn dead_in_a() -> i32 { 2 }
pub fn a_unused_param(p: i32) {}
pub fn a_discards() -> i32 { 3 }
pub fn a_caller() { a_discards(); }
";

const B_LIB: &str = "\
pub fn b_fn() -> i32 { a::used_by_b() }
pub fn b_dead(unused_b: i32) -> i32 { 3 }
pub fn b_helper() -> i32 { 4 }
pub fn b_discarder() { b_helper(); }
";

fn workspace_repo(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let b_manifest = package("b", "\n[dependencies]\na = { path = \"../a\" }\n");
    let mut files = vec![
        ("Cargo.toml", WORKSPACE.to_string()),
        ("a/Cargo.toml", package("a", "")),
        ("a/src/lib.rs", A_LIB.to_string()),
        ("b/Cargo.toml", b_manifest),
        ("b/src/lib.rs", B_LIB.to_string()),
    ];
    files.extend(extra.iter().map(|(n, c)| (*n, c.to_string())));
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
    git_repo_with(&borrowed)
}

fn run(check: &dyn Check, project: &Path) -> anyhow::Result<Vec<Finding>> {
    check.run(&CheckContext::new(
        project.to_path_buf(),
        project.to_path_buf(),
    ))
}

fn subjects(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

fn files(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings
        .iter()
        .filter_map(|f| f.location.as_ref().map(|l| l.file.clone()))
        .collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn unused_parameters_on_a_member_reports_that_package_only_with_package_relative_paths() {
    let repo = workspace_repo(&[]);
    let b = run(&UnusedParametersCheck, &repo.path().join("b")).unwrap();
    assert_eq!(subjects(&b), vec!["b_dead::unused_b"]);
    assert_eq!(files(&b), vec!["src/lib.rs"]);
    // the sibling is analyzed on its own, not as a side effect
    let a = run(&UnusedParametersCheck, &repo.path().join("a")).unwrap();
    assert_eq!(subjects(&a), vec!["a_unused_param::p"]);
}

#[test]
fn unused_return_values_on_a_member_reports_that_package_only() {
    let repo = workspace_repo(&[]);
    let b = run(&UnusedReturnValuesCheck, &repo.path().join("b")).unwrap();
    assert_eq!(subjects(&b), vec!["b_helper"]);
    assert_eq!(files(&b), vec!["src/lib.rs"]);
    let a = run(&UnusedReturnValuesCheck, &repo.path().join("a")).unwrap();
    assert_eq!(subjects(&a), vec!["a_discards"]);
}

#[test]
fn reachability_on_a_member_reports_that_package_only() {
    let repo = workspace_repo(&[]);
    let b = run(&ReachabilityCheck, &repo.path().join("b")).unwrap();
    // nothing in b calls these (b has no bin or test), and `dead_in_a` etc. are not b's to report
    assert_eq!(
        subjects(&b),
        vec!["b_dead", "b_discarder", "b_fn", "b_helper"]
    );
    assert_eq!(files(&b), vec!["src/lib.rs"]);
}

#[test]
fn a_project_scope_check_does_not_count_a_sibling_members_use() {
    // `used_by_b` is called by member b, but this check's scope is one package: for `a` it has zero
    // callers. (Whether another crate in the portfolio uses a pub item is the portfolio-scope pass.)
    let repo = workspace_repo(&[]);
    let a = run(&ReachabilityCheck, &repo.path().join("a")).unwrap();
    assert!(
        subjects(&a).contains(&"used_by_b".to_string()),
        "{:?}",
        subjects(&a)
    );
}

#[test]
fn a_virtual_workspace_root_is_refused_by_every_check_and_names_the_members() {
    let repo = workspace_repo(&[]);
    for check in [
        &ReachabilityCheck as &dyn Check,
        &UnusedParametersCheck,
        &UnusedReturnValuesCheck,
    ] {
        let err = run(check, repo.path()).unwrap_err().to_string();
        assert!(
            err.contains("not a package directory"),
            "{} said: {err}",
            check.id()
        );
        assert!(err.contains('a') && err.contains('b'), "{err}");
    }
}

#[test]
fn the_allowlist_is_the_members_own() {
    let repo = workspace_repo(&[(
        "b/.coderipper.toml",
        "[[allow]]\ncheck = \"unused-parameters\"\nfile = \"src/lib.rs\"\nsymbol = \"b_dead::unused_b\"\nreason = \"kept for the next release\"\n",
    )]);
    let result = coderipper::run_checks(
        &CheckContext::new(repo.path().join("b"), repo.path().to_path_buf()),
        Tier::Fast,
        Some("unused-parameters"),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn a_workspace_root_that_is_itself_a_package_is_analyzed_as_that_package() {
    let manifest = format!("{MANIFEST}\n[workspace]\nmembers = [\"sub\"]\n");
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        ("src/lib.rs", "pub fn root_fn(unused_root: i32) {}\n"),
        ("sub/Cargo.toml", &package("sub", "")),
        ("sub/src/lib.rs", "pub fn sub_fn(unused_sub: i32) {}\n"),
    ]);
    let root = run(&UnusedParametersCheck, repo.path()).unwrap();
    assert_eq!(subjects(&root), vec!["root_fn::unused_root"]);
    let sub = run(&UnusedParametersCheck, &repo.path().join("sub")).unwrap();
    assert_eq!(subjects(&sub), vec!["sub_fn::unused_sub"]);
}

#[test]
fn a_workspace_below_the_repository_root_uses_both_prefixes_correctly() {
    // The git repository is the monorepo (`proj/` is one cargo workspace inside it), so the member's
    // path in the CHECKOUT (`proj/m`) differs from its path in the WORKSPACE (`m`), which is the base
    // cargo uses for diagnostic file names.
    let repo = git_repo_with(&[
        (
            "proj/Cargo.toml",
            "[workspace]\nmembers = [\"m\"]\nresolver = \"2\"\n",
        ),
        ("proj/m/Cargo.toml", &package("m", "")),
        ("proj/m/src/lib.rs", "pub fn f(unused_m: i32) {}\n"),
        ("other/README.md", "unrelated sibling directory\n"),
    ]);
    let found = run(&UnusedParametersCheck, &repo.path().join("proj/m")).unwrap();
    assert_eq!(subjects(&found), vec!["f::unused_m"]);
    assert_eq!(files(&found), vec!["src/lib.rs"]);
}

// ---- Review fixes ----

#[test]
fn a_path_dependency_of_the_root_package_is_not_reported_as_the_root_packages_dead_code() {
    // The root package depends on `sub`, so cargo builds `sub` too and `sub`'s private dead function
    // warns. It is not the root package's.
    let manifest = format!(
        "{MANIFEST}\n[dependencies]\nsub = {{ path = \"sub\" }}\n\n[workspace]\nmembers = [\"sub\"]\n"
    );
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        ("src/lib.rs", "pub fn root_fn() -> i32 { sub::sub_pub() }\n"),
        ("sub/Cargo.toml", &package("sub", "")),
        (
            "sub/src/lib.rs",
            "pub fn sub_pub() -> i32 { 1 }\nfn sub_private_dead() {}\n",
        ),
    ]);
    assert_eq!(
        subjects(&run(&ReachabilityCheck, repo.path()).unwrap()),
        vec!["root_fn"]
    );
}

#[test]
fn a_member_nested_below_another_member_is_not_reported_as_the_outer_ones() {
    let repo = git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"b\", \"b/inner\"]\nresolver = \"2\"\n",
        ),
        (
            "b/Cargo.toml",
            &package("b", "\n[dependencies]\ninner = { path = \"inner\" }\n"),
        ),
        (
            "b/src/lib.rs",
            "pub fn b_fn() -> i32 { inner::inner_pub() }\n",
        ),
        ("b/inner/Cargo.toml", &package("inner", "")),
        (
            "b/inner/src/lib.rs",
            "pub fn inner_pub() -> i32 { 1 }\nfn inner_private_dead() {}\n",
        ),
    ]);
    let found = run(&ReachabilityCheck, &repo.path().join("b")).unwrap();
    assert_eq!(subjects(&found), vec!["b_fn"]);
    assert_eq!(files(&found), vec!["src/lib.rs"]);
}

#[test]
fn default_members_do_not_make_a_sibling_part_of_the_analysis() {
    // `cargo build` at the root builds every default member; only the root package is the project.
    let manifest = format!(
        "{MANIFEST}\n[workspace]\nmembers = [\"sub\"]\ndefault-members = [\".\", \"sub\"]\n"
    );
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        ("src/lib.rs", "pub fn root_fn() {}\n"),
        ("sub/Cargo.toml", &package("sub", "")),
        ("sub/src/lib.rs", "pub fn sub_pub() {}\nfn sub_dead() {}\n"),
    ]);
    assert_eq!(
        subjects(&run(&ReachabilityCheck, repo.path()).unwrap()),
        vec!["root_fn"]
    );
}

#[test]
fn a_member_that_is_not_committed_says_so() {
    let repo = workspace_repo(&[]);
    std::fs::create_dir_all(repo.path().join("c/src")).unwrap();
    std::fs::write(repo.path().join("c/Cargo.toml"), package("c", "")).unwrap();
    std::fs::write(
        repo.path().join("c/src/lib.rs"),
        "pub fn f(unused: i32) {}\n",
    )
    .unwrap();
    let err = run(&UnusedParametersCheck, &repo.path().join("c"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("HEAD") && err.contains("committed"), "{err}");
}

#[test]
fn a_refusal_names_the_directory_the_user_passed_not_the_throwaway_checkout() {
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn f() {}\n")]);
    let project = repo.path().join("src");
    let err = run(&UnusedParametersCheck, &project)
        .unwrap_err()
        .to_string();
    assert!(err.contains(&project.display().to_string()), "{err}");
    // The throwaway checkout is `<tmp>/wt/...`. Look for that as a path COMPONENT: a bare substring test matched
    // CI's random temp directory `.tmpvsPwtr` (a correct message naming the user's own directory).
    assert!(
        !err.contains("/wt") && !err.contains(r"\wt"),
        "must not leak the checkout path: {err}"
    );
}
