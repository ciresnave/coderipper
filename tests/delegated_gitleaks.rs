#![cfg(feature = "cli")]
//! SEC-002 against the REAL gitleaks: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over a seeded repository and its clean twin. No fake stands in for the tool here: this is the
//! test that says the mapping reads what gitleaks really writes.
//!
//! These tests need the network once (the download, ~8 MB). To run the suite offline set `CODERIPPER_SKIP_NETWORK_TESTS=1`: they
//! then print `SKIPPED` and pass, which proves nothing about gitleaks, and CI must not set it.

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

/// A tools directory holding the real gitleaks, installed once for the whole test binary.
fn tools_dir() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("gitleaks")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned gitleaks (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
            });
        assert!(path.is_file(), "{path:?}");
        dir
    })
    .path()
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

/// A committed copy of a SEC-002 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let from = fixtures().join("SEC-002").join(kind);
    for name in ["README.md", "settings.cfg"] {
        std::fs::copy(from.join(name), dir.path().join(name)).unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "fixture"]);
    dir
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

const SECRET: &str = "k9Xv2Qm7Lr4Tz8Wp3Yb6Nc1Hd5Sf0Ga";

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let module = DelegatedModule::new(env);
    let proofs = run(&module, &Options::new(fixtures())).expect("the runner works");
    let verdicts: Vec<_> = proofs
        .rules
        .iter()
        .map(|r| (&r.rule[..], &r.verdict))
        .collect();
    assert_eq!(verdicts, vec![("SEC-002", &Verdict::Proven)]);
}

#[test]
fn without_the_tool_the_same_fixtures_are_unproven_not_failed() {
    let empty = tempfile::tempdir().unwrap();
    let env = ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven);
    let proofs = run(&DelegatedModule::new(env), &Options::new(fixtures())).unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("gitleaks")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(
        std::fs::read_dir(empty.path()).unwrap().count(),
        0,
        "nothing was installed without consent"
    );
}

#[test]
fn a_committed_secret_is_a_sec_002_finding_and_the_value_is_never_printed() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("defective");
    let assert = coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--message-format",
            "json",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0);
    let out = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    let err = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    let finding = out
        .lines()
        .find(|l| l.contains("coderipper-finding"))
        .unwrap_or_else(|| panic!("no finding line in: {out}\nstderr: {err}"));
    let v: serde_json::Value = serde_json::from_str(finding).unwrap();
    assert_eq!(v["check_id"], "SEC-002");
    assert_eq!(v["severity"], "high");
    assert_eq!(v["location"]["file"], "settings.cfg");
    assert_eq!(v["location"]["line"], 2);
    assert!(
        !out.contains(SECRET) && !err.contains(SECRET),
        "the secret was printed"
    );
}

#[test]
fn deny_turns_the_finding_into_exit_1() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("defective");
    coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--deny",
            "high",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(1);
}

#[test]
fn the_clean_twin_reports_nothing_and_says_the_rule_ran() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("clean");
    coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--message-format",
            "json",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("coderipper-finding").not());
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("SEC-002 not run"))
        .stderr(predicate::str::contains(
            "coderipper tools install gitleaks --install-tools",
        ))
        .stderr(predicate::str::contains("check error").not());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn install_tools_installs_the_pinned_tool_and_then_scans() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    // a fresh directory: the run itself installs, because --install-tools was passed
    let fresh = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(fresh.path())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--install-tools",
            "--deny",
            "high",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(1);
    assert!(fresh.path().join("gitleaks").join("8.30.1").is_dir());
}

#[test]
fn the_tool_is_part_of_extended_only() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args(["check", "SEC-002", "--project"])
        .arg(repo.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--profile extended"));
}

/// A repository whose first commit holds the seeded key and whose second removes it: the secret is in history only.
fn repo_with_secret_only_in_history() -> tempfile::TempDir {
    let dir = repo_of("defective");
    std::fs::write(dir.path().join("settings.cfg"), "# deployment settings\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "remove the key"]);
    dir
}

fn shallow_clone_of(repo: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("file://{}", repo.display().to_string().replace('\\', "/"));
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--depth", "1", &url])
        .arg(dir.path())
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    dir
}

#[test]
fn a_secret_deleted_from_the_tree_is_still_found_in_history() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_with_secret_only_in_history();
    coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--deny",
            "high",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(1);
}

#[test]
fn a_shallow_clone_that_looks_clean_is_a_gap_not_a_clean_rule() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    // the control above shows the full repository has the secret; the shallow clone of the same repository cannot see it
    let full = repo_with_secret_only_in_history();
    let shallow = shallow_clone_of(full.path());
    coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(shallow.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("SEC-002 not run"))
        .stderr(predicate::str::contains("shallow"));
}

