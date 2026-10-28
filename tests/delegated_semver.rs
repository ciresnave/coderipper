#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-001 against the REAL cargo-semver-checks: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over seeded workflows and their clean twins. No fake stands in for the tool here: this is the
//! test that says the mapping reads what cargo-semver-checks really writes.
//!
//! The pull-request tests need the network once, for the download (~8 MB), and nothing else: cargo-semver-checks runs offline and every
//! workflow here is written by the test, so the result does not change with the day. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about cargo-semver-checks, and CI must not set it.

use assert_cmd::Command;
use coderipper::conformance::{run, Options, Verdict};
use coderipper::module::DelegatedModule;
use coderipper::tools::{Consent, ToolEnv};
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn offline() -> bool {
    std::env::var("CODERIPPER_SKIP_NETWORK_TESTS").is_ok_and(|v| v == "1")
}

/// A tools directory holding the real cargo-semver-checks, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("cargo-semver-checks")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned cargo-semver-checks (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
            });
        assert!(path.is_file(), "{path:?}");
        (dir, path)
    })
}

fn tools_dir() -> &'static Path {
    tools().0.path()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// Commits everything in `dir` (force-adding, so a `.gitignore` in the fixture does not keep a file out of the repository).
fn commit(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["add", "-A", "-f"]);
    git(dir, &["commit", "-q", "-m", "fixture"]);
}

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A repository whose first commit holds `base` and whose second holds `now`. The baseline is the release tag `tag` on the first
/// commit, or, with no tag, the default branch `origin/main` set to the first commit.
fn repo_with_baseline(
    base: &[(&str, &str)],
    now: &[(&str, &str)],
    tag: Option<&str>,
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "t"]);
    let write = |files: &[(&str, &str)]| {
        for (name, text) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    };
    write(base);
    git(dir.path(), &["add", "-A", "-f"]);
    git(dir.path(), &["commit", "-q", "-m", "baseline"]);
    match tag {
        Some(tag) => git(dir.path(), &["tag", tag]),
        None => git(
            dir.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        ),
    }
    write(now);
    git(dir.path(), &["add", "-A", "-f"]);
    git(dir.path(), &["commit", "-q", "-m", "change"]);
    dir
}

/// A committed copy of an API-001 fixture, its `.baseline` directory as the first commit.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let fixture = fixtures().join("API-001").join(kind);
    let read = |dir: &Path| -> Vec<(String, String)> {
        let mut out = Vec::new();
        for name in ["Cargo.toml", "src/lib.rs"] {
            out.push((
                name.to_string(),
                std::fs::read_to_string(dir.join(name)).unwrap(),
            ));
        }
        out
    };
    let base = read(&fixture.join(".baseline"));
    let now = read(&fixture);
    let borrow = |v: &Vec<(String, String)>| -> Vec<(String, String)> { v.clone() };
    let (base, now) = (borrow(&base), borrow(&now));
    let b: Vec<(&str, &str)> = base.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let n: Vec<(&str, &str)> = now.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    repo_with_baseline(&b, &n, None)
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

/// `coderipper check API-001 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "API-001",
            "--deny",
            "none",
            "--profile",
            "extended",
            "--message-format",
            "json",
            "--project",
        ])
        .arg(project)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn findings(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|l| l.contains("coderipper-finding"))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Whether the lock pins a cargo-semver-checks build for the platform running the tests.
fn pinned_here() -> bool {
    let empty = tempfile::tempdir().unwrap();
    match ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven)
        .resolve("cargo-semver-checks")
    {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("tool_unavailable_for_platform"),
    }
}

macro_rules! needs_tool {
    () => {
        if offline() {
            eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
            return;
        }
        if !pinned_here() {
            eprintln!("SKIPPED: the lock pins no cargo-semver-checks build for this platform");
            return;
        }
    };
}

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    needs_tool!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("API-001"),
    )
    .expect("the runner works");
    assert_eq!(proofs.rules.len(), 1);
    assert_eq!(proofs.rules[0].verdict, Verdict::Proven);
}

#[test]
fn without_the_tool_the_rule_is_unproven_and_nothing_is_installed() {
    let empty = tempfile::tempdir().unwrap();
    let env = ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("API-001"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("cargo-semver-checks")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_removed_public_function_is_a_finding_at_its_place_in_the_baseline_and_the_clean_twin_reports_nothing(
) {
    needs_tool!();
    let bad = repo_of("defective");
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["check_id"], "API-001");
    assert_eq!(all[0]["location"]["file"], "src/lib.rs");
    assert_eq!(all[0]["location"]["line"], 2);
    assert_eq!(all[0]["subject"], "function fixture::gone");
    assert_eq!(all[0]["severity"], "high");
    let good = repo_of("clean");
    let (code, out, err) = run_json(good.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn the_release_tag_is_the_baseline_and_a_version_bump_that_declares_the_break_passes() {
    needs_tool!();
    let lib_old = "pub fn keep() {}\npub fn gone() {}\n";
    let lib_new = "pub fn keep() {}\n";
    // tagged release, then the function is removed without a bump: a finding
    let unbumped = repo_with_baseline(
        &[("Cargo.toml", MANIFEST), ("src/lib.rs", lib_old)],
        &[("src/lib.rs", lib_new)],
        Some("v0.1.0"),
    );
    let (_, out, err) = run_json(unbumped.path());
    assert_eq!(findings(&out).len(), 1, "{out}\n{err}");
    // the same removal released as a breaking change (before 1.0 the second number): not a finding
    let bumped = repo_with_baseline(
        &[("Cargo.toml", MANIFEST), ("src/lib.rs", lib_old)],
        &[
            ("Cargo.toml", &MANIFEST.replace("0.1.0", "0.2.0")),
            ("src/lib.rs", lib_new),
        ],
        Some("v0.1.0"),
    );
    let (code, out, err) = run_json(bumped.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}\n{err}");
}

#[test]
fn nothing_is_written_into_the_analysed_project() {
    needs_tool!();
    let bad = repo_of("defective");
    run_json(bad.path());
    // a second run over the same project works too (the tool otherwise leaves a clone below the project's target/ and fails)
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(findings(&out).len(), 1, "{out}\n{err}");
    assert!(
        !bad.path().join("target").exists(),
        "the tool wrote a target directory into the project"
    );
}

#[test]
fn a_crate_with_only_binaries_or_no_baseline_is_a_gap_not_a_clean_rule() {
    needs_tool!();
    // no library: nothing to compare
    let bin = repo_with_baseline(
        &[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")],
        &[("src/main.rs", "fn main() { }\n")],
        Some("v0.1.0"),
    );
    let (code, out, err) = run_json(bin.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("API-001") && err.contains("no library target"),
        "{err}"
    );
    // a library with no release tag and no default branch: no baseline
    let lone = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(lone.path().join("src")).unwrap();
    std::fs::write(lone.path().join("Cargo.toml"), MANIFEST).unwrap();
    std::fs::write(lone.path().join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    commit(lone.path());
    let (code, out, err) = run_json(lone.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("API-001") && err.contains("release tag"),
        "{err}"
    );
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_with_baseline(
        &[("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn a() {}\n")],
        &[("src/lib.rs", "pub fn b() {}\n")],
        Some("v0.1.0"),
    );
    coderipper(empty.path())
        .args([
            "check",
            "API-001",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("cargo-semver-checks"));
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
