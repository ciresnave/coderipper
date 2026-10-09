#![cfg(feature = "cli")]
//! SUP-002 and SEC-006 against the REAL osv-scanner: the pinned release is downloaded (through the embedded lock,
//! checksum-verified) into a temporary tools directory and run over seeded lockfiles and their clean twins. No fake stands in for
//! the tool here: this is the test that says the mapping reads what osv-scanner really writes.
//!
//! These tests need the network twice: the download (~30 MB) and osv-scanner's own query of osv.dev. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about osv-scanner, and CI must not
//! set it. The fixtures depend on today's database: lodash 4.17.15 has had advisories for years, but a new advisory against the
//! clean twin's packages would fail its test, by design (replace the package).

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

/// A tools directory holding the real osv-scanner, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("osv-scanner")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned osv-scanner (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
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

/// Commits everything in `dir`.
fn commit(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "fixture"]);
}

/// A committed copy of a SUP-002 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixtures()
            .join("SUP-002")
            .join(kind)
            .join("package-lock.json"),
        dir.path().join("package-lock.json"),
    )
    .unwrap();
    commit(dir.path());
    dir
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

/// `coderipper check <rule> --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(rule: &str, project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            rule,
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

#[test]
fn the_claims_are_earned_by_the_fixtures_against_the_real_tool() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let module = DelegatedModule::new(env);
    for rule in ["SUP-002", "SEC-006"] {
        let proofs = run(&module, &Options::new(fixtures()).network(true).rule(rule))
            .expect("the runner works");
        assert_eq!(proofs.rules.len(), 1, "{rule}");
        assert_eq!(proofs.rules[0].verdict, Verdict::Proven, "{rule}");
    }
}

#[test]
fn without_the_network_flag_the_same_fixtures_are_unproven_not_failed() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("SUP-002"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(_)),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
}

#[test]
fn without_the_tool_the_rules_are_unproven_and_nothing_is_installed() {
    let empty = tempfile::tempdir().unwrap();
    let env = ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).network(true).rule("SUP-002"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("osv-scanner")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_pinned_vulnerable_dependency_is_a_sup_002_finding_at_the_lockfile() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("defective");
    let (code, out, err) = run_json("SUP-002", repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert!(!all.is_empty(), "no finding in: {out}\nstderr: {err}");
    for f in &all {
        assert_eq!(f["check_id"], "SUP-002");
        assert_eq!(f["location"]["file"], "package-lock.json");
        assert_eq!(f["subject"], "npm:lodash@4.17.15");
    }
    // the two serious advisories are among them (these have been published since 2020/2021)
    let text = all
        .iter()
        .map(|f| f["detail"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("GHSA-35jh-r3h4-6jhm") && text.contains("GHSA-p6mc-m468-83gw"),
        "{text}"
    );
}

#[test]
fn deny_turns_the_finding_into_exit_1_and_the_clean_twin_reports_nothing() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let bad = repo_of("defective");
    coderipper(tools_dir())
        .args([
            "check",
            "SUP-002",
            "--profile",
            "extended",
            "--deny",
            "high",
            "--project",
        ])
        .arg(bad.path())
        .assert()
        .code(1);
    let good = repo_of("clean");
    coderipper(tools_dir())
        .args([
            "check",
            "SUP-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--message-format",
            "json",
            "--project",
        ])
        .arg(good.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("coderipper-finding").not());
}

#[test]
fn sec_006_reports_only_the_serious_advisories_and_says_it_has_not_checked_reachability() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("defective");
    let (code, out, err) = run_json("SEC-006", repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let sec = findings(&out);
    let (_, sup_out, _) = run_json("SUP-002", repo.path());
    let sup = findings(&sup_out);
    assert!(
        !sec.is_empty() && sec.len() < sup.len(),
        "{} of {}",
        sec.len(),
        sup.len()
    );
    for f in &sec {
        assert_eq!(f["check_id"], "SEC-006");
        assert_eq!(f["confidence"], "low");
        assert!(
            f["detail"].as_str().unwrap().contains("not analysed"),
            "{f}"
        );
        assert!(
            matches!(f["severity"].as_str(), Some("high" | "critical")),
            "{f}"
        );
    }
}

#[test]
fn the_repository_cannot_silence_its_own_findings() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("defective");
    std::fs::write(
        repo.path().join("osv-scanner.toml"),
        "[[IgnoredVulns]]\nid = \"GHSA-35jh-r3h4-6jhm\"\nreason = \"x\"\n\n[[IgnoredVulns]]\nid = \"GHSA-p6mc-m468-83gw\"\nreason = \"x\"\n",
    )
    .unwrap();
    std::fs::write(repo.path().join(".gitignore"), "package-lock.json\n").unwrap();
    git(repo.path(), &["add", "-A", "-f"]);
    git(repo.path(), &["commit", "-q", "-m", "silence it"]);

    // the control: osv-scanner itself, run the way a user would, honours both (the ignored advisories are gone from its report)
    let scratch = tempfile::tempdir().unwrap();
    let report = scratch.path().join("report.json");
    let status = std::process::Command::new(&tools().1)
        .args([
            "scan",
            "source",
            "--format",
            "json",
            "--verbosity",
            "error",
            "--output-file",
        ])
        .arg(&report)
        .args(["--recursive"])
        .arg(repo.path())
        .status()
        .unwrap();
    // 128: the .gitignore hid the only lockfile, or 1: the other advisories are still there
    let plain = std::fs::read_to_string(&report).unwrap_or_default();
    assert!(
        matches!(status.code(), Some(1 | 128)),
        "unexpected exit {status:?}"
    );
    assert!(
        !plain.contains("GHSA-35jh-r3h4-6jhm") && !plain.contains("GHSA-p6mc-m468-83gw"),
        "the control did not silence anything: {plain}"
    );

    let (code, out, err) = run_json("SEC-006", repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let text = findings(&out)
        .iter()
        .map(|f| f["detail"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("GHSA-35jh-r3h4-6jhm") && text.contains("GHSA-p6mc-m468-83gw"),
        "{text}\n{err}"
    );
}

#[test]
fn a_project_with_no_lockfile_is_a_gap_not_a_clean_rule() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "# nothing to scan\n").unwrap();
    commit(repo.path());
    coderipper(tools_dir())
        .args([
            "check",
            "SUP-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("SUP-002 not run"))
        .stderr(predicate::str::contains("no package sources"));
}

#[test]
fn a_lockfile_osv_scanner_cannot_read_beside_clean_ones_is_an_error_not_a_clean_rule() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = repo_of("clean");
    std::fs::create_dir_all(repo.path().join("web")).unwrap();
    std::fs::write(repo.path().join("web/package-lock.json"), "{ broken").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "a broken lockfile"]);
    let (code, out, err) = run_json("SUP-002", repo.path());
    assert_eq!(code, 3, "{out}\n{err}");
    assert!(err.contains("could not read"), "{err}");
    assert!(findings(&out).is_empty());
}

