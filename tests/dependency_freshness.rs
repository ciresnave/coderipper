// SPDX-License-Identifier: MIT OR Apache-2.0
//! `dependency-freshness` against canned `cargo update --dry-run --verbose` output, so no network is involved. The text is
//! the shape cargo 1.99 prints (checked against this repository's own lockfile, 2026-10-24).

use coderipper::check::{Check, CheckContext, Network, Tier};
use coderipper::checks::dependency_freshness::Cargo;
use coderipper::checks::DependencyFreshnessCheck;
use coderipper::finding::Severity;
use std::collections::BTreeSet;
use std::path::Path;

struct Fake {
    dry_run: Result<String, String>,
    direct: Vec<&'static str>,
}

impl Cargo for Fake {
    fn dry_run(&self, _dir: &Path) -> anyhow::Result<String> {
        self.dry_run.clone().map_err(|e| anyhow::anyhow!(e))
    }

    fn direct_dependencies(&self, _dir: &Path) -> anyhow::Result<BTreeSet<String>> {
        Ok(self.direct.iter().map(|s| s.to_string()).collect())
    }
}

const BEHIND: &str = "\
    Updating crates.io index
     Locking 3 packages to highest compatible versions
      Adding bumpalo v3.20.3
    Updating libc v0.2.190 -> v0.2.191
    Updating serde_json v1.0.151 -> v1.0.154
 Unchanged ureq v2.0.0 (available: v3.5.0)
warning: not updating lockfile due to dry run
";

fn project(lock: bool) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    if lock {
        std::fs::write(tmp.path().join("Cargo.lock"), "version = 4\n").unwrap();
    }
    tmp
}

fn run(
    dir: &Path,
    dry_run: Result<&str, &str>,
    direct: Vec<&'static str>,
) -> anyhow::Result<Vec<coderipper::finding::Finding>> {
    let fake = Fake {
        dry_run: dry_run.map(str::to_string).map_err(str::to_string),
        direct,
    };
    DependencyFreshnessCheck::with_cargo(Box::new(fake)).run(&CheckContext::new(dir))
}

#[test]
fn it_is_a_network_tier_check() {
    let check = DependencyFreshnessCheck::new();
    assert_eq!(check.network(), Network::NetworkRequired);
    assert_eq!(check.network().tier(), Tier::Sweep);
}

#[test]
fn each_package_behind_is_one_finding_and_adding_is_not_staleness() {
    let tmp = project(true);
    let found = run(tmp.path(), Ok(BEHIND), vec!["serde_json", "ureq"]).unwrap();
    let got: Vec<_> = found
        .iter()
        .map(|f| (f.subject.as_deref().unwrap(), f.severity))
        .collect();
    assert_eq!(
        got,
        vec![
            ("libc", Severity::Low),
            ("serde_json", Severity::Medium),
            ("ureq", Severity::Medium),
        ]
    );
    assert!(found
        .iter()
        .all(|f| f.location.as_ref().unwrap().file == "Cargo.lock"));
    assert!(found[2].summary.contains("2.0.0") && found[2].summary.contains("3.5.0"));
    assert!(
        found[2].detail.contains("requirement"),
        "{}",
        found[2].detail
    );
}

#[test]
fn a_fresh_lockfile_is_empty_and_a_cargo_failure_is_an_error() {
    let tmp = project(true);
    let quiet = "    Updating crates.io index\nwarning: not updating lockfile due to dry run\n";
    assert!(run(tmp.path(), Ok(quiet), vec![]).unwrap().is_empty());
    // the control: the same check does report on the stale text
    assert!(!run(tmp.path(), Ok(BEHIND), vec![]).unwrap().is_empty());

    let err = run(tmp.path(), Err("network down"), vec![]).unwrap_err();
    assert!(err.to_string().contains("network down"), "{err}");
    // output with no sign that cargo finished must not read as fresh
    let err = run(tmp.path(), Ok("error: nope\n"), vec![]).unwrap_err();
    assert!(err.to_string().contains("no sign"), "{err}");
}

#[test]
fn no_lockfile_is_an_info_finding_not_a_clean_run() {
    let tmp = project(false);
    let found = run(tmp.path(), Ok(BEHIND), vec![]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Info);
    assert!(found[0].positive_control.is_some());
}
