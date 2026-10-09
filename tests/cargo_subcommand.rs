#![cfg(feature = "cli")]
//! `cargo coderipper`: the same program installed as a cargo subcommand, so it runs next to `cargo clippy`. Cargo runs
//! `cargo-coderipper coderipper <args>` (it passes the subcommand's own name first); a bare `cargo coderipper` means
//! `fast`.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn project_with_a_medium_finding() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ])
}

/// `cargo-coderipper` as cargo itself would run it: the subcommand name first, in the project's directory.
fn as_cargo_runs_it(project: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("cargo-coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off")
        .current_dir(project)
        .arg("coderipper");
    cmd
}

#[test]
fn a_bare_invocation_runs_the_fast_checks_in_the_current_directory() {
    let repo = project_with_a_medium_finding();
    // a finding fails the run by default (0.5.0), under `cargo coderipper` as under `coderipper`
    as_cargo_runs_it(repo.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains("(unused-return-values)"));
    as_cargo_runs_it(repo.path())
        .args(["--deny", "none"])
        .assert()
        .code(0)
        .stdout(predicates::str::contains("(unused-return-values)"));
}

#[test]
fn deny_and_the_exit_codes_work_the_same_way() {
    let repo = project_with_a_medium_finding();
    as_cargo_runs_it(repo.path())
        .args(["--deny", "medium"])
        .assert()
        .code(1);
    as_cargo_runs_it(repo.path())
        .args(["--deny", "high"])
        .assert()
        .code(0);
}

#[test]
fn workspace_and_message_format_are_accepted() {
    let repo = project_with_a_medium_finding();
    let out = as_cargo_runs_it(repo.path())
        .args(["--workspace", "--message-format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "a finding fails the run by default"
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let last: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(last["reason"], "coderipper-summary");
    assert_eq!(last["findings"], 1);
}

#[test]
fn an_explicit_subcommand_is_still_honoured() {
    let repo = project_with_a_medium_finding();
    as_cargo_runs_it(repo.path())
        .args(["check", "unused-return-values", "--deny", "medium"])
        .assert()
        .code(1);
}

#[test]
fn it_also_runs_without_the_subcommand_name_cargo_adds() {
    let repo = project_with_a_medium_finding();
    Command::cargo_bin("cargo-coderipper")
        .unwrap()
        .env("CODERIPPER_CACHE", "off")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicates::str::contains("(unused-return-values)"));
}

#[test]
fn a_usage_error_exits_2() {
    let repo = project_with_a_medium_finding();
    as_cargo_runs_it(repo.path())
        .arg("--no-such-flag")
        .assert()
        .code(2);
}

#[test]
fn the_real_cargo_finds_the_subcommand_on_path_and_runs_it() {
    // The end-to-end path: `cargo` looks `cargo-coderipper` up on PATH. This is the proof that the binary's name and
    // the argument convention match what cargo really does, which the tests above only assume.
    let repo = project_with_a_medium_finding();
    let bin_dir: PathBuf = PathBuf::from(env!("CARGO_BIN_EXE_cargo-coderipper"))
        .parent()
        .unwrap()
        .to_path_buf();
    let path = std::env::join_paths(
        std::iter::once(bin_dir).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let out = std::process::Command::new("cargo")
        .args(["coderipper", "--message-format", "json", "--deny", "medium"])
        .env("PATH", path)
        .env("CODERIPPER_CACHE", "off")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let first: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(first["reason"], "coderipper-finding");
}
