#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The reusable `portfolio.yml` must not die of a full disk without saying so, and must name a check or a tool that could not run.
//!
//! synapse#290 (run 41007729285) built three workspace members into a full disk ("failed to write"), could not install gitleaks ("No space
//! left on device"), and ended with `Process completed with exit code 3`: the reason was one line in thousands. These tests read the workflow
//! text, so they pin that the steps exist and in what order; the behaviour itself is exercised by running the workflow (see the CHANGELOG
//! entry that added them for the runs).

const WORKFLOW: &str = include_str!("../.github/workflows/portfolio.yml");

fn at(needle: &str) -> usize {
    WORKFLOW
        .find(needle)
        .unwrap_or_else(|| panic!("portfolio.yml does not contain {needle:?}"))
}

#[test]
fn the_workflow_frees_disk_before_the_checkout_and_fails_by_name_under_its_floor() {
    assert!(
        at("name: Free disk space") < at("actions/checkout@"),
        "the cleanup must run before the checkout and the install fill the disk"
    );
    assert!(at("MIN_FREE_GB:") < at("name: Free disk space"));
    // the floor is checked twice: after the cleanup and again before the sweep, each failing with a message that starts "disk:"
    assert_eq!(WORKFLOW.matches("::error::disk:").count(), 2);
    assert!(
        at("name: Enough free disk for the sweep")
            < at("name: coderipper sweep --profile portfolio")
    );
}

#[test]
fn the_workflow_names_a_check_that_failed_to_run_and_a_tool_that_would_not_install() {
    let sweep = &WORKFLOW[at("name: coderipper sweep --profile portfolio")..];
    // the secrets scan's exit code is kept apart and never lowered by the sweep's
    assert!(sweep.contains("secrets_code=$?"));
    assert!(sweep.contains("check error: .* failed to run"));
    for code in [
        "tool_install_failed",
        "tool_missing",
        "checksum_mismatch",
        "download_failed",
        "consent_not_given",
    ] {
        assert!(
            sweep.contains(code),
            "the tool failure code {code} is not named"
        );
    }
    // an error line with a zero exit code still fails the job
    assert!(sweep.contains("[ \"$failed\" -ne 0 ] && [ \"$code\" -eq 0 ]"));
}