fn head(dir: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "SEC-002",
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

fn finding_count(stdout: &str) -> usize {
    stdout
        .lines()
        .filter(|l| l.contains("coderipper-finding"))
        .count()
}

#[test]
fn the_repository_cannot_silence_its_own_findings() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    // A: a .gitleaksignore naming the finding's fingerprint and a .gitleaks.toml allowlisting the file, both committed
    let a = repo_of("defective");
    let fingerprint = format!("{}:settings.cfg:generic-api-key:2\n", head(a.path()));
    std::fs::write(a.path().join(".gitleaksignore"), fingerprint).unwrap();
    std::fs::write(
        a.path().join(".gitleaks.toml"),
        "[extend]\nuseDefault = true\n[allowlist]\npaths = ['''settings\\.cfg''']\n",
    )
    .unwrap();
    git(a.path(), &["add", "-A"]);
    git(a.path(), &["commit", "-q", "-m", "silence it"]);
    let (code, out, err) = run_json(a.path());
    assert_eq!((code, finding_count(&out)), (0, 1), "{out}\n{err}");

    // B: the key's own line says gitleaks:allow
    let b = tempfile::tempdir().unwrap();
    std::fs::write(
        b.path().join("settings.cfg"),
        "vendor_api_key = \"k9Xv2Qm7Lr4Tz8Wp3Yb6Nc1Hd5Sf0Ga\" # gitleaks:allow\n",
    )
    .unwrap();
    git(b.path(), &["init", "-q"]);
    git(b.path(), &["config", "user.email", "t@example.com"]);
    git(b.path(), &["config", "user.name", "t"]);
    git(b.path(), &["add", "-A"]);
    git(b.path(), &["commit", "-q", "-m", "allowed"]);
    let (code, out, err) = run_json(b.path());
    assert_eq!((code, finding_count(&out)), (0, 1), "{out}\n{err}");
}

#[test]
fn a_directory_that_is_not_a_repository_or_has_no_commit_is_an_error_not_a_clean_rule() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let not_a_repo = tempfile::tempdir().unwrap();
    let (code, out, err) = run_json(not_a_repo.path());
    assert_eq!(code, 3, "{out}\n{err}");
    assert!(err.contains("not a git repository with a commit"), "{err}");

    let no_commit = tempfile::tempdir().unwrap();
    git(no_commit.path(), &["init", "-q"]);
    let (code, out, err) = run_json(no_commit.path());
    assert_eq!(code, 3, "{out}\n{err}");
}

#[test]
fn a_relative_project_path_is_scanned_not_silently_skipped() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    // under target/ so the path is relative to the package root, which is the working directory of `cargo test`
    let holder = tempfile::tempdir_in("target").unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/SEC-002/defective");
    for name in ["README.md", "settings.cfg"] {
        std::fs::copy(from.join(name), holder.path().join(name)).unwrap();
    }
    git(holder.path(), &["init", "-q"]);
    git(holder.path(), &["config", "user.email", "t@example.com"]);
    git(holder.path(), &["config", "user.name", "t"]);
    git(holder.path(), &["add", "-A"]);
    git(holder.path(), &["commit", "-q", "-m", "fixture"]);
    let relative = PathBuf::from("target").join(holder.path().file_name().unwrap());
    assert!(relative.is_relative());
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let ctx = coderipper::check::CheckContext::new(relative);
    let result = coderipper::run_checks_in_with_tools(
        coderipper::Profile::Extended,
        &env,
        &ctx,
        coderipper::check::Tier::Fast,
        Some("SEC-002"),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
}

#[test]
fn a_project_in_a_subdirectory_gets_its_own_leaks_named_relative_to_it() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("sub")).unwrap();
    std::fs::create_dir_all(repo.path().join("other")).unwrap();
    let from = fixtures().join("SEC-002/defective/settings.cfg");
    std::fs::copy(&from, repo.path().join("sub/settings.cfg")).unwrap();
    std::fs::write(repo.path().join("other/readme.md"), "# other\n").unwrap();
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["config", "user.email", "t@example.com"]);
    git(repo.path(), &["config", "user.name", "t"]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "two projects"]);

    let (code, out, err) = run_json(&repo.path().join("sub"));
    assert_eq!(code, 0, "{out}\n{err}");
    let line = out
        .lines()
        .find(|l| l.contains("coderipper-finding"))
        .expect("a finding");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["location"]["file"], "settings.cfg", "{line}");
    assert_eq!(v["project"], "sub");

    // the sibling project owns none of it
    let (code, out, err) = run_json(&repo.path().join("other"));
    assert_eq!((code, finding_count(&out)), (0, 0), "{out}\n{err}");
}
