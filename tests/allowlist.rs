//! Host-level allowlist behaviour: suppression is applied by `run_checks`, not by each check, so
//! the host can see which entries matched nothing ("stale this run", design doc §4).

mod common;

use coderipper::check::{CheckContext, Tier};
use coderipper::finding::{Finding, Severity};
use coderipper::{run_checks, RunResult};
use common::{git_repo_with, MANIFEST};

const URV: &str = "unused-return-values";

fn ctx(repo: &tempfile::TempDir) -> CheckContext {
    CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    }
}

fn run_one(repo: &tempfile::TempDir, check_id: &str) -> RunResult {
    run_checks(&ctx(repo), Tier::Fast, Some(check_id))
}

fn entry(check: &str, file: &str, symbol: &str) -> String {
    format!("[[allow]]\ncheck = \"{check}\"\nfile = \"{file}\"\nsymbol = \"{symbol}\"\nreason = \"test\"\n\n")
}

/// A crate with exactly one finding for `unused-return-values`: `f`, discarded in `main`.
fn crate_with_one_finding(allowlist: &str) -> tempfile::TempDir {
    git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
        (".coderipper.toml", allowlist),
    ])
}

fn stale(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|f| f.check_id == "allowlist")
        .collect()
}

#[test]
fn a_matching_entry_suppresses_the_finding_and_is_not_reported_stale() {
    let repo = crate_with_one_finding(&entry(URV, "src/main.rs", "f"));
    let result = run_one(&repo, URV);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn without_an_entry_the_finding_is_reported() {
    let repo = crate_with_one_finding("");
    let result = run_one(&repo, URV);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].subject.as_deref(), Some("f"));
}

#[test]
fn an_entry_whose_symbol_matches_nothing_is_reported_as_an_informational_finding() {
    let repo = crate_with_one_finding(&format!(
        "{}{}",
        entry(URV, "src/main.rs", "f"),
        entry(URV, "src/main.rs", "long_since_deleted")
    ));
    let result = run_one(&repo, URV);

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let stale = stale(&result.findings);
    assert_eq!(stale.len(), 1, "{:?}", result.findings);
    let s = stale[0];
    assert_eq!(s.severity, Severity::Info);
    assert_eq!(s.subject.as_deref(), Some("long_since_deleted"));
    assert!(s.summary.contains("`long_since_deleted`"), "{}", s.summary);
    assert!(
        s.detail.contains("test"),
        "the entry's reason must be shown: {}",
        s.detail
    );
    assert!(s.positive_control.is_some());
    assert!(s.clone().validate().is_ok());
}

#[test]
fn a_matching_symbol_in_a_different_file_does_not_suppress_and_the_entry_is_stale() {
    // Exact identity, not loose containment: same symbol, wrong file.
    let repo = crate_with_one_finding(&entry(URV, "src/other.rs", "f"));
    let result = run_one(&repo, URV);
    assert_eq!(
        result.findings.iter().filter(|f| f.check_id == URV).count(),
        1
    );
    assert_eq!(stale(&result.findings).len(), 1);
}

#[test]
fn an_entry_for_a_check_that_failed_to_run_is_not_judged() {
    // The build is broken, so the check errors. Its entries must not be called stale: nothing was
    // learned about whether they still match.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn main() { let x: i32 = \"no\"; }\n"),
        (".coderipper.toml", &entry(URV, "src/main.rs", "f")),
    ]);
    let result = run_one(&repo, URV);
    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    assert!(stale(&result.findings).is_empty(), "{:?}", result.findings);
}

#[test]
fn an_entry_for_a_registered_check_that_did_not_run_is_not_judged() {
    // `--check reachability` never ran URV, so a URV entry says nothing about this run.
    let repo = crate_with_one_finding(&entry(URV, "src/main.rs", "f"));
    let result = run_one(&repo, "reachability");
    assert!(stale(&result.findings).is_empty(), "{:?}", result.findings);
}

#[test]
fn an_entry_naming_an_unknown_check_is_reported_even_when_that_check_was_not_asked_for() {
    let repo = crate_with_one_finding(&entry("unused-return-valeus", "src/main.rs", "f"));
    let result = run_one(&repo, URV);
    let stale = stale(&result.findings);
    assert_eq!(stale.len(), 1, "{:?}", result.findings);
    assert!(
        stale[0].summary.contains("unused-return-valeus"),
        "{}",
        stale[0].summary
    );
    assert!(
        stale[0].detail.contains(URV),
        "should list registered checks: {}",
        stale[0].detail
    );
    assert!(stale[0].clone().validate().is_ok());
}

#[test]
fn a_malformed_allowlist_is_an_error_but_does_not_swallow_the_findings() {
    let repo = crate_with_one_finding("[[allow]]\ncheck = \"x\"\n");
    let result = run_one(&repo, URV);
    assert!(
        result.errors.iter().any(|e| e.contains("allowlist")),
        "{:?}",
        result.errors
    );
    assert_eq!(
        result.findings.iter().filter(|f| f.check_id == URV).count(),
        1
    );
}
