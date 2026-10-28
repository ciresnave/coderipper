#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-007 against the REAL squawk: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over seeded workflows and their clean twins. No fake stands in for the tool here: this is the
//! test that says the mapping reads what squawk really writes.
//!
//! The pull-request tests need the network once, for the download (~20 MB), and nothing else: squawk runs offline and every
//! workflow here is written by the test, so the result does not change with the day. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about squawk, and CI must not set it.

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

/// A tools directory holding the real squawk, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("squawk")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned squawk (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
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

/// A committed repository holding these files.
fn repo_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    commit(dir.path());
    dir
}

/// A committed copy of an API-007 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("API-007").join(kind), dir.path());
    std::fs::remove_file(dir.path().join("expect.toml")).unwrap();
    commit(dir.path());
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

/// `coderipper check API-007 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "API-007",
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

/// Whether the lock pins a squawk build for the platform running the tests (it has no Windows arm build).
fn pinned_here() -> bool {
    let empty = tempfile::tempdir().unwrap();
    match ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven)
        .resolve("squawk")
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
            eprintln!("SKIPPED: the lock pins no squawk build for this platform");
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
        &Options::new(fixtures()).rule("API-007"),
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
        &Options::new(fixtures()).rule("API-007"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("squawk")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_dropped_table_is_a_high_finding_at_its_file_and_line_and_the_clean_twin_reports_nothing() {
    needs_tool!();
    let bad = repo_of("defective");
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["check_id"], "API-007");
    assert_eq!(all[0]["location"]["file"], "migrations/001_drop.sql");
    assert_eq!(all[0]["location"]["line"], 2);
    assert_eq!(all[0]["subject"], "ban-drop-table");
    assert_eq!(all[0]["severity"], "high");
    let good = repo_of("clean");
    let (code, out, err) = run_json(good.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn the_tools_style_advice_is_not_this_rules_finding() {
    needs_tool!();
    // squawk's own defaults flag all of these (no IF NOT EXISTS, a 32-bit integer, no lock timeout): none is a safety problem
    let repo = repo_with(&[(
        "migrations/001_style.sql",
        b"CREATE TABLE t (id int);\nALTER TABLE t ADD COLUMN n integer;\n",
    )]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
}

#[test]
fn the_repositorys_squawk_config_is_not_read_but_an_ignore_comment_is_honoured_and_said() {
    needs_tool!();
    // a committed config that excludes the rule would silence squawk run from the project; run from a scratch directory it does not
    let repo = repo_with(&[
        ("migrations/001_a.sql", b"DROP TABLE a;\n"),
        (".squawk.toml", b"excluded_rules = [\"ban-drop-table\"]\n"),
    ]);
    // the control: squawk itself, given that config, drops the finding
    let control = std::process::Command::new(&tools().1)
        .args(["--reporter", "json", "--config"])
        .arg(repo.path().join(".squawk.toml"))
        .arg(repo.path().join("migrations/001_a.sql"))
        .output()
        .unwrap();
    let control = String::from_utf8_lossy(&control.stdout).to_string();
    assert!(
        !control.contains("ban-drop-table"),
        "the control did not silence anything: {control}"
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(
        findings(&out).len(),
        1,
        "the config did not silence the rule: {out}\n{err}"
    );
    // an inline ignore comment is the tool's own and is honoured; the finding of another statement says so
    let ignored = repo_with(&[(
        "migrations/001_b.sql",
        b"-- squawk-ignore ban-drop-table\nDROP TABLE a;\nDROP TABLE b;\n",
    )]);
    let (code, out, err) = run_json(ignored.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["line"], 3);
    assert!(
        all[0]["detail"].as_str().unwrap().contains("squawk-ignore"),
        "{}",
        all[0]["detail"]
    );
}

#[test]
fn a_file_squawk_cannot_parse_makes_a_clean_result_a_gap_and_findings_say_so() {
    needs_tool!();
    let only_bad = repo_with(&[("migrations/001_x.sql", b"SELEKT 1;\n")]);
    let (code, out, err) = run_json(only_bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("API-007") && err.contains("could not parse"),
        "{err}"
    );
    let beside = repo_with(&[
        ("migrations/001_x.sql", b"SELEKT 1;\n"),
        ("migrations/002_y.sql", b"DROP TABLE c;\n"),
    ]);
    let (code, out, err) = run_json(beside.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert!(all[0]["detail"]
        .as_str()
        .unwrap()
        .contains("could not parse"));
}

#[test]
fn a_project_with_no_migration_is_a_gap_not_a_clean_rule() {
    needs_tool!();
    let repo = repo_with(&[
        ("queries/report.sql", b"DROP TABLE not_a_migration;\n"),
        ("README.md", b"x\n"),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("API-007") && err.contains("no tracked database migration"),
        "{err}"
    );
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_with(&[("migrations/001_a.sql", b"DROP TABLE a;\n")]);
    coderipper(empty.path())
        .args([
            "check",
            "API-007",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("squawk"));
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
