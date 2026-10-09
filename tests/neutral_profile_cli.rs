#![cfg(feature = "cli")]
//! The language-neutral rules from the command line (multi-language P3, PR 1): they run only under `--profile extended`, in the
//! same run as the five original checks, with the same allowlist; `classic` is unchanged.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use predicates::prelude::*;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const MAIN: &str = "fn main() {}\n";

/// A clean Rust binary with no lockfile and no CODEOWNERS: the neutral rules SUP-001 and WSP-001 each have something to say.
fn project() -> tempfile::TempDir {
    common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", MAIN)])
}

fn coderipper(project: &tempfile::TempDir, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(args).arg("--project").arg(project.path());
    cmd.env("CODERIPPER_CACHE", "off");
    // no tools anywhere: what these tests say must not depend on what is installed on the machine running them
    cmd.env(
        "CODERIPPER_TOOLS_DIR",
        std::env::temp_dir().join("coderipper-test-no-tools-here"),
    );
    cmd
}

#[test]
fn classic_does_not_run_the_neutral_rules() {
    coderipper(&project(), &["fast"])
        .assert()
        .code(0)
        .stdout("coderipper: no issues found\n");
}

#[test]
fn extended_runs_them_with_the_catalog_ids() {
    coderipper(&project(), &["fast", "--profile", "extended"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(SUP-001)"))
        .stdout(predicate::str::contains("(WSP-001)"));
}

#[test]
fn the_neutral_findings_count_toward_deny() {
    coderipper(
        &project(),
        &["fast", "--profile", "extended", "--deny", "medium"],
    )
    .assert()
    .code(1);
}

#[test]
fn naming_a_neutral_rule_needs_the_extended_profile() {
    coderipper(&project(), &["check", "WSP-001"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("belongs to --profile extended"));
    coderipper(&project(), &["check", "WSP-001", "--profile", "extended"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(WSP-001)"))
        .stdout(predicate::str::contains("(SUP-001)").not());
}

#[test]
fn an_unknown_rule_is_still_a_usage_error_under_extended() {
    coderipper(&project(), &["check", "NOPE-001", "--profile", "extended"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no check named"));
}

#[test]
fn the_allowlist_suppresses_a_neutral_finding_and_is_not_called_stale() {
    let allowlist = "[[allow]]\ncheck = \"WSP-001\"\nfile = \".github/CODEOWNERS\"\nsymbol = \"CODEOWNERS\"\nreason = \"owned by the portfolio\"\n";
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", MAIN),
        (".coderipper.toml", allowlist),
    ]);
    coderipper(&repo, &["fast", "--profile", "extended"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(WSP-001)").not())
        .stdout(predicate::str::contains("(SUP-001)"))
        .stdout(predicate::str::contains("allowlist").not());
}

#[test]
fn an_entry_for_a_neutral_rule_is_stale_when_it_matches_nothing() {
    let allowlist = "[[allow]]\ncheck = \"WSP-001\"\nfile = \".github/CODEOWNERS\"\nsymbol = \"CODEOWNERS\"\nreason = \"x\"\n";
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", MAIN),
        (".github/CODEOWNERS", "* @owner\n"),
        (".coderipper.toml", allowlist),
    ]);
    coderipper(&repo, &["fast", "--profile", "extended"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(allowlist)"));
}

#[test]
fn a_workspace_run_takes_the_neutral_rules_once_not_once_per_member() {
    let root = "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n";
    let member = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    };
    let repo = common::git_repo_with(&[
        ("Cargo.toml", root),
        ("a/Cargo.toml", &member("a")),
        ("a/src/lib.rs", "pub fn a() {}\n"),
        ("b/Cargo.toml", &member("b")),
        ("b/src/lib.rs", "pub fn b() {}\n"),
    ]);
    let out = coderipper(&repo, &["fast", "--workspace", "--profile", "extended"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches("(WSP-001)").count(), 1, "{text}");
}

#[test]
fn the_conformance_command_judges_the_neutral_module() {
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["conformance", "--module", "neutral", "--require-proven"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("SUP-001: proven"))
        .stdout(predicate::str::contains("WSP-001: proven"));
}
