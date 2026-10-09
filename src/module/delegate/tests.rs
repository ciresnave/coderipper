//! The delegation framework: getting a tool, running it, and what each way of failing means. `git` stands in for a tool that
//! misbehaves (the suite needs it anyway): `git --version` exits 0 and writes no report, `git <nonsense>` exits non-zero.

use super::*;
use crate::check::{CheckContext, Tier};
use crate::module::{reconcile, RuleStatus};
use crate::tools::{Consent, Fetcher, ToolsLock};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

const PLATFORM: &str = "x86_64-linux";
const SRC: &str = "https://example.com/gitleaks-8";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn lock(platform: &str, bytes: &[u8]) -> ToolsLock {
    ToolsLock::parse(&format!(
        "[[tool]]\nname = \"gitleaks\"\nversion = \"8.0.0\"\nplatform = \"{platform}\"\nsource = \"{SRC}\"\nsha256 = \"{}\"\nlicence = \"MIT\"\n",
        sha(bytes)
    ))
    .unwrap()
}

/// Serves canned bytes (or fails), counting how often it was asked.
#[derive(Clone, Default)]
struct Fake {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    calls: Arc<Mutex<usize>>,
}

impl Fake {
    fn serving(bytes: &[u8]) -> Self {
        let fake = Self::default();
        fake.files
            .lock()
            .unwrap()
            .insert(SRC.to_string(), bytes.to_vec());
        fake
    }

    fn asked(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

impl Fetcher for Fake {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, String> {
        *self.calls.lock().unwrap() += 1;
        self.files
            .lock()
            .unwrap()
            .get(source)
            .cloned()
            .ok_or_else(|| format!("no such source {source}"))
    }
}

fn env(lock: ToolsLock, root: &std::path::Path, consent: Consent, fake: &Fake) -> ToolEnv {
    ToolEnv::from_environment()
        .lock(lock)
        .cache_root(root)
        .platform(PLATFORM)
        .consent(consent)
        .fetcher(fake.clone())
}

fn request(rules: &[&str]) -> Request {
    Request::new(
        &CheckContext::new("."),
        None,
        Tier::Fast,
        rules.iter().map(|r| r.to_string()).collect(),
        Limits::new(60, 1 << 20),
    )
}

// ---- getting the tool: absence is a gap, a bad tool is an error ----

#[test]
fn a_tool_the_lock_does_not_have_is_a_gap() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::default();
    let e = env(
        ToolsLock::parse("").unwrap(),
        root.path(),
        Consent::Granted,
        &fake,
    );
    let Resolved::Unavailable(why) = resolve(&e, "gitleaks") else {
        panic!("not in the lock must be a gap")
    };
    assert!(why.starts_with("tool_missing"), "{why}");
    assert_eq!(
        fake.asked(),
        0,
        "nothing is downloaded for a tool that is not pinned"
    );
}

#[test]
fn a_tool_with_no_build_for_this_platform_is_a_gap() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::default();
    let e = env(
        lock("aarch64-macos", b"x"),
        root.path(),
        Consent::Granted,
        &fake,
    );
    let Resolved::Unavailable(why) = resolve(&e, "gitleaks") else {
        panic!("no build for the platform must be a gap")
    };
    assert!(why.starts_with("tool_unavailable_for_platform"), "{why}");
}

#[test]
fn a_tool_not_installed_without_consent_is_a_gap_that_names_the_command_and_downloads_nothing() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::serving(b"bytes");
    let e = env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::NotGiven,
        &fake,
    );
    let Resolved::Unavailable(why) = resolve(&e, "gitleaks") else {
        panic!("no consent must be a gap")
    };
    assert!(
        why.contains("coderipper tools install gitleaks --install-tools"),
        "{why}"
    );
    assert_eq!(fake.asked(), 0);
    assert!(
        std::fs::read_dir(root.path()).unwrap().next().is_none(),
        "nothing is written without consent"
    );
}

#[test]
fn a_download_that_does_not_match_the_lock_is_an_error_not_a_gap() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::serving(b"tampered");
    let e = env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::Granted,
        &fake,
    );
    let Resolved::Failed(kind, why) = resolve(&e, "gitleaks") else {
        panic!("a checksum mismatch must be an error")
    };
    assert_eq!(kind, ErrorKind::ChecksumMismatch, "{why}");
}

#[test]
fn an_install_the_user_asked_for_that_fails_is_an_error_not_a_gap() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::default(); // serves nothing: the download fails
    let e = env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::Granted,
        &fake,
    );
    let Resolved::Failed(kind, why) = resolve(&e, "gitleaks") else {
        panic!("a failed install after consent must be an error")
    };
    assert_eq!(kind, ErrorKind::ToolMissing, "{why}");
}

