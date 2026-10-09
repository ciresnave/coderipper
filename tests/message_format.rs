#![cfg(feature = "cli")]
//! `--message-format json`: one JSON object per line on stdout, each with a `"reason"` (the way cargo's own
//! `--message-format json` works), so a tool can read findings without parsing the human text. The golden tests pin the
//! exact key sets: a field renamed or removed is a breaking change for everyone reading the stream.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use serde_json::Value;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn project_with_a_medium_finding() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ])
}

fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

/// Runs the CLI and returns (exit code, stdout lines parsed as JSON). Every stdout line must be JSON.
fn run_json(args: &[&str], project: &std::path::Path) -> (i32, Vec<Value>) {
    let out = coderipper()
        .args(args)
        .args(["--message-format", "json", "--project"])
        .arg(project)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines = stdout
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap_or_else(|e| panic!("not JSON: {l:?}: {e}"))
        })
        .collect();
    (out.status.code().unwrap(), lines)
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

#[test]
fn a_finding_is_one_json_line_followed_by_a_summary_line() {
    let repo = project_with_a_medium_finding();
    let (code, lines) = run_json(&["check", "unused-return-values"], repo.path());
    assert_eq!(code, 1, "a finding fails the run by default (0.5.0)");
    assert_eq!(lines.len(), 2, "{lines:?}");
    let finding = &lines[0];
    assert_eq!(finding["reason"], "coderipper-finding");
    assert_eq!(finding["check_id"], "unused-return-values");
    assert_eq!(finding["severity"], "medium");
    assert_eq!(finding["subject"], "f");
    assert_eq!(finding["location"]["file"], "src/main.rs");
    let summary = &lines[1];
    assert_eq!(summary["reason"], "coderipper-summary");
    assert_eq!(summary["findings"], 1);
    assert_eq!(summary["errors"], serde_json::json!([]));
    assert_eq!(summary["waived"], 0);
    assert_eq!(summary["exit_code"], 1);
}

#[test]
fn golden_the_keys_of_a_finding_line_and_of_the_summary_line() {
    let repo = project_with_a_medium_finding();
    let (_, lines) = run_json(&["check", "unused-return-values"], repo.path());
    assert_eq!(
        keys(&lines[0]),
        GOLDEN_FINDING_KEYS,
        "the finding line's keys are a contract"
    );
    assert_eq!(keys(&lines[0]["location"]), ["file", "line"]);
    assert_eq!(
        keys(&lines[1]),
        ["errors", "exit_code", "findings", "reason", "waived"],
        "`waived` is the 0.5.0 schema addition"
    );
}

/// A finding line is the `Finding` fields plus `reason`. `positive_control` is `null` when absent; `member` is left out
/// unless a `--workspace` run set it.
const GOLDEN_FINDING_KEYS: [&str; 10] = [
    "check_id",
    "confidence",
    "detail",
    "location",
    "positive_control",
    "project",
    "reason",
    "severity",
    "subject",
    "summary",
];

#[test]
fn the_summary_carries_the_exit_code_the_process_returns() {
    let repo = project_with_a_medium_finding();
    let (code, lines) = run_json(
        &["check", "unused-return-values", "--deny", "medium"],
        repo.path(),
    );
    assert_eq!(code, 1);
    assert_eq!(lines.last().unwrap()["exit_code"], 1);
}

#[test]
fn a_clean_run_prints_only_the_summary_never_the_human_sentence() {
    let repo =
        common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")]);
    let (code, lines) = run_json(&["fast"], repo.path());
    assert_eq!(code, 0);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["reason"], "coderipper-summary");
    assert_eq!(lines[0]["findings"], 0);
}

#[test]
fn a_check_that_could_not_run_is_in_the_summary_and_exits_3() {
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "this is not valid rust {{{\n"),
    ]);
    let (code, lines) = run_json(&["fast"], repo.path());
    assert_eq!(code, 3);
    let summary = lines.last().unwrap();
    assert_eq!(summary["reason"], "coderipper-summary");
    assert_eq!(summary["exit_code"], 3);
    let errors = summary["errors"].as_array().unwrap();
    assert!(!errors.is_empty());
    assert!(errors[0].as_str().unwrap().contains("reachability"));
}

#[test]
fn an_unknown_message_format_is_a_usage_error() {
    let repo = project_with_a_medium_finding();
    coderipper()
        .args([
            "fast",
            "--deny",
            "none",
            "--message-format",
            "yaml",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(2);
}

#[test]
fn the_default_format_is_still_the_human_text() {
    let repo = project_with_a_medium_finding();
    coderipper()
        .args([
            "check",
            "unused-return-values",
            "--deny",
            "none",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stdout(predicates::str::contains("[Medium/Medium]"));
}
