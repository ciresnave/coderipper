//! The examples are documentation that must keep working: each one is run for real and its output checked.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use std::process::{Command, Output};

fn run_example(name: &str, args: &[&std::ffi::OsStr]) -> Output {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    Command::new(cargo)
        .args(["run", "--quiet", "--example", name, "--"])
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn custom_check_runs_a_users_check_and_its_allowlist_entry_suppresses_it() {
    let out = run_example("custom_check", &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("found: the project keeps a TODO.md (todo-file)"));
    assert!(stdout.contains("after the allowlist entry: 0 findings"));
}

#[test]
fn fake_github_reports_an_unprotected_branch_with_no_network() {
    let out = run_example("fake_github", &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("no branch protection"));
}

#[test]
fn run_on_a_project_prints_the_findings_of_the_project_it_is_given() {
    let repo = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ]);
    let out = run_example("run_on_a_project", &[repo.path().as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("(unused-return-values)"));
}

#[test]
fn run_on_a_project_exits_2_for_a_path_that_does_not_exist() {
    let out = run_example("run_on_a_project", &["/does/not/exist".as_ref()]);
    assert_eq!(out.status.code(), Some(2));
}