#[test]
fn a_consented_install_yields_the_absolute_path() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::serving(b"bytes");
    let e = env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::Granted,
        &fake,
    );
    let Resolved::Ready(path) = resolve(&e, "gitleaks") else {
        panic!("a good download must install")
    };
    assert!(
        path.is_absolute() && path.starts_with(root.path()),
        "{path:?}"
    );
    assert!(e.is_installed("gitleaks"));
}

// ---- running it ----

fn no_report_args(_repo: &std::path::Path, _report: &std::path::Path) -> Vec<OsString> {
    vec!["--version".into()]
}

fn failing_args(_repo: &std::path::Path, _report: &std::path::Path) -> Vec<OsString> {
    vec!["definitely-not-a-git-command".into()]
}

fn git() -> std::path::PathBuf {
    std::path::PathBuf::from("git")
}

#[test]
fn a_tool_that_exits_zero_and_writes_no_report_is_unreadable_not_clean() {
    let Verdict::Failed(kind, why) =
        gitleaks::scan_with(&git(), no_report_args, &request(&["SEC-002"]))
    else {
        panic!("no report must not read as a clean scan")
    };
    assert_eq!(kind, ErrorKind::ToolOutputUnreadable, "{why}");
}

#[test]
fn a_tool_that_exits_non_zero_is_tool_failed_with_its_stderr() {
    let Verdict::Failed(kind, why) =
        gitleaks::scan_with(&git(), failing_args, &request(&["SEC-002"]))
    else {
        panic!("a non-zero exit must be a failure")
    };
    assert_eq!(kind, ErrorKind::ToolFailed, "{why}");
    assert!(
        why.contains("definitely-not-a-git-command"),
        "stderr tail missing: {why}"
    );
}

#[test]
fn a_program_that_cannot_start_is_tool_failed() {
    let missing = std::path::PathBuf::from("this-program-does-not-exist-anywhere");
    let Verdict::Failed(kind, _) =
        gitleaks::scan_with(&missing, no_report_args, &request(&["SEC-002"]))
    else {
        panic!("an unstartable program must be a failure")
    };
    assert_eq!(kind, ErrorKind::ToolFailed);
}

// ---- the module, as the host judges it ----

#[test]
fn an_unavailable_tool_is_a_skipped_gap_in_the_host_s_eyes_and_fails_nothing() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::default();
    let module = DelegatedModule::new(env(
        ToolsLock::parse("").unwrap(),
        root.path(),
        Consent::NotGiven,
        &fake,
    ));
    let req = request(&["SEC-002"]);
    let reconciled = reconcile(&req, module.check(&req));
    assert_eq!(reconciled.run_errors, Vec::<String>::new());
    assert!(!reconciled.incomplete);
    assert!(
        matches!(&reconciled.rules[0].status, RuleStatus::Unavailable { detail } if detail.contains("gitleaks")),
        "{:?}",
        reconciled.rules[0].status
    );
    assert!(reconciled.rules[0].findings.is_empty());
}

#[test]
fn a_rule_the_module_does_not_have_is_an_error() {
    let module = DelegatedModule::default();
    let req = request(&["SEC-999"]);
    let reconciled = reconcile(&req, module.check(&req));
    assert!(matches!(
        reconciled.rules[0].status,
        RuleStatus::Error {
            kind: ErrorKind::Internal,
            ..
        }
    ));
}

#[test]
fn the_claim_carries_its_proof_only_while_the_tool_is_installed() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake::serving(b"bytes");
    let absent = DelegatedModule::new(env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::NotGiven,
        &fake,
    ));
    let hello = absent.describe().unwrap();
    assert!(hello.problem().is_none());
    assert_eq!(hello.rules.len(), 1);
    let claim = &hello.rules[0];
    assert_eq!(
        (
            claim.id.as_str(),
            claim.status.as_str(),
            claim.tool.as_deref()
        ),
        ("SEC-002", "delegated", Some("gitleaks"))
    );
    assert_eq!(
        claim.proof, None,
        "a tool that is not here earns no coverage"
    );

    let present = DelegatedModule::new(env(
        lock(PLATFORM, b"bytes"),
        root.path(),
        Consent::Granted,
        &fake,
    ));
    resolve(&present.env, "gitleaks");
    assert_eq!(
        present.describe().unwrap().rules[0].proof.as_deref(),
        Some("conformance/SEC-002")
    );
}

#[test]
fn the_rule_ids_are_what_the_module_claims() {
    let hello = DelegatedModule::default().describe().unwrap();
    let claimed: Vec<&str> = hello.rules.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(claimed, delegated_rule_ids());
}
