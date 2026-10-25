#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The release `portfolio.yml` installs must track the release that was published.
//!
//! The workflow names one version (`CODERIPPER_VERSION`) and installs exactly that from crates.io. When it was left at 0.9.6 while 0.9.7 to 0.9.14
//! were published, no workflow SHA could reach the fixes since. Two things keep it honest:
//!
//! - **always, offline:** the version the workflow names is well formed and is never ahead of `Cargo.toml` (it could not be installed);
//! - **at release time, online:** it equals the newest version crates.io lists. The scheduled workflow `release-drift.yml` sets
//!   `CODERIPPER_RELEASE_CHECK=1` and runs this; a run without the variable skips only the network read.
//!
//! A version in `Cargo.toml` may be ahead of both (docs-only and workflow-only changes are not published), so the workflow is compared with what is
//! *published*, not with the manifest.

const WORKFLOW: &str = include_str!("../.github/workflows/portfolio.yml");
const MANIFEST: &str = include_str!("../Cargo.toml");

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// The version the workflow installs: the value of its `CODERIPPER_VERSION:` line.
fn workflow_version(workflow: &str) -> Option<String> {
    workflow.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("CODERIPPER_VERSION:")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn manifest_version(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("version")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim();
        Some(rest.trim_matches('"').to_string())
    })
}

/// The workflow must name exactly the newest published version: behind means the fixes since are unreachable from CI, ahead means it cannot be installed.
fn drift(workflow: &str, newest_published: &str) -> Result<(), String> {
    let w = parse(workflow)
        .ok_or_else(|| format!("the workflow's CODERIPPER_VERSION `{workflow}` is not x.y.z"))?;
    let n = parse(newest_published)
        .ok_or_else(|| format!("the newest published version `{newest_published}` is not x.y.z"))?;
    match w.cmp(&n) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Less => Err(format!(
            "portfolio.yml installs {workflow} but {newest_published} is published: every fix since is unreachable from CI; bump CODERIPPER_VERSION and repin the callers"
        )),
        std::cmp::Ordering::Greater => Err(format!(
            "portfolio.yml installs {workflow}, which crates.io does not list (newest is {newest_published}): the install step would fail"
        )),
    }
}

/// The newest non-yanked version in the crates.io sparse index body for the crate.
fn newest_in_index(body: &str) -> Option<String> {
    body.lines()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            if v["yanked"].as_bool().unwrap_or(false) {
                return None;
            }
            let vers = v["vers"].as_str()?.to_string();
            parse(&vers).map(|p| (p, vers))
        })
        .max_by_key(|(p, _)| *p)
        .map(|(_, v)| v)
}

#[test]
fn the_workflow_names_a_well_formed_version_that_the_manifest_has_reached() {
    let w = workflow_version(WORKFLOW).expect("portfolio.yml has a CODERIPPER_VERSION line");
    let m = manifest_version(MANIFEST).expect("Cargo.toml has a version");
    assert!(parse(&w).is_some(), "CODERIPPER_VERSION `{w}` is not x.y.z");
    assert!(
        parse(&w) <= parse(&m),
        "portfolio.yml names {w}, ahead of Cargo.toml's {m}: it could not be installed"
    );
}

#[test]
fn a_workflow_behind_the_newest_published_release_is_drift_and_one_ahead_cannot_install() {
    assert!(drift("0.9.14", "0.9.14").is_ok());
    // the control: the drift that happened, 0.9.6 against 0.9.14, fails and says what to do
    let behind = drift("0.9.6", "0.9.14").unwrap_err();
    assert!(
        behind.contains("unreachable from CI") && behind.contains("repin"),
        "{behind}"
    );
    let ahead = drift("0.9.15", "0.9.14").unwrap_err();
    assert!(ahead.contains("does not list"), "{ahead}");
    assert!(drift("not-a-version", "0.9.14").is_err());
}

#[test]
fn the_newest_published_version_is_read_from_the_index_and_a_yanked_one_is_not_newest() {
    let body = concat!(
        "{\"name\":\"coderipper\",\"vers\":\"0.9.9\",\"yanked\":false}\n",
        "{\"name\":\"coderipper\",\"vers\":\"0.9.14\",\"yanked\":false}\n",
        "{\"name\":\"coderipper\",\"vers\":\"0.9.20\",\"yanked\":true}\n",
        "{\"name\":\"coderipper\",\"vers\":\"0.10.0\",\"yanked\":false}\n",
    );
    assert_eq!(newest_in_index(body).as_deref(), Some("0.10.0"));
    assert_eq!(newest_in_index("").as_deref(), None);
}

#[test]
fn the_workflow_installs_the_newest_published_release() {
    // the network read runs in the scheduled `release-drift.yml`, which sets this; a developer's `cargo test` does not reach crates.io
    if std::env::var("CODERIPPER_RELEASE_CHECK").as_deref() != Ok("1") {
        return;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let mut response = agent
        .get("https://index.crates.io/co/de/coderipper")
        .call()
        .expect("the crates.io sparse index answers");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("the index body is text");
    let newest = newest_in_index(&body).expect("the index lists a version");
    let w = workflow_version(WORKFLOW).expect("portfolio.yml has a CODERIPPER_VERSION line");
    if let Err(why) = drift(&w, &newest) {
        panic!("{why}");
    }
}

#[test]
fn the_workflow_checks_that_the_installed_binary_is_the_pinned_release() {
    // the control against a caller whose pinned SHA installed one release while the file said another: the step compares `--version` with
    // the pin and fails the job, so removing it (or comparing with something else) is caught here
    let step = WORKFLOW
        .split("      - name: ")
        .find(|s| s.starts_with("The installed coderipper is the pinned release"))
        .expect("portfolio.yml has the step that checks the installed version");
    assert!(step.contains("coderipper --version"), "{step}");
    assert!(
        step.contains("\"coderipper $CODERIPPER_VERSION\"") && step.contains("exit 1"),
        "{step}"
    );
}
