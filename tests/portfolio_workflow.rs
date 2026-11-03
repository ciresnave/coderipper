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

#[test]
fn the_job_has_a_timeout_a_caller_can_change_and_it_defaults_to_thirty_minutes() {
    // a portfolio job that hangs holds its runner for GitHub's default of 360 minutes (a slow job held both Humboldt runners for 40);
    // the file may have CRLF line endings, so the text is normalised before it is searched
    let w = WORKFLOW.replace("\r\n", "\n");
    let from = w
        .find("      timeout-minutes:\n")
        .expect("the workflow declares a timeout-minutes input");
    let input = &w[from..];
    let input = &input[..input.find("\n\n").unwrap_or(input.len())];
    assert!(input.contains("type: number"), "{input}");
    assert!(input.contains("default: 30"), "{input}");
    // and the job uses it: the line sits under the `portfolio` job, before its steps
    let job = &w[w.find("\njobs:\n").expect("jobs")..];
    let before_steps = &job[..job.find("    steps:").expect("the job has steps")];
    assert!(
        before_steps.contains("    timeout-minutes: ${{ inputs.timeout-minutes }}\n"),
        "{before_steps}"
    );
}

#[test]
fn no_step_walks_the_home_directory_unbounded_or_on_a_self_hosted_runner() {
    // Humboldt run 41550578993: `du -sm $HOME/.cargo` on a self-hosted runner (the machine user's cargo home is huge) sat 28 minutes and
    // held the runner. A step may run `du` or `find` only bounded by `timeout`, and over `$HOME` or `~` only on a GitHub-hosted runner.
    let w = WORKFLOW.replace("\r\n", "\n");
    let mut steps = 0;
    for step in w.split("\n      - name: ").skip(1) {
        let name = step.lines().next().unwrap_or_default();
        let hosted_only = step.lines().any(|l| {
            l.trim_start().starts_with("if:") && l.contains("runner.environment == 'github-hosted'")
        });
        for line in step.lines() {
            let code = line.split('#').next().unwrap_or_default();
            let walks = code.split([';', '|', '&']).map(str::trim).any(|cmd| {
                let cmd = cmd.strip_prefix("timeout 60 ").unwrap_or(cmd);
                cmd.starts_with("du ") || cmd.starts_with("find ")
            });
            if !walks {
                continue;
            }
            steps += 1;
            assert!(
                code.contains("timeout 60 du") || code.contains("timeout 60 find"),
                "step {name:?} runs du or find without `timeout 60`: {line}"
            );
            if code.contains("$HOME") || code.contains("~/") || code.contains("${HOME}") {
                assert!(
                    hosted_only,
                    "step {name:?} measures $HOME without being limited to a GitHub-hosted runner: {line}"
                );
            }
        }
    }
    // positive control: the two bounded steps that remain were found, so the scan reads the steps it is meant to
    assert_eq!(steps, 2, "expected the two bounded du steps");
}
