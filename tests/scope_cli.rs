#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! `--scope FILE` (audit row C7): one command over the repositories a file lists, each on its own, grouped by repository, the exit
//! code the worst of them, an unreadable repository reported without stopping the others.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A project with one Medium finding from `unused-return-values` (`f`'s result is always discarded).
fn with_a_finding() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ])
}

fn clean() -> tempfile::TempDir {
    common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")])
}

fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

/// `coderipper check unused-return-values --profile classic --scope <list>` plus `extra` arguments.
fn run(list: &std::path::Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    coderipper()
        .args([
            "check",
            "unused-return-values",
            "--profile",
            "classic",
            "--scope",
        ])
        .arg(list)
        .args(extra)
        .assert()
}

fn list_of(lines: &[String]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("portfolio.list");
    std::fs::write(&file, lines.join("\n")).unwrap();
    (dir, file)
}

#[test]
fn each_listed_repository_is_run_grouped_and_the_exit_code_is_the_worst() {
    let (good, bad) = (clean(), with_a_finding());
    let (_dir, list) = list_of(&[
        "# the portfolio".to_string(),
        good.path().display().to_string(),
        String::new(),
        format!("{}  # has a finding", bad.path().display()),
        // a repeated path runs once
        good.path().display().to_string(),
    ]);
    run(&list, &[])
        .code(1)
        .stdout(predicate::str::contains("(1 of 2) =="))
        .stdout(predicate::str::contains("(2 of 2) =="))
        .stdout(predicate::str::contains("(unused-return-values)"))
        .stdout(predicate::str::contains(
            "scope: 2 repositories, 1 clean, 1 with findings, 0 could not be run",
        ));
}

#[test]
fn json_marks_each_repository_and_ends_with_a_scope_summary() {
    let (good, bad) = (clean(), with_a_finding());
    let (_dir, list) = list_of(&[
        good.path().display().to_string(),
        bad.path().display().to_string(),
    ]);
    let out = coderipper()
        .args([
            "check",
            "unused-return-values",
            "--profile",
            "classic",
            "--message-format",
            "json",
            "--scope",
        ])
        .arg(&list)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let reasons: Vec<&str> = lines
        .iter()
        .map(|l| l["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        [
            "coderipper-scope",
            "coderipper-summary",
            "coderipper-scope",
            "coderipper-finding",
            "coderipper-summary",
            "coderipper-scope-summary",
        ],
        "{lines:?}"
    );
    let last = lines.last().unwrap();
    assert_eq!(last["repositories"], 2);
    assert_eq!(
        (last["clean"].as_u64(), last["with_findings"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(last["exit_code"], 1);
}

#[test]
fn a_repository_that_cannot_be_read_is_reported_and_the_others_still_run() {
    let bad = with_a_finding();
    let missing = bad.path().join("no-such-directory");
    let (_dir, list) = list_of(&[
        missing.display().to_string(),
        bad.path().display().to_string(),
    ]);
    run(&list, &[])
        .code(3)
        .stderr(predicate::str::contains("no-such-directory"))
        // the second repository was still analysed
        .stdout(predicate::str::contains("(unused-return-values)"))
        .stdout(predicate::str::contains(
            "1 with findings, 1 could not be run",
        ));
}

#[test]
fn a_relative_path_is_read_from_the_directory_of_the_list() {
    let parent = tempfile::tempdir().unwrap();
    let repo = parent.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let made = clean();
    // a copy of a clean project made in place, so the path can be relative
    for entry in ["Cargo.toml", "src/main.rs"] {
        let to = repo.join(entry);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(made.path().join(entry), to).unwrap();
    }
    for args in [
        &["init", "-q"][..],
        &["add", "-A"],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_CONFIG_GLOBAL", "")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap()
            .success());
    }
    let list = parent.path().join("list.txt");
    std::fs::write(&list, "repo\n").unwrap();
    // run from an unrelated directory: the path is the list's, not the current directory's
    let elsewhere = tempfile::tempdir().unwrap();
    coderipper()
        .current_dir(elsewhere.path())
        .args([
            "check",
            "unused-return-values",
            "--profile",
            "classic",
            "--scope",
        ])
        .arg(&list)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("scope: 1 repositories, 1 clean"));
}

#[test]
fn an_empty_or_unreadable_list_and_scope_with_project_are_usage_errors() {
    let (_dir, empty) = list_of(&["# nothing".to_string(), String::new()]);
    run(&empty, &[])
        .code(2)
        .stderr(predicate::str::contains("lists no repository"));
    let dir = tempfile::tempdir().unwrap();
    run(&dir.path().join("absent.list"), &[])
        .code(2)
        .stderr(predicate::str::contains("cannot read"));
    let repo = clean();
    run(&empty, &["--project", &repo.path().display().to_string()]).code(2);
}
