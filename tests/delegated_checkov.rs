#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-009 against the REAL checkov: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over seeded workflows and their clean twins. No fake stands in for the tool here: this is the
//! test that says the mapping reads what checkov really writes.
//!
//! The pull-request tests need the network once, for the download (~50 MB), and nothing else: checkov runs offline and every
//! workflow here is written by the test, so the result does not change with the day. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about checkov, and CI must not set it.

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

/// A tools directory holding the real checkov, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("checkov")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned checkov (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
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

/// A committed copy of a SEC-009 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("SEC-009").join(kind), dir.path());
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

/// `coderipper check SEC-009 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "SEC-009",
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

/// Whether the lock pins a checkov build for the platform running the tests. The release has x86_64 builds for Windows, Linux and
/// macOS only, so on an Apple-silicon Mac the rule is a reported gap (`tool_unavailable_for_platform`) and the real-binary tests
/// have nothing to run; the tests that check the gap itself still run there.
fn pinned_here() -> bool {
    let empty = tempfile::tempdir().unwrap();
    match ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven)
        .resolve("checkov")
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
            eprintln!("SKIPPED: the lock pins no checkov build for this platform");
            return;
        }
    };
}

const PRIVILEGED_POD: &str = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: p\nspec:\n  containers:\n    - name: c\n      image: nginx\n      securityContext:\n        privileged: true\n";
const WILDCARD_POLICY: &str = "resource \"aws_iam_policy\" \"p\" {\n  name   = \"p\"\n  policy = jsonencode({\n    Version   = \"2012-10-17\"\n    Statement = [{ Action = \"*\", Effect = \"Allow\", Resource = \"*\" }]\n  })\n}\n";

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    needs_tool!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("SEC-009"),
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
        &Options::new(fixtures()).rule("SEC-009"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("checkov")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_root_container_is_a_finding_at_its_file_and_line_and_the_clean_twin_reports_nothing() {
    needs_tool!();
    let bad = repo_of("defective");
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["check_id"], "SEC-009");
    assert_eq!(all[0]["location"]["file"], "Dockerfile");
    assert_eq!(all[0]["location"]["line"], 2);
    assert_eq!(all[0]["severity"], "medium");
    assert_eq!(all[0]["confidence"], "high");
    let good = repo_of("clean");
    let (code, out, err) = run_json(good.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn a_wildcard_policy_and_a_privileged_pod_are_high_findings() {
    needs_tool!();
    let repo = repo_with(&[
        ("iam.tf", WILDCARD_POLICY.as_bytes()),
        ("deploy/pod.yaml", PRIVILEGED_POD.as_bytes()),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    let files: Vec<&str> = all
        .iter()
        .map(|f| f["location"]["file"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"iam.tf") && files.contains(&"deploy/pod.yaml"),
        "{out}"
    );
    let privileged = all
        .iter()
        .find(|f| f["detail"].as_str().unwrap().contains("CKV_K8S_16"))
        .expect(&out);
    assert_eq!(privileged["severity"], "high", "{out}");
    assert!(
        all.iter()
            .any(|f| f["detail"].as_str().unwrap().contains("CKV_AWS_62")),
        "{out}"
    );
}

#[test]
fn another_rules_check_is_not_reported_here() {
    needs_tool!();
    // an S3 bucket with no access logging: checkov knows many checks about it, none of them this rule's
    let repo = repo_with(&[(
        "bucket.tf",
        b"resource \"aws_s3_bucket\" \"b\" {\n  bucket = \"x\"\n}\n",
    )]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
}

#[test]
fn the_repository_cannot_silence_its_own_findings() {
    needs_tool!();
    let skipped = PRIVILEGED_POD.replace(
        "metadata:\n  name: p\n",
        "metadata:\n  name: p\n  annotations:\n    checkov.io/skip1: CKV_K8S_16=needed for the driver\n",
    );
    let repo = repo_with(&[("pod.yaml", skipped.as_bytes())]);
    // the control: checkov itself, run the way a user would, does skip it
    // (in an empty directory and with the same scrubbed environment the rule uses, so a CI runner's own variables, such as the
    // ones that mark a GitHub Actions job, cannot change what checkov does)
    let scratch = tempfile::tempdir().unwrap();
    let mut control = std::process::Command::new(&tools().1);
    control.env_clear().current_dir(scratch.path());
    for key in [
        "PATH",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(key) {
            control.env(key, value);
        }
    }
    let plain = control
        .args([
            "--output",
            "json",
            "--compact",
            "--skip-download",
            "--framework",
            "kubernetes",
            "--check",
            "CKV_K8S_16",
            "-f",
        ])
        .arg(repo.path().join("pod.yaml"))
        .env("BC_SKIP_MAPPING", "TRUE")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&plain.stderr).to_string();
    let plain = String::from_utf8_lossy(&plain.stdout).to_string();
    assert!(
        plain.contains("SKIPPED") && plain.contains("needed for the driver"),
        "the control did not suppress anything: stdout {plain:?}, stderr {stderr:?}"
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    // the pod also fails other checks of this rule (escalation, root); the suppressed one is the privileged-container check
    let suppressed: Vec<_> = all
        .iter()
        .filter(|f| f["detail"].as_str().unwrap().contains("checkov:skip"))
        .collect();
    assert_eq!(suppressed.len(), 1, "{out}\n{err}");
    let detail = suppressed[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("CKV_K8S_16") && detail.contains("does not honour"),
        "{detail}"
    );
}

#[test]
fn a_project_with_no_infrastructure_file_is_a_gap_not_a_clean_rule() {
    needs_tool!();
    let repo = repo_with(&[("README.md", b"nothing here\n")]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("SEC-009") && err.contains("no tracked infrastructure"),
        "{err}"
    );
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_with(&[("Dockerfile", b"FROM ubuntu:22.04\nUSER root\n")]);
    coderipper(empty.path())
        .args([
            "check",
            "SEC-009",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("checkov"));
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
