#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! `--summary`: per-rule counts for a run, in the human output and as one JSON line with stable field names.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

/// A Python project with no CODEOWNERS and no licence file: `WSP-001` and `licence` each find one thing, and nothing is built.
fn project() -> tempfile::TempDir {
    common::git_repo_with(&[("requirements.txt", "requests==2.32.0\n")])
}

fn run(project: &tempfile::TempDir, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args([
        "fast",
        "--profile",
        "portfolio",
        "--deny",
        "none",
        "--no-config",
    ])
    .args(extra)
    .arg("--project")
    .arg(project.path())
    .env("CODERIPPER_CACHE", "off")
    .env(
        "CODERIPPER_TOOLS_DIR",
        std::env::temp_dir().join("coderipper-test-no-tools-here"),
    );
    cmd
}

#[test]
fn without_the_flag_no_summary_is_printed() {
    run(&project(), &[])
        .assert()
        .stdout(predicate::str::contains("summary ").not());
    let out = run(&project(), &["--message-format", "json"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("coderipper-rule-summary"), "{text}");
}

#[test]
fn the_human_summary_counts_findings_per_rule() {
    run(&project(), &["--summary"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("finding(s)"))
        .stdout(predicate::str::contains("opt-out(s)"))
        .stdout(predicate::str::contains("WSP-001"));
}

#[test]
fn the_json_summary_is_one_line_before_the_final_summary_with_stable_fields() {
    let out = run(&project(), &["--summary", "--message-format", "json"])
        .output()
        .unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let reasons: Vec<&str> = lines
        .iter()
        .map(|l| l["reason"].as_str().unwrap_or(""))
        .collect();
    let at = reasons
        .iter()
        .position(|r| *r == "coderipper-rule-summary")
        .expect("a rule summary line");
    assert_eq!(reasons.last(), Some(&"coderipper-summary"));
    assert_eq!(at + 1, reasons.len() - 1, "just before the final summary");
    let s = &lines[at];
    for key in [
        "project",
        "findings",
        "waived",
        "declared_public",
        "opt_outs",
        "errors",
        "rules",
    ] {
        assert!(s.get(key).is_some(), "{key} missing from {s}");
    }
    let wsp = s["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["rule"] == "WSP-001")
        .expect("WSP-001 row");
    assert_eq!(wsp["findings"], 1);
    for key in ["waived", "declared_public", "severities"] {
        assert!(wsp.get(key).is_some(), "{key} missing from {wsp}");
    }
    // the totals are the rows' sums
    let total: u64 = s["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["findings"].as_u64().unwrap())
        .sum();
    assert_eq!(s["findings"].as_u64().unwrap(), total);
}
