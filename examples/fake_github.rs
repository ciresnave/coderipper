//! Testing a check that talks to GitHub, with no network: hand it a [`Github`] that answers from a table.
//!
//! ```text
//! cargo run --example fake_github
//! ```
//!
//! `CiProtectionPresenceCheck::with_api` takes any `Github`; the real one (`CiProtectionPresenceCheck::new`) shells out
//! to `gh api`. Here the fake says the default branch has no protection, and the check reports it. This needs `git` on
//! your PATH: the check reads the repository's `origin` to know which GitHub repository to ask about.

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::CiProtectionPresenceCheck;
use coderipper::github::{ApiError, Github};
use serde_json::{json, Value};

struct Fake;

impl Github for Fake {
    fn get(&self, path: &str) -> Result<Value, ApiError> {
        match path {
            "repos/acme/widgets" => Ok(json!({ "default_branch": "main", "archived": false })),
            "repos/acme/widgets/branches/main" => Ok(json!({
                "name": "main",
                "commit": { "sha": "b69e34e4eaf803ea4b34405518805cc32b6814ce" },
                "protected": false,
                "protection": {
                    "enabled": false,
                    "required_status_checks": { "enforcement_level": "off", "contexts": [], "checks": [] }
                }
            })),
            // no repository rulesets
            p if p.contains("/rules/branches/") => Ok(json!([])),
            other => Err(ApiError::new(
                Some(404),
                format!("no canned answer for {other}"),
            )),
        }
    }
}

fn git(dir: &std::path::Path, args: &[&str]) -> anyhow::Result<()> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()?;
    anyhow::ensure!(status.success(), "git {args:?} failed");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let project = tempfile::tempdir()?;
    git(project.path(), &["init", "-q"])?;
    git(
        project.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    )?;

    let check: Vec<Box<dyn Check>> = vec![Box::new(CiProtectionPresenceCheck::with_api(Box::new(
        Fake,
    )))];
    let ctx = CheckContext::new(project.path());
    // a network-tier check runs in the sweep tier
    let result = coderipper::run_checks_with(&check, &ctx, Tier::Sweep, None);

    for error in &result.errors {
        eprintln!("check error: {error}");
    }
    for finding in &result.findings {
        println!("{:?}: {}", finding.severity, finding.summary);
    }
    assert!(result.errors.is_empty());
    assert_eq!(result.findings.len(), 1);
    assert!(result.findings[0].summary.contains("no branch protection"));
    Ok(())
}
