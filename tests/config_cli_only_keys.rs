// SPDX-License-Identifier: MIT OR Apache-2.0
//! A key only the command-line program reads, in a repository's `.coderipper.toml`. Without the `cli` feature the library accepts
//! it and does not look at it; with the feature a wrong value is an error. A key nobody reads is an error in both builds.
//! CI runs this file with `--no-default-features` as well as with the defaults.

mod common;

use coderipper::check::{CheckContext, Tier};
use coderipper::{run_checks, RunResult};
#[path = "common/manifest.rs"]
mod manifest;
use common::git_repo_with;
use manifest::MANIFEST;

const URV: &str = "unused-return-values";

fn run_with(config: &str) -> RunResult {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn f() -> i32 { 1 }
fn main() { f(); }
",
        ),
        (".coderipper.toml", config),
    ]);
    run_checks(
        &CheckContext::new(repo.path().to_path_buf()),
        Tier::Fast,
        Some(URV),
    )
}

fn config_errors(result: &RunResult) -> usize {
    result
        .errors
        .iter()
        .filter(|e| e.contains("allowlist"))
        .count()
}

fn finding_count(result: &RunResult) -> usize {
    result.findings.iter().filter(|f| f.check_id == URV).count()
}

#[test]
fn a_valid_cli_only_key_is_accepted_in_both_builds() {
    let result = run_with(
        "redact = true
install-tools = false
[cache]
enabled = false
[tools]
dir = \"t\"
[limits]
max_processes = 4
",
    );
    assert_eq!(config_errors(&result), 0, "{:?}", result.errors);
    assert_eq!(finding_count(&result), 1, "the check still ran");
}

#[test]
fn a_key_nobody_reads_is_an_error_in_both_builds() {
    let result = run_with(
        "redakt = true
",
    );
    assert_eq!(config_errors(&result), 1, "{:?}", result.errors);
    assert_eq!(finding_count(&result), 1, "the findings are not swallowed");
}

#[cfg(feature = "cli")]
#[test]
fn with_the_cli_a_cli_only_key_with_a_wrong_value_is_an_error() {
    let result = run_with(
        "redact = \"yes\"
",
    );
    assert_eq!(config_errors(&result), 1, "{:?}", result.errors);
}

#[cfg(not(feature = "cli"))]
#[test]
fn without_the_cli_a_cli_only_key_is_not_looked_at() {
    let result = run_with(
        "redact = \"yes\"
[limits]
max_cpus = 1
",
    );
    assert_eq!(config_errors(&result), 0, "{:?}", result.errors);
}
