#![cfg(feature = "cli")]
//! The CLI's exit codes, which are a promise to CI: 0 nothing at or above `--deny` (and every check ran), 1 a finding at
//! or above `--deny`, 2 a usage error, 3 a check could not run. `--deny` is off by default, like clippy's warnings: a
//! finding is printed but does not fail the run unless the caller asks for it (`--deny medium` in CI).

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use predicates::prelude::*;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A project with one Medium finding from `unused-return-values` (`f`'s result is always discarded).
fn project_with_a_medium_finding() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ])
}

fn clean_project() -> tempfile::TempDir {
    common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")])
}

/// A crate that does not compile: every check that builds it reports an error instead of a verdict.
fn project_that_does_not_build() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "this is not valid rust {{{\n"),
    ])
}

/// The CLI with the build cache OFF, so a test never reads or writes a developer's real cache.
fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

fn run(args: &[&str], project: &std::path::Path) -> assert_cmd::assert::Assert {
    coderipper()
        .args(args)
        .arg("--project")
        .arg(project)
        .assert()
}

#[test]
fn a_finding_without_deny_is_printed_and_exits_0() {
    let repo = project_with_a_medium_finding();
    run(&["check", "unused-return-values"], repo.path())
        .code(0)
        .stdout(predicate::str::contains("(unused-return-values)"));
}

#[test]
fn a_finding_at_the_deny_level_exits_1_and_is_still_printed() {
    let repo = project_with_a_medium_finding();
    run(
        &["check", "unused-return-values", "--deny", "medium"],
        repo.path(),
    )
    .code(1)
    .stdout(predicate::str::contains("(unused-return-values)"));
}

#[test]
fn a_finding_below_the_deny_level_exits_0() {
    let repo = project_with_a_medium_finding();
    run(
        &["check", "unused-return-values", "--deny", "high"],
        repo.path(),
    )
    .code(0);
}

#[test]
fn a_finding_above_the_deny_level_exits_1() {
    let repo = project_with_a_medium_finding();
    run(
        &["check", "unused-return-values", "--deny", "info"],
        repo.path(),
    )
    .code(1);
}

#[test]
fn a_clean_project_exits_0_even_with_the_strictest_deny() {
    let repo = clean_project();
    run(&["fast", "--deny", "info"], repo.path())
        .code(0)
        .stdout(predicate::str::contains("no issues found"));
}

#[test]
fn a_check_that_could_not_run_exits_3() {
    let repo = project_that_does_not_build();
    run(&["fast"], repo.path())
        .code(3)
        .stderr(predicate::str::contains("check error"));
}

#[test]
fn a_check_that_could_not_run_exits_3_whatever_deny_says() {
    let repo = project_that_does_not_build();
    run(&["fast", "--deny", "critical"], repo.path()).code(3);
}

#[test]
fn an_error_wins_over_a_finding() {
    // A run that could not judge everything must not report "1" (findings) as if it had: the caller would treat the
    // run as complete. Exit 3 says part of the audit did not happen.
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
        (".coderipper.toml", "[[allow]]\ncheck = \"x\"\n"),
    ]);
    run(
        &["check", "unused-return-values", "--deny", "info"],
        repo.path(),
    )
    .code(3);
}

#[test]
fn an_unknown_flag_is_a_usage_error_and_exits_2() {
    let repo = clean_project();
    run(&["fast", "--no-such-flag"], repo.path()).code(2);
}

#[test]
fn an_unknown_deny_level_is_a_usage_error_and_exits_2() {
    let repo = clean_project();
    run(&["fast", "--deny", "severe"], repo.path())
        .code(2)
        .stderr(predicate::str::contains("medium"));
}

#[test]
fn a_project_path_that_does_not_exist_is_a_usage_error_and_exits_2() {
    coderipper()
        .args(["fast", "--project", "/does/not/exist"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("/does/not/exist"));
}

#[test]
fn the_workspace_run_uses_the_same_codes() {
    let repo = project_with_a_medium_finding();
    run(
        &[
            "check",
            "unused-return-values",
            "--workspace",
            "--deny",
            "medium",
        ],
        repo.path(),
    )
    .code(1);
    run(
        &["check", "unused-return-values", "--workspace"],
        repo.path(),
    )
    .code(0);
}

#[test]
fn a_check_id_that_does_not_exist_is_a_usage_error_and_exits_2() {
    // Used to print "no issues found" and exit 0: a typo in a CI step was green forever.
    let repo = clean_project();
    run(&["check", "unused-return-valuse"], repo.path())
        .code(2)
        .stderr(predicate::str::contains(
            "no check named \"unused-return-valuse\"",
        ))
        .stderr(predicate::str::contains("unused-return-values"))
        .stdout(predicate::str::contains("no issues found").not());
}
