#![cfg(feature = "cli")]
//! `--profile portfolio`: the language-neutral rules without the delegated tool rules, failing the run by a floor per rule instead of
//! one `--deny`. The cases use a Python project (a `requirements.txt`) so only the rules under test have anything to say.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

fn project(requirements: &str) -> tempfile::TempDir {
    common::git_repo_with(&[("requirements.txt", requirements)])
}

fn portfolio(project: &tempfile::TempDir, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(["fast", "--profile", "portfolio"])
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
fn a_project_with_nothing_the_index_lists_passes() {
    portfolio(&project("requests==2.32.0\nnumpy\n"), &[])
        .assert()
        .code(0)
        .stderr(predicate::str::contains("0 matched"));
}

#[test]
fn a_dependency_on_an_avoid_entry_blocks() {
    portfolio(&project("requests\ngensim>=4\n"), &[])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("(patent-index-match)"))
        .stdout(predicate::str::contains("index match"));
}

#[test]
fn flag_for_review_and_conditions_entries_report_but_do_not_block() {
    // openhands-ai is flag-for-review (Medium), z3-solver is allow-with-conditions (Low): both are shown, neither fails the run
    portfolio(&project("openhands-ai\nz3-solver\n"), &[])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("openhands-ai"))
        .stdout(predicate::str::contains("z3-solver"));
}

#[test]
fn deny_none_still_never_fails_on_a_finding() {
    portfolio(&project("gensim\n"), &["--deny", "none"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(patent-index-match)"));
}

#[test]
fn the_delegated_tool_rules_are_not_part_of_the_profile() {
    portfolio(&project("requests\n"), &["--help"])
        .assert()
        .success();
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(["check", "SUP-002", "--profile", "portfolio", "--project"])
        .arg(project("requests\n").path())
        .assert()
        .code(2);
}

#[test]
fn the_floors_block_mechanical_rules_and_leave_expectation_rules_advisory() {
    use coderipper::finding::Severity;
    use coderipper::portfolio_floor;
    assert_eq!(portfolio_floor("SUP-004"), Some(Severity::Medium));
    assert_eq!(portfolio_floor("conflict-marker"), Some(Severity::Low));
    assert_eq!(portfolio_floor("patent-index-match"), Some(Severity::High));
    // positive control above: the table answers; these are advisory
    for advisory in [
        "DOC-005",
        "DOC-009",
        "SUP-011",
        "lint-suppression",
        "reachability",
    ] {
        assert_eq!(portfolio_floor(advisory), None, "{advisory}");
    }
}

#[test]
fn a_rule_not_yet_ruled_blocking_reports_without_failing() {
    // WSP-001 (no CODEOWNERS) is a Medium finding on this project, shown and not blocking
    portfolio(&project("requests\n"), &[])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(WSP-001)"));
}

fn with_blocking(requirements: &str, config: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        ("requirements.txt", requirements),
        (".coderipper.toml", config),
    ])
}

#[test]
fn a_repository_can_make_a_rule_blocking_for_itself() {
    // WSP-001 (no CODEOWNERS) is advisory by default; this repository has opted in, so it now fails the run
    portfolio(
        &with_blocking("requests\n", "blocking = [\"WSP-001\"]\n"),
        &[],
    )
    .assert()
    .code(1)
    .stdout(predicate::str::contains("(WSP-001)"));
}

#[test]
fn blocking_names_a_rule_that_exists() {
    portfolio(
        &with_blocking("requests\n", "blocking = [\"WSP-999\"]\n"),
        &[],
    )
    .assert()
    .code(2)
    .stderr(predicate::str::contains("no rule of --profile portfolio"));
}