#[test]
fn a_lockfile_in_a_subdirectory_is_named_relative_to_the_project() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("web/app")).unwrap();
    std::fs::copy(
        fixtures().join("SUP-002/defective/package-lock.json"),
        repo.path().join("web/app/package-lock.json"),
    )
    .unwrap();
    commit(repo.path());
    let (code, out, err) = run_json("SUP-002", repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert!(!all.is_empty(), "{out}\n{err}");
    assert!(
        all.iter()
            .all(|f| f["location"]["file"] == "web/app/package-lock.json"),
        "{out}"
    );
}

#[test]
fn a_relative_project_path_is_scanned_not_silently_skipped() {
    if offline() {
        eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
        return;
    }
    // under target/ so the path is relative to the package root, which is the working directory of `cargo test`
    std::fs::create_dir_all("target").unwrap();
    let holder = tempfile::tempdir_in("target").unwrap();
    std::fs::copy(
        fixtures().join("SUP-002/defective/package-lock.json"),
        holder.path().join("package-lock.json"),
    )
    .unwrap();
    commit(holder.path());
    let relative = PathBuf::from("target").join(holder.path().file_name().unwrap());
    assert!(relative.is_relative());
    let (code, out, err) = run_json("SUP-002", &relative);
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert!(!all.is_empty(), "{out}\n{err}");
    assert!(
        all.iter()
            .all(|f| f["location"]["file"] == "package-lock.json"),
        "{out}"
    );
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args([
            "check",
            "SUP-002",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("SUP-002 not run"))
        .stderr(predicate::str::contains(
            "coderipper tools install osv-scanner --install-tools",
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
            "SUP-002",
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
    assert!(fresh.path().join("osv-scanner").join("2.6.0").is_dir());
}

#[test]
fn a_fast_run_leaves_the_network_rules_alone_and_a_sweep_reports_them_as_a_gap() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    // fast: the advisory rules are sweep-tier, so they are not even attempted (no note about them)
    coderipper(empty.path())
        .args(["fast", "--profile", "extended", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("SUP-002").not())
        .stderr(predicate::str::contains("SEC-006").not());
    // sweep: attempted, and with no tool installed each is a reported gap
    coderipper(empty.path())
        .args(["sweep", "--profile", "extended", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("SUP-002 not run"))
        .stderr(predicate::str::contains("SEC-006 not run"));
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

/// The scheduled run's canary (`.github/workflows/live-tools.yml`): a real, popular package version that has no advisory today is
/// clean. It depends on the day's database, so it is not part of a pull request's checks: a failure here is a signal that a new
/// advisory was published (replace the package) or that osv.dev or osv-scanner changed, never a reason to block a merge.
#[test]
#[ignore = "reads the live advisory database; run by the scheduled workflow with --include-ignored"]
fn a_real_package_with_no_advisory_today_is_clean() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("package-lock.json"),
        r#"{"name":"d","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"d","version":"1.0.0","dependencies":{"is-odd":"3.0.1"}},"node_modules/is-number":{"version":"6.0.0"},"node_modules/is-odd":{"version":"3.0.1"}}}"#,
    )
    .unwrap();
    commit(repo.path());
    let (code, out, err) = run_json("SUP-002", repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "{err}");
}
