#![cfg(feature = "cli")]
//! `coderipper conformance`: runs the fixtures and says which claims are earned. Exit 0 when no fixture contradicts a
//! claim (a claim without a fixture is reported, not failed), 1 when one does, 2 for a usage error, 3 when it could not run.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

fn coderipper() -> Command {
    Command::cargo_bin("coderipper").unwrap()
}

#[test]
fn an_earned_rule_is_reported_proven_and_exits_0() {
    coderipper()
        .args(["conformance", "--rule", "unused-parameters", "--fixtures"])
        .arg(fixtures())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("unused-parameters: proven"));
}

#[test]
fn a_claim_without_a_fixture_is_reported_but_does_not_fail() {
    coderipper()
        .args([
            "conformance",
            "--rule",
            "ci-protection-presence",
            "--fixtures",
        ])
        .arg(fixtures())
        .assert()
        .code(0)
        .stdout(predicate::str::contains(
            "ci-protection-presence: no fixture",
        ));
}

#[test]
fn json_output_has_one_line_per_rule() {
    let out = coderipper()
        .args([
            "conformance",
            "--rule",
            "unused-parameters",
            "--message-format",
            "json",
            "--fixtures",
        ])
        .arg(fixtures())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let value: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(value["reason"], "coderipper-conformance");
    assert_eq!(value["rule"], "unused-parameters");
    assert_eq!(value["verdict"], "proven");
}

#[test]
fn a_fixture_that_contradicts_the_claim_exits_1() {
    // The defective project of a rule, used as its own clean twin: the rule reports in the "clean" project.
    let dir = tempfile::tempdir().unwrap();
    let source = fixtures().join("unused-parameters");
    for kind in ["defective", "clean"] {
        for file in ["Cargo.toml", "src/lib.rs", "expect.toml"] {
            let to = dir.path().join("unused-parameters").join(kind).join(file);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            let from = if kind == "clean" && file == "src/lib.rs" {
                source.join("defective").join(file)
            } else {
                source.join(kind).join(file)
            };
            std::fs::copy(from, to).unwrap();
        }
    }
    coderipper()
        .args(["conformance", "--rule", "unused-parameters", "--fixtures"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("unused-parameters: FAILED"))
        .stdout(predicate::str::contains("clean twin"));
}

#[test]
fn an_unknown_module_is_a_usage_error() {
    coderipper()
        .args(["conformance", "--module", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("nope"));
}

#[test]
fn an_unknown_rule_is_a_usage_error() {
    coderipper()
        .args(["conformance", "--rule", "no-such-rule", "--fixtures"])
        .arg(fixtures())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no-such-rule"));
}

#[test]
fn a_missing_fixtures_directory_is_a_usage_error_not_a_pass() {
    // Run from the wrong directory, a CI job must not go green on "no fixtures found".
    coderipper()
        .args([
            "conformance",
            "--rule",
            "unused-parameters",
            "--fixtures",
            "does-not-exist",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does-not-exist"));
}

#[test]
fn requiring_proof_fails_a_claim_that_is_not_earned() {
    // A CI job that wants every claim earned passes --require-proven, so a missing fixture cannot pass silently.
    coderipper()
        .args([
            "conformance",
            "--rule",
            "ci-protection-presence",
            "--require-proven",
            "--fixtures",
        ])
        .arg(fixtures())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "ci-protection-presence: no fixture",
        ));
    coderipper()
        .args([
            "conformance",
            "--rule",
            "unused-parameters",
            "--require-proven",
            "--fixtures",
        ])
        .arg(fixtures())
        .assert()
        .code(0);
}

#[test]
fn json_names_a_failed_verdict_and_its_reasons() {
    let dir = tempfile::tempdir().unwrap();
    let source = fixtures().join("unused-parameters");
    for kind in ["defective", "clean"] {
        for file in ["Cargo.toml", "src/lib.rs", "expect.toml"] {
            let to = dir.path().join("unused-parameters").join(kind).join(file);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            let from = if kind == "clean" && file == "src/lib.rs" {
                source.join("defective").join(file)
            } else {
                source.join(kind).join(file)
            };
            std::fs::copy(from, to).unwrap();
        }
    }
    let out = coderipper()
        .args([
            "conformance",
            "--rule",
            "unused-parameters",
            "--message-format",
            "json",
            "--fixtures",
        ])
        .arg(dir.path())
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let value: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(value["verdict"], "failed");
    assert!(
        value["notes"][0].as_str().unwrap().contains("clean twin"),
        "{value}"
    );
}
