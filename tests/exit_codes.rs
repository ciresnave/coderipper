#![cfg(feature = "cli")]
//! The CLI's exit codes, which are a promise to CI: 0 no finding at or above `--deny` (and every check ran), 1 a finding at
//! or above `--deny` that no `[[allow]]` entry waives, 2 a usage error, 3 a check could not run. Since 0.5.0 `--deny` defaults
//! to `info`, so any finding fails the run; `--deny none` restores "print, never fail", and `--deny medium` lets Low and Info through.

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
fn a_finding_without_deny_fails_the_run_and_is_printed() {
    let repo = project_with_a_medium_finding();
    run(&["check", "unused-return-values"], repo.path())
        .code(1)
        .stdout(predicate::str::contains("(unused-return-values)"));
}

#[test]
fn deny_none_prints_the_finding_and_exits_0() {
    let repo = project_with_a_medium_finding();
    run(
        &["check", "unused-return-values", "--deny", "none"],
        repo.path(),
    )
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
    .code(1);
    run(
        &[
            "check",
            "unused-return-values",
            "--workspace",
            "--deny",
            "none",
        ],
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

// ---- waivers: `[[allow]]` with a reason (0.5.0) ----

const SRC: &str = "fn f() -> i32 { 1 }\nfn main() { f(); }\n";

/// The medium-finding project plus a `.coderipper.toml` holding `allow` (the text of one or more `[[allow]]` entries).
fn project_with_allow(allow: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", SRC),
        (".coderipper.toml", allow),
    ])
}

fn entry(extra: &str) -> String {
    format!(
        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nreason = \"kept on purpose, see issue 7\"\n{extra}"
    )
}

/// The line `unused-return-values` reports the finding at, read from the unwaived run's JSON.
fn reported_line() -> u64 {
    let repo = project_with_a_medium_finding();
    let out = coderipper()
        .args(["check", "unused-return-values", "--deny", "none"])
        .args(["--message-format", "json", "--project"])
        .arg(repo.path())
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["reason"] == "coderipper-finding")
        .expect("one finding")["location"]["line"]
        .as_u64();
    line.expect("the finding names a line")
}

#[test]
fn a_waived_finding_passes_and_shows_its_reason() {
    let repo = project_with_allow(&entry(""));
    run(&["check", "unused-return-values"], repo.path())
        .code(0)
        .stdout(predicate::str::contains("[waived"))
        .stdout(predicate::str::contains("kept on purpose, see issue 7"))
        .stdout(predicate::str::contains("(unused-return-values)"));
}

#[test]
fn a_waiver_with_a_symbol_still_has_to_name_that_symbol() {
    // `f` is the discarded function; `main` is not what the finding is about.
    let right = project_with_allow(&entry("symbol = \"f\"\n"));
    run(&["check", "unused-return-values"], right.path()).code(0);
    let wrong = project_with_allow(&entry("symbol = \"main\"\n"));
    run(&["check", "unused-return-values"], wrong.path()).code(1);
}

#[test]
fn a_waiver_for_a_line_range_waives_only_findings_inside_it() {
    let line = reported_line();
    let inside = project_with_allow(&entry(&format!("lines = \"{line}\"\n")));
    run(&["check", "unused-return-values"], inside.path()).code(0);
    let range = project_with_allow(&entry(&format!("lines = \"1-{}\"\n", line + 10)));
    run(&["check", "unused-return-values"], range.path()).code(0);
    let outside = project_with_allow(&entry(&format!("lines = \"{}-{}\"\n", line + 1, line + 9)));
    run(&["check", "unused-return-values"], outside.path()).code(1);
}

#[test]
fn a_waiver_for_another_file_or_check_waives_nothing() {
    let other_file = project_with_allow(&entry("").replace("src/main.rs", "src/lib.rs"));
    run(&["check", "unused-return-values"], other_file.path()).code(1);
    let other_check =
        project_with_allow(&entry("").replace("unused-return-values", "reachability"));
    run(&["check", "unused-return-values"], other_check.path()).code(1);
}

#[test]
fn a_waiver_that_matches_nothing_is_reported_and_fails_the_run() {
    // A stale waiver is a finding of the check `allowlist`: under the default it fails like any other.
    let repo = project_with_allow(&entry("symbol = \"long_gone\"\n"));
    run(&["check", "unused-return-values"], repo.path())
        .code(1)
        .stdout(predicate::str::contains("(allowlist)"));
}

#[test]
fn a_malformed_lines_value_is_an_error_not_a_wider_waiver() {
    let repo = project_with_allow(&entry("lines = \"20-10\"\n"));
    run(&["check", "unused-return-values"], repo.path())
        .code(3)
        .stderr(predicate::str::contains("lines"));
}

#[test]
fn json_shows_a_waived_finding_with_its_reason_and_counts_it() {
    let repo = project_with_allow(&entry(""));
    let out = coderipper()
        .args(["check", "unused-return-values"])
        .args(["--message-format", "json", "--project"])
        .arg(repo.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let lines: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let waived: Vec<_> = lines
        .iter()
        .filter(|v| v["reason"] == "coderipper-waived")
        .collect();
    assert_eq!(waived.len(), 1, "{lines:?}");
    assert_eq!(waived[0]["waiver_reason"], "kept on purpose, see issue 7");
    assert_eq!(waived[0]["check_id"], "unused-return-values");
    assert!(
        lines.iter().all(|v| v["reason"] != "coderipper-finding"),
        "a waived finding is not also a finding: {lines:?}"
    );
    let summary = lines.last().unwrap();
    assert_eq!(summary["reason"], "coderipper-summary");
    assert_eq!(summary["findings"], 0);
    assert_eq!(summary["waived"], 1);
    assert_eq!(summary["exit_code"], 0);
}

#[test]
fn a_clean_run_reports_no_waivers() {
    let repo = clean_project();
    let out = coderipper()
        .args(["fast", "--message-format", "json", "--project"])
        .arg(repo.path())
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"waived\":0"), "{text}");
    assert!(!text.contains("coderipper-waived"), "{text}");
}

#[test]
fn the_workspace_run_shows_a_waiver_under_its_member() {
    let repo = project_with_allow(&entry(""));
    // the member is the cargo package (`fixture`), not the temporary directory the project sits in
    run(
        &["check", "unused-return-values", "--workspace"],
        repo.path(),
    )
    .code(0)
    .stdout(predicate::str::contains("[waived/Medium] fixture "));
}

#[test]
fn a_run_whose_only_findings_are_waived_does_not_say_no_issues_found() {
    let repo = project_with_allow(&entry(""));
    run(&["check", "unused-return-values"], repo.path())
        .code(0)
        .stdout(predicate::str::contains("no findings (1 waived"))
        .stdout(predicate::str::contains("no issues found").not());
}

#[test]
fn a_waiver_reason_with_a_newline_stays_on_one_line() {
    let repo = project_with_allow(
        &entry("").replace("kept on purpose, see issue 7", "first line\\nsecond line"),
    );
    let out = coderipper()
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text
        .lines()
        .find(|l| l.starts_with("[waived"))
        .expect("a waived line");
    assert!(line.contains("first line second line"), "{text}");
}
