# CI and branch-protection presence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CodeRipper's fifth check, `ci-protection-presence`: report a GitHub repository whose default branch requires no
status checks — CireSnave's standing rule, *"every repo, new ones included, gets CI checks and branch protection that requires
them"*. Read `enforcement_level` and `contexts`, **never `.protected`** (it reads `true` on branches that enforce nothing).

**Architecture:** `gh api` GETs behind a small `Github` trait, so every rule is tested with canned responses (copied from real
ones) and no network. The check finds the repository from `origin`, asks GitHub for the default branch, reads its
`protection.required_status_checks`, and — only if that requires nothing — also reads the repository *rulesets* that apply to the
branch before calling it unprotected. Network-required, so it belongs to the `sweep` tier and never runs in `fast`.

**Tech Stack:** Rust 2021, no new dependencies (`serde_json` is already used; the client is the `gh` binary the portfolio already uses).

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §5 (`ci-protection-presence`: `Project` scope, `NetworkRequired`,
`sweep` tier; `High` severity if protection is absent or enforces zero contexts).
**Scope, per the PM (2026-10-02): milestone 1, part 2 of 2**, after `version-consistency` (PR #15). **Out of scope, as ordered:**
`dependency-staleness` and the `sweep` *runner* (where and how often sweep runs). The `sweep` *tier* already exists in the host.

**Stacked on `version-consistency`** (registered-check list and its pinning test are edited by both). `git fetch origin`: if #15 has
merged, branch from `origin/main`; otherwise from `origin/feat/version-consistency`, and rebase onto `main` after it merges
(`git rebase --onto origin/main <old base tip>`; the portfolio's repos squash-merge).

## What running it taught (probed against the real API, read-only, as `ciresnave-bot`)

1. **The real shapes** (2026-10-02), copied into the tests: a protected default branch → `protected: true`, `protection.enabled: true`,
   `required_status_checks: { enforcement_level: "non_admins", contexts: [3 names], checks: [3 objects] }`; an unprotected one →
   `protected: false`, `protection: { enabled: false, required_status_checks: { enforcement_level: "off", contexts: [], checks: [] } }`;
   an EMPTY repository → `repos/<o>/<r>` works (`default_branch: "main"`) but `branches/main` is `404 "Branch not found"`.
2. **`gh api` reports failures on stdout as JSON** (`{"message":"Not Found","status":"404"}`) and on stderr as `gh: Not Found (HTTP 404)`;
   the client takes the status from whichever is there. A token without push access can still read `protection` on a PUBLIC repo
   (the bot has `push: false` on this repo and read it fine); on a private repo GitHub omits the object — handled below.
3. **Rulesets can require checks with no classic protection**, so a repository protected only by a ruleset would be reported as
   unprotected: a false `High`. The check therefore reads `rules/branches/<branch>` before concluding. Verified on the real API: every
   portfolio repository answers `[]` (none uses rulesets today), and a PRIVATE repository on a free plan answers
   `403 "Upgrade to GitHub Pro or make this repository public to enable this feature."` — rulesets cannot exist there, so that answer
   means "none", not "cannot see". This was added after the first build, test-first (3 of the 5 new tests failed first).
4. **What the portfolio looks like** (60 non-archived, non-fork repositories under `ciresnave`, default branch only, read just now):
   **10** protected with required checks, **44** with no protection at all, **6** empty (no commits). The check reports; whether all of
   those should be protected is CireSnave's call.
5. **Acceptance (real API, this branch):** `coderipper` and `fuel` → "no issues found"; `smskit` → ``[High/High] ... the default branch `main` has no branch protection``;
   `ee-amt` and `HumboldtUnifiedKidTracker` (private, free plan) → the same `High`; `bayes-optimal` (empty) → an `Info` note; a repository that
   does not exist → an error (`Not Found (HTTP 404)`); a GitLab origin → an error.

## Design decisions (rulings, with what each costs if wrong)

1. **A finding when `enforcement_level` is `off` or no context is required;** `non_admins` (admins may bypass) and `everyone` both pass. The
   portfolio's own repos use `non_admins`. *Cost if wrong:* a repo where admins routinely bypass looks fine; `enforce_admins` is a separate
   setting this check does not read.
2. **`contexts` is the count of required checks, falling back to `checks`** only when `contexts` is absent. Never `protected`.
3. **Cannot-see is an error, never clean:** no `origin`; an origin that is not GitHub; any API failure (status and message in the error); a
   branch reply with no `protection` object. *Cost:* a sweep over many repos shows one error per repo it could not read, which is the point.
4. **Two honest non-findings, each an `Info` finding saying so:** an archived repository, and a repository with no commits yet.
5. **Rulesets are consulted only when classic protection requires nothing** (one extra call, only where it matters). A free-plan 403 means "none";
   any other failure reading rules is an error.
6. **`GitHub` only, read-only, as whichever `gh` account is active** — the check never switches accounts (portfolio rule) and never writes.
7. **`Finding.location.file` is the pseudo-path `github:branch-protection`** and `subject` is `owner/repo@branch`, so the existing
   `[[allow]]` mechanism (and its stale detection) works unchanged: `check = "ci-protection-presence"`, `file = "github:branch-protection"`,
   `symbol = "owner/repo@main"`.
8. **`github` is a public module** (`coderipper::github::{Github, ApiError}`) so integration tests can supply a fake; its production client `GhCli` shells out to `gh api`.
9. **No version bump in this PR** — the PM allocates it at gate time.

## Global Constraints

- Rust edition 2021; CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --all-targets`,
  `cargo test --no-fail-fast` on **ubuntu, windows and macos**. **CI must never reach the network:** every test uses the fake; the live probes
  in Task 4 are manual acceptance steps, not tests.
- A check that cannot read what it must judge returns `Err`, never `Ok(vec![])`: an empty result must always mean "protected".
- Absence claims carry a `positive_control`: here, the successful read of the branch (name, commit, `protection` values) that shows the settings were readable, not merely missing.
- **Never `gh auth switch`**; never write to GitHub; GET only.
- Do **not** bump any version in `Cargo.toml`; ask the PM at gate time.
- Clone with `-c core.autocrlf=false` on this Windows box; never `checkout` in a shared tree. Backslash literals via a shell heredoc can be dropped or doubled here — use the editor/Write tool.
- Commit trailers (this lane): end each commit message with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. The lane account cannot open or merge PRs here; ask the PM.

## Review Focus

1. **`.protected` must never decide.** → `protected_true_with_no_required_checks_is_still_reported`, `protection_without_any_status_check_settings_is_reported` (Task 2).
2. **Enforcement level and contexts both matter**: contexts listed but enforcement `off` is a finding; `everyone` is clean. → `a_required_context_with_enforcement_off_is_reported_and_everyone_is_clean` (Task 2).
3. **Every way of not seeing is an error**: missing `protection` object; API failures with their status; non-GitHub or missing origin (and GitHub is not called for them). → `a_reply_without_the_protection_object_is_an_error_never_clean`, `api_failures_are_errors_with_the_status`, `a_repository_that_is_not_on_github_or_has_no_origin_is_an_error` (Task 2); `a_rules_lookup_that_fails_for_another_reason_is_an_error_never_a_verdict` (Task 3).
4. **No false `High` for a ruleset-protected repo**, and rulesets must not rescue a branch they do not require checks on. → `a_ruleset_that_requires_checks_counts_even_without_classic_protection`, `rules_that_require_no_status_checks_do_not_rescue_an_unprotected_branch`, `a_plan_that_cannot_have_rulesets_is_not_an_error`, `rules_are_not_even_asked_for_when_classic_protection_already_requires_checks` (Task 3).
5. **The default branch is whatever GitHub says**, not `main`. → `the_default_branch_is_whatever_github_says_not_main` (Task 2).
6. **Honest non-findings and tier.** → `an_archived_repository_is_informational_not_a_defect`, `a_repository_with_no_commits_yet_is_informational`, `it_belongs_to_the_sweep_tier_and_is_not_run_by_fast` (Task 2).
7. **Remote-URL and `gh`-failure parsing** (https/ssh/scp-like/credentials/`.git`/trailing slash; other hosts; look-alike hosts; JSON body vs stderr). → the `github` unit tests (Task 1).

**Known gaps accepted:** only the default branch; `enforce_admins`, required reviews and signed commits are not read; the *names* of required checks are not matched against the repository's workflows (a required check that never runs would block merges, not pass them); GitHub only (other forges are an error); no rate-limit handling beyond reporting the error.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/github.rs` (new) | `Github` trait, `ApiError`, `GhCli` (`gh api`), `parse_gh_failure`, `RepoRef`, `parse_github_remote`, `origin_url` |
| `src/checks/ci_protection_presence/mod.rs` (new) | The check |
| `src/lib.rs`, `src/checks/mod.rs` | Declare, register |
| `tests/ci_protection.rs` (new), `tests/cli.rs` | Canned-response tests; CLI |

Baseline before starting (tip of #15, `origin/feat/version-consistency`): `cargo test` shows 139 lib tests, 8 in `tests/allowlist.rs`, 11 in `tests/cli.rs`,
20 in `tests/reachability.rs`, 11 in `tests/unused_parameters.rs`, 13 in `tests/unused_return_values.rs`, 18 in `tests/version_consistency.rs`, 13 in `tests/workspace.rs`.

---

### Task 1: The GitHub client and remote parsing

**Files:** Create `src/github.rs`; modify `src/lib.rs` (`pub mod github;` after `pub mod finding;`).

**Interfaces:**
- Produces: `github::Github` (`fn get(&self, path: &str) -> Result<serde_json::Value, ApiError>`), `github::ApiError { status: Option<u16>, message: String }`
  (`Display`: `message (HTTP status)`), `github::GhCli`, `github::RepoRef { owner, name }`, `github::parse_github_remote(&str) -> Option<RepoRef>`,
  `github::origin_url(&Path) -> anyhow::Result<String>`; crate-private `parse_gh_failure(stdout: &[u8], stderr: &[u8]) -> ApiError`.

- [ ] **Step 1: Declare the module and write the tests first.** Add `pub mod github;` to `src/lib.rs`. Create `src/github.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from the full file in Step 2.
  Run: `cargo test --lib github`
  Expected: FAIL to compile — `cannot find type RepoRef`, `cannot find function origin_url / parse_gh_failure`.

- [ ] **Step 2: Replace the file with the full implementation:**

````rust
// src/github.rs
//! The little of GitHub this tool needs: read one JSON document from the REST API, and work out which
//! repository a git checkout belongs to.
//!
//! The client is `gh api <path>` — the portfolio's own authenticated GitHub client — behind the
//! [`Github`] trait so a check's logic is tested with canned responses (copied from real ones) and no
//! network. This module never switches `gh` accounts and never writes to GitHub: it only GETs.

use std::path::Path;
use std::process::Command;

/// A failed API call. `status` is the HTTP status when GitHub answered, `None` when it never did
/// (no `gh`, no network, not authenticated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: Option<u16>,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "{} (HTTP {status})", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ApiError {}

/// Reads one JSON document from the GitHub REST API. `path` is relative to the API root, e.g.
/// `repos/ciresnave/coderipper/branches/main`.
pub trait Github {
    fn get(&self, path: &str) -> Result<serde_json::Value, ApiError>;
}

/// The real client: `gh api <path>`, as whichever account `gh` has active.
pub struct GhCli;

impl Github for GhCli {
    fn get(&self, path: &str) -> Result<serde_json::Value, ApiError> {
        let output = Command::new("gh")
            .args(["api", path])
            .output()
            .map_err(|e| ApiError {
                status: None,
                message: format!(
                    "cannot run `gh` ({e}); install the GitHub CLI and `gh auth login`"
                ),
            })?;
        if output.status.success() {
            serde_json::from_slice(&output.stdout).map_err(|e| ApiError {
                status: None,
                message: format!("GitHub's answer for {path} was not JSON: {e}"),
            })
        } else {
            Err(parse_gh_failure(&output.stdout, &output.stderr))
        }
    }
}

/// `gh api` prints the API's JSON error body on stdout (`{"message":"Not Found","status":"404"}`) and
/// `gh: Not Found (HTTP 404)` on stderr. Take the status and message from whichever is there.
pub(crate) fn parse_gh_failure(stdout: &[u8], stderr: &[u8]) -> ApiError {
    let body: Option<serde_json::Value> = serde_json::from_slice(stdout).ok();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    let status = body
        .as_ref()
        .and_then(|b| b.get("status"))
        .and_then(|s| {
            s.as_u64()
                .or_else(|| s.as_str().and_then(|text| text.parse().ok()))
        })
        .or_else(|| {
            stderr
                .split("(HTTP ")
                .nth(1)
                .and_then(|rest| rest.split(')').next())
                .and_then(|n| n.trim().parse().ok())
        })
        .map(|n| n as u16);
    let message = body
        .as_ref()
        .and_then(|b| b.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .filter(|m| !m.is_empty())
        .unwrap_or(stderr);
    ApiError { status, message }
}

/// Which GitHub repository a remote URL names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

/// `https://github.com/o/n(.git)`, `git@github.com:o/n(.git)`, `ssh://git@github.com/o/n(.git)`,
/// `git://github.com/o/n`, with or without credentials, a trailing slash, or `.git`. Any other host
/// (GitLab, a local path) is `None`.
pub fn parse_github_remote(url: &str) -> Option<RepoRef> {
    let url = url.trim();
    let path = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        host.eq_ignore_ascii_case("github.com").then_some(path)?
    } else {
        let rest = url.split_once("://")?.1;
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        let host = host.split(':').next()?;
        host.eq_ignore_ascii_case("github.com").then_some(path)?
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let valid = |s: &str| !s.is_empty() && !s.contains('/');
    (valid(owner) && valid(name)).then(|| RepoRef {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// The URL of `origin` in the git repository containing `dir`.
pub fn origin_url(dir: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{} has no `origin` remote ({}); ci-protection-presence needs to know which GitHub repository it is",
        dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, name: &str) -> Option<RepoRef> {
        Some(RepoRef {
            owner: owner.into(),
            name: name.into(),
        })
    }

    #[test]
    fn every_common_github_remote_form_is_understood() {
        for url in [
            "https://github.com/ciresnave/coderipper.git",
            "https://github.com/ciresnave/coderipper",
            "https://github.com/ciresnave/coderipper/",
            "http://github.com/ciresnave/coderipper.git",
            "https://user:token@github.com/ciresnave/coderipper.git",
            "git@github.com:ciresnave/coderipper.git",
            "git@github.com:ciresnave/coderipper",
            "ssh://git@github.com/ciresnave/coderipper.git",
            "ssh://git@github.com:22/ciresnave/coderipper.git",
            "git://github.com/ciresnave/coderipper.git",
            "https://GitHub.com/ciresnave/coderipper.git",
        ] {
            assert_eq!(
                parse_github_remote(url),
                repo("ciresnave", "coderipper"),
                "{url}"
            );
        }
    }

    #[test]
    fn other_hosts_and_malformed_urls_are_not_github() {
        for url in [
            "https://gitlab.com/ciresnave/coderipper.git",
            "git@gitlab.com:ciresnave/coderipper.git",
            "https://github.com.evil.example/ciresnave/coderipper.git",
            "https://github.com/ciresnave",
            "https://github.com/a/b/c",
            "/home/me/repos/coderipper",
            "C:\\repos\\coderipper",
            "",
        ] {
            assert_eq!(parse_github_remote(url), None, "{url}");
        }
    }

    #[test]
    fn a_gh_failure_yields_the_status_and_message_from_the_json_body() {
        let body = br#"{"message":"Branch not found","documentation_url":"https://docs.github.com/x","status":"404"}"#;
        let err = parse_gh_failure(body, b"gh: Branch not found (HTTP 404)");
        assert_eq!(err.status, Some(404));
        assert_eq!(err.message, "Branch not found");
    }

    #[test]
    fn a_gh_failure_without_a_body_falls_back_to_stderr() {
        let err = parse_gh_failure(b"", b"gh: Bad credentials (HTTP 401)\n");
        assert_eq!(err.status, Some(401));
        assert!(err.message.contains("Bad credentials"), "{}", err.message);
        let err = parse_gh_failure(b"", b"gh: To use GitHub CLI, run: gh auth login");
        assert_eq!(err.status, None);
        assert!(err.message.contains("gh auth login"));
    }

    #[test]
    fn a_numeric_status_in_the_body_is_accepted_too() {
        let err = parse_gh_failure(br#"{"message":"Forbidden","status":403}"#, b"");
        assert_eq!(err.status, Some(403));
    }

    #[test]
    fn the_origin_of_a_repository_is_read_and_a_missing_origin_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert!(origin_url(tmp.path()).is_err(), "no origin yet");
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widgets.git",
            ])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert_eq!(
            origin_url(tmp.path()).unwrap(),
            "https://github.com/acme/widgets.git"
        );
    }
}
````

- [ ] **Step 3: Run the tests.** Run: `cargo fmt && cargo test --lib github`
  Expected: PASS, 6 tests. (Expect `dead_code` warnings until Task 2.)

- [ ] **Step 4: Commit** — `git add -A src && git commit -m "feat: GitHub client behind a trait, gh failure parsing, remote URL parsing"` (+ trailers).

---

### Task 2: The check (classic branch protection)

**Files:** Create `src/checks/ci_protection_presence/mod.rs`, `tests/ci_protection.rs`; modify `src/checks/mod.rs`, `src/lib.rs`.

**Interfaces:**
- Consumes: Task 1.
- Produces: `checks::CiProtectionPresenceCheck` (`new()`; `with_api(Box<dyn Github>)`), id `"ci-protection-presence"`, `Scope::Project`, `Network::NetworkRequired`; `CHECK_ID`; `SETTINGS_FILE = "github:branch-protection"`. Findings: `High`/`High` with `subject = "owner/repo@branch"`; `Info` for archived / no commits; a `positive_control` always.

- [ ] **Step 1: Write the failing tests first.** Create `tests/ci_protection.rs` with this content (Task 3 appends to it). The `Fake` answers from a table and records every path asked:

````rust
// tests/ci_protection.rs
//! `ci-protection-presence` against canned GitHub responses. The JSON for "protected" and "unprotected" is
//! copied from real `gh api repos/<o>/<r>/branches/main` answers (2026-10-02): a protected branch with
//! three required contexts, and a branch with no protection at all. No network is involved.

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::CiProtectionPresenceCheck;
use coderipper::finding::{Finding, Severity};
use coderipper::github::{ApiError, Github};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Answers from a table and remembers what was asked.
struct Fake {
    answers: HashMap<String, Result<Value, ApiError>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Github for Fake {
    fn get(&self, path: &str) -> Result<Value, ApiError> {
        self.asked.lock().unwrap().push(path.to_string());
        self.answers.get(path).cloned().unwrap_or_else(|| {
            Err(ApiError {
                status: Some(404),
                message: format!("no canned answer for {path}"),
            })
        })
    }
}

fn repo_with_origin(url: Option<&str>) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    if let Some(url) = url {
        Command::new("git")
            .args(["remote", "add", "origin", url])
            .current_dir(tmp.path())
            .status()
            .unwrap();
    }
    tmp
}

fn run_with(
    dir: &Path,
    answers: Vec<(&str, Result<Value, ApiError>)>,
) -> (anyhow::Result<Vec<Finding>>, Vec<String>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let fake = Fake {
        answers: answers
            .into_iter()
            .map(|(p, a)| (p.to_string(), a))
            .collect(),
        asked: asked.clone(),
    };
    let check = CiProtectionPresenceCheck::with_api(Box::new(fake));
    let result = check.run(&CheckContext {
        project_root: dir.to_path_buf(),
        portfolio_root: dir.to_path_buf(),
    });
    let asked = asked.lock().unwrap().clone();
    (result, asked)
}

fn repo_info(default_branch: &str) -> Value {
    json!({ "default_branch": default_branch, "archived": false, "private": false })
}

/// Real shape: a protected branch requiring three contexts, enforced for non-admins.
fn protected_branch() -> Value {
    json!({
        "name": "main",
        "commit": { "sha": "627f001d0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
        "protected": true,
        "protection": {
            "enabled": true,
            "required_status_checks": {
                "checks": [
                    { "app_id": 15368, "context": "Build, test, clippy, fmt (ubuntu-latest)" },
                    { "app_id": 15368, "context": "Build, test, clippy, fmt (windows-latest)" },
                    { "app_id": 15368, "context": "Build, test, clippy, fmt (macos-latest)" }
                ],
                "contexts": [
                    "Build, test, clippy, fmt (ubuntu-latest)",
                    "Build, test, clippy, fmt (windows-latest)",
                    "Build, test, clippy, fmt (macos-latest)"
                ],
                "enforcement_level": "non_admins"
            }
        }
    })
}

/// Real shape: a branch with no protection at all.
fn unprotected_branch() -> Value {
    json!({
        "name": "main",
        "commit": { "sha": "b69e34e4eaf803ea4b34405518805cc32b6814ce" },
        "protected": false,
        "protection": {
            "enabled": false,
            "required_status_checks": { "enforcement_level": "off", "contexts": [], "checks": [] }
        }
    })
}

const ORIGIN: &str = "https://github.com/acme/widgets.git";
const REPO: &str = "repos/acme/widgets";
const BRANCH: &str = "repos/acme/widgets/branches/main";

fn subjects(findings: &[Finding]) -> Vec<String> {
    findings.iter().filter_map(|f| f.subject.clone()).collect()
}

#[test]
fn a_branch_requiring_checks_is_clean_the_negative_control() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(protected_branch())),
        ],
    );
    assert!(result.unwrap().is_empty());
}

#[test]
fn a_branch_with_no_protection_is_reported_the_positive_control() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
        ],
    );
    let found = result.unwrap();
    assert_eq!(subjects(&found), vec!["acme/widgets@main"]);
    let f = &found[0];
    assert_eq!(f.severity, Severity::High);
    assert_eq!(f.confidence, coderipper::finding::Confidence::High);
    assert!(f.summary.contains("no branch protection"), "{}", f.summary);
    assert!(
        f.positive_control.as_deref().unwrap().contains("b69e34e"),
        "{:?}",
        f.positive_control
    );
    assert!(f.clone().validate().is_ok());
}

#[test]
fn protected_true_with_no_required_checks_is_still_reported() {
    // The portfolio's own hard-learned lesson: `.protected` reads true on branches that enforce nothing.
    let mut branch = protected_branch();
    branch["protection"]["required_status_checks"] =
        json!({ "enforcement_level": "off", "contexts": [], "checks": [] });
    assert_eq!(branch["protected"], json!(true));
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(branch))],
    );
    let found = result.unwrap();
    assert_eq!(subjects(&found), vec!["acme/widgets@main"]);
    assert!(
        found[0]
            .summary
            .contains("protected but requires no status checks"),
        "{}",
        found[0].summary
    );
    assert!(
        found[0].detail.contains("protected: true"),
        "{}",
        found[0].detail
    );
}

#[test]
fn protection_without_any_status_check_settings_is_reported() {
    // e.g. only "require a pull request": `required_status_checks` is absent
    let branch = json!({
        "name": "main",
        "commit": { "sha": "1234567890" },
        "protected": true,
        "protection": { "enabled": true }
    });
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(branch))],
    );
    assert_eq!(subjects(&result.unwrap()), vec!["acme/widgets@main"]);
}

#[test]
fn a_required_context_with_enforcement_off_is_reported_and_everyone_is_clean() {
    let mut off = protected_branch();
    off["protection"]["required_status_checks"]["enforcement_level"] = json!("off");
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(off))],
    );
    assert_eq!(
        result.unwrap().len(),
        1,
        "contexts listed but enforcement off enforces nothing"
    );

    let mut everyone = protected_branch();
    everyone["protection"]["required_status_checks"]["enforcement_level"] = json!("everyone");
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(everyone))],
    );
    assert!(result.unwrap().is_empty());
}

#[test]
fn the_default_branch_is_whatever_github_says_not_main() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("trunk"))),
            (
                "repos/acme/widgets/branches/trunk",
                Ok(unprotected_branch()),
            ),
        ],
    );
    assert_eq!(subjects(&result.unwrap()), vec!["acme/widgets@trunk"]);
    assert_eq!(
        asked,
        vec![
            REPO.to_string(),
            "repos/acme/widgets/branches/trunk".to_string()
        ]
    );
}

#[test]
fn an_archived_repository_is_informational_not_a_defect() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![(
            REPO,
            Ok(json!({ "default_branch": "main", "archived": true })),
        )],
    );
    let found = result.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Info);
    assert!(
        found[0].summary.contains("archived"),
        "{}",
        found[0].summary
    );
    assert_eq!(
        asked,
        vec![REPO.to_string()],
        "no branch lookup for an archived repo"
    );
}

#[test]
fn a_repository_with_no_commits_yet_is_informational() {
    let repo = repo_with_origin(Some(ORIGIN));
    let not_found = ApiError {
        status: Some(404),
        message: "Branch not found".into(),
    };
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Err(not_found))],
    );
    let found = result.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Info);
    assert!(
        found[0].summary.contains("no commits"),
        "{}",
        found[0].summary
    );
    assert!(found[0].clone().validate().is_ok());
}

#[test]
fn a_reply_without_the_protection_object_is_an_error_never_clean() {
    // GitHub omits `protection` for a token without push access to a private repository.
    let branch = json!({ "name": "main", "commit": { "sha": "abc1234" }, "protected": true });
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(branch))],
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("protection") && err.contains("not reporting clean"),
        "{err}"
    );
}

#[test]
fn api_failures_are_errors_with_the_status() {
    let repo = repo_with_origin(Some(ORIGIN));
    let forbidden = ApiError {
        status: Some(403),
        message: "API rate limit exceeded".into(),
    };
    let (result, _) = run_with(repo.path(), vec![(REPO, Err(forbidden.clone()))]);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("403") && err.contains("rate limit"), "{err}");

    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Err(forbidden))],
    );
    assert!(result.unwrap_err().to_string().contains("403"));
}

#[test]
fn a_repository_that_is_not_on_github_or_has_no_origin_is_an_error() {
    let gitlab = repo_with_origin(Some("https://gitlab.com/acme/widgets.git"));
    let (result, asked) = run_with(gitlab.path(), vec![]);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not a GitHub repository"));
    assert!(
        asked.is_empty(),
        "must not call GitHub for a non-GitHub origin"
    );

    let none = repo_with_origin(None);
    let (result, _) = run_with(none.path(), vec![]);
    assert!(result.unwrap_err().to_string().contains("origin"));
}

#[test]
fn it_belongs_to_the_sweep_tier_and_is_not_run_by_fast() {
    let check = CiProtectionPresenceCheck::with_api(Box::new(Fake {
        answers: HashMap::new(),
        asked: Arc::new(Mutex::new(Vec::new())),
    }));
    assert_eq!(check.id(), "ci-protection-presence");
    assert_eq!(check.tier(), Tier::Sweep);
    assert!(coderipper::registered_checks()
        .iter()
        .any(|c| c.id() == "ci-protection-presence"));
}
````

  Run: `cargo test --test ci_protection`
  Expected: FAIL to compile — `unresolved import coderipper::checks::CiProtectionPresenceCheck`.

- [ ] **Step 2: Create the check.** `src/checks/ci_protection_presence/mod.rs`:

````rust
// src/checks/ci_protection_presence/mod.rs
//! CI/protection-presence check (project scope, NETWORK): does the repository's default branch enforce
//! required status checks?
//!
//! CireSnave's standing rule: every repo gets CI, and branch protection that REQUIRES it. This reads
//! GitHub's branch-protection settings for the repository `origin` points at and reports a default
//! branch that enforces nothing.
//!
//! **Never `.protected`.** GitHub's `protected: true` is true for a branch whose only rule is, say,
//! "no force pushes", and for branches that require zero status checks: it reads `true` on branches
//! that enforce nothing. What counts is `protection.required_status_checks`: its `enforcement_level`
//! (`off` / `non_admins` / `everyone`) and how many `contexts` it requires. A finding when the level is
//! `off` or no context is required; `non_admins` (admins may bypass) and `everyone` both pass.
//!
//! Network tier: it runs under `coderipper sweep` or `coderipper check ci-protection-presence`, never in
//! `fast`. Reads only (`gh api` GETs), as whichever account `gh` has active; it never switches accounts.
//!
//! **A check that cannot see must not report clean.** No `origin`, an origin that is not GitHub, an API
//! failure, or a reply that omits the `protection` object (GitHub hides it from tokens without push
//! access to a private repo) is an error. The two honest non-findings, each an `Info` finding saying so: an
//! archived repository, and one with no commits yet (its default branch does not exist).

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::github::{origin_url, parse_github_remote, GhCli, Github, RepoRef};

pub const CHECK_ID: &str = "ci-protection-presence";

/// `Finding.location.file` of every finding here: repository settings are not a file, so this names
/// the settings page an `[[allow]]` entry would point at (`file = "github:branch-protection"`,
/// `symbol = "owner/repo@branch"`).
pub const SETTINGS_FILE: &str = "github:branch-protection";

pub struct CiProtectionPresenceCheck {
    api: Box<dyn Github>,
}

impl CiProtectionPresenceCheck {
    /// The real check, reading GitHub through `gh`.
    pub fn new() -> Self {
        Self::with_api(Box::new(GhCli))
    }

    /// The check over any [`Github`] — what tests use, with canned responses.
    pub fn with_api(api: Box<dyn Github>) -> Self {
        Self { api }
    }
}

impl Default for CiProtectionPresenceCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl Check for CiProtectionPresenceCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn network(&self) -> Network {
        Network::NetworkRequired
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let url = origin_url(&ctx.project_root)?;
        let repo = parse_github_remote(&url).ok_or_else(|| {
            anyhow::anyhow!(
                "origin ({url}) is not a GitHub repository; ci-protection-presence reads GitHub's \
                 branch-protection settings"
            )
        })?;
        let slug = format!("{}/{}", repo.owner, repo.name);

        let info = self
            .api
            .get(&format!("repos/{slug}"))
            .map_err(|e| anyhow::anyhow!("cannot read {slug} from GitHub: {e}"))?;
        if info["archived"].as_bool() == Some(true) {
            return Ok(vec![info_finding(
                &repo,
                None,
                format!("{slug} is archived, so branch protection is not applicable"),
                format!("GET repos/{slug} reported `archived: true`"),
            )]);
        }
        let branch = info["default_branch"].as_str().ok_or_else(|| {
            anyhow::anyhow!("GitHub's answer for {slug} has no `default_branch`; cannot tell which branch to read")
        })?;

        let branch_path = format!("repos/{slug}/branches/{branch}");
        let doc = match self.api.get(&branch_path) {
            Ok(doc) => doc,
            Err(e) if e.status == Some(404) => {
                return Ok(vec![info_finding(
                    &repo,
                    Some(branch),
                    format!("{slug} has no commits yet, so its default branch `{branch}` does not exist to protect"),
                    format!("GET {branch_path} answered 404 while GET repos/{slug} reported default branch `{branch}`"),
                )]);
            }
            Err(e) => anyhow::bail!("cannot read {branch_path} from GitHub: {e}"),
        };

        // The `protection` object is what holds the settings; `protected` alone proves nothing.
        let protection = doc.get("protection").ok_or_else(|| {
            anyhow::anyhow!(
                "GitHub's answer for {branch_path} has no `protection` object, so the settings cannot be \
                 read (a private repository hides them from a token without push access); not reporting \
                 clean"
            )
        })?;
        let enabled = protection["enabled"].as_bool().unwrap_or(false);
        let checks = &protection["required_status_checks"];
        let level = checks["enforcement_level"].as_str().unwrap_or("off");
        let required = checks["contexts"]
            .as_array()
            .or_else(|| checks["checks"].as_array())
            .map_or(0, Vec::len);

        if level != "off" && required > 0 {
            return Ok(Vec::new());
        }

        let sha = doc["commit"]["sha"].as_str().unwrap_or("?");
        let sha = &sha[..sha.len().min(7)];
        let flag = doc["protected"].as_bool();
        let summary = if enabled {
            format!(
                "{slug}: the default branch `{branch}` is protected but requires no status checks \
                 (enforcement `{level}`, {required} required)"
            )
        } else {
            format!("{slug}: the default branch `{branch}` has no branch protection")
        };
        let detail = format!(
            "CireSnave's rule: every repo gets CI and branch protection that REQUIRES it. GitHub reports \
             `protection.enabled: {enabled}`, `required_status_checks.enforcement_level: \"{level}\"` and \
             {required} required context(s) for `{branch}`. (`protected: {}` is not evidence: it is true for a \
             branch whose rules require no checks.)",
            flag.map_or("absent".to_string(), |f| f.to_string())
        );
        Ok(vec![Finding {
            check_id: CHECK_ID.into(),
            severity: Severity::High,
            confidence: Confidence::High,
            project: repo.name.clone(),
            location: Some(Location {
                file: SETTINGS_FILE.into(),
                line: None,
            }),
            subject: Some(format!("{slug}@{branch}")),
            summary,
            detail,
            positive_control: Some(format!(
                "GET {branch_path} succeeded: branch `{}` at commit {sha}, with a `protection` object \
                 (enabled: {enabled}, enforcement_level: {level}, {required} required context(s)), so the \
                 settings were readable, not merely missing",
                doc["name"].as_str().unwrap_or(branch)
            )),
        }])
    }
}

fn info_finding(repo: &RepoRef, branch: Option<&str>, summary: String, control: String) -> Finding {
    let slug = format!("{}/{}", repo.owner, repo.name);
    Finding {
        check_id: CHECK_ID.into(),
        severity: Severity::Info,
        confidence: Confidence::High,
        project: repo.name.clone(),
        location: Some(Location {
            file: SETTINGS_FILE.into(),
            line: None,
        }),
        subject: Some(match branch {
            Some(b) => format!("{slug}@{b}"),
            None => slug,
        }),
        detail: "Nothing to protect: this is informational, not a defect.".into(),
        summary,
        positive_control: Some(control),
    }
}
````

- [ ] **Step 3: Register it.** `src/checks/mod.rs`: add `pub mod ci_protection_presence;` and `pub use ci_protection_presence::CiProtectionPresenceCheck;`.
  `src/lib.rs`: add `Box::new(checks::CiProtectionPresenceCheck::new()),` after the `VersionConsistencyCheck` entry of `registered_checks()`, and add `"ci-protection-presence"` to the id list the registration test pins.

- [ ] **Step 4: Run the tests.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 145 lib tests (139 + 6), 12 in `tests/ci_protection.rs`, every other count unchanged. `cargo clippy --all-targets -- -D warnings`: no output.

- [ ] **Step 5: Prove four guards can fail.** (a) Replace `if level != "off" && required > 0 {` with `if doc["protected"].as_bool() == Some(true) {`: three tests must FAIL (`protected_true_with_no_required_checks_is_still_reported`, `protection_without_any_status_check_settings_is_reported`, `a_required_context_with_enforcement_off_is_reported_and_everyone_is_clean`). (b) Replace it with `if required > 0 {`: `a_required_context_with_enforcement_off_is_reported_and_everyone_is_clean` must FAIL. (c) Make a missing `protection` object read as `Null` instead of an error: `a_reply_without_the_protection_object_is_an_error_never_clean` must FAIL. (d) Disable the `404 => Info` arm: `a_repository_with_no_commits_yet_is_informational` must FAIL. Revert each.

- [ ] **Step 6: Commit** — `git add -A src tests && git commit -m "feat: ci-protection-presence check -- default branch must require status checks"` (+ trailers).

---

### Task 3: Rulesets

**Files:** Modify `src/checks/ci_protection_presence/mod.rs`, `tests/ci_protection.rs`.

- [ ] **Step 1: Write the failing tests first** (and teach the `Fake` that no rulesets exist unless a test says otherwise — the real answer for the whole portfolio) — apply the `tests/ci_protection.rs` part of this diff:

````diff
diff --git a/tests/ci_protection.rs b/tests/ci_protection.rs
--- a/tests/ci_protection.rs
+++ b/tests/ci_protection.rs
@@ -22,6 +22,10 @@ impl Github for Fake {
     fn get(&self, path: &str) -> Result<Value, ApiError> {
         self.asked.lock().unwrap().push(path.to_string());
         self.answers.get(path).cloned().unwrap_or_else(|| {
+            // no rulesets unless a test says otherwise (the real answer for the whole portfolio)
+            if path.contains("/rules/branches/") {
+                return Ok(json!([]));
+            }
             Err(ApiError {
                 status: Some(404),
                 message: format!("no canned answer for {path}"),
@@ -242,7 +246,9 @@ fn the_default_branch_is_whatever_github_says_not_main() {
         asked,
         vec![
             REPO.to_string(),
-            "repos/acme/widgets/branches/trunk".to_string()
+            "repos/acme/widgets/branches/trunk".to_string(),
+            // classic protection requires nothing, so the rulesets are read before the verdict
+            "repos/acme/widgets/rules/branches/trunk".to_string(),
         ]
     );
 }
@@ -358,3 +364,110 @@ fn it_belongs_to_the_sweep_tier_and_is_not_run_by_fast() {
         .iter()
         .any(|c| c.id() == "ci-protection-presence"));
 }
+
+// ---- repository rulesets: GitHub's newer way to require checks, with no classic branch protection ----
+
+fn rules_requiring_checks() -> Value {
+    json!([
+        { "type": "deletion", "ruleset_source_type": "Repository", "ruleset_id": 1 },
+        {
+            "type": "required_status_checks",
+            "parameters": {
+                "strict_required_status_checks_policy": false,
+                "required_status_checks": [ { "context": "ci / build" } ]
+            },
+            "ruleset_source_type": "Repository",
+            "ruleset_id": 1
+        }
+    ])
+}
+
+const RULES: &str = "repos/acme/widgets/rules/branches/main";
+
+#[test]
+fn a_ruleset_that_requires_checks_counts_even_without_classic_protection() {
+    // Without this a repo protected only by a ruleset would be reported as unprotected: a false High.
+    let repo = repo_with_origin(Some(ORIGIN));
+    let (result, asked) = run_with(
+        repo.path(),
+        vec![
+            (REPO, Ok(repo_info("main"))),
+            (BRANCH, Ok(unprotected_branch())),
+            (RULES, Ok(rules_requiring_checks())),
+        ],
+    );
+    assert!(result.unwrap().is_empty());
+    assert!(asked.contains(&RULES.to_string()), "{asked:?}");
+}
+
+#[test]
+fn rules_that_require_no_status_checks_do_not_rescue_an_unprotected_branch() {
+    let rules =
+        json!([{ "type": "deletion", "ruleset_source_type": "Repository", "ruleset_id": 1 }]);
+    let repo = repo_with_origin(Some(ORIGIN));
+    let (result, _) = run_with(
+        repo.path(),
+        vec![
+            (REPO, Ok(repo_info("main"))),
+            (BRANCH, Ok(unprotected_branch())),
+            (RULES, Ok(rules)),
+        ],
+    );
+    let found = result.unwrap();
+    assert_eq!(subjects(&found), vec!["acme/widgets@main"]);
+    assert!(found[0].detail.contains("ruleset"), "{}", found[0].detail);
+}
+
+#[test]
+fn a_plan_that_cannot_have_rulesets_is_not_an_error() {
+    // Real answer for a private repo on a free plan: 403 "Upgrade to GitHub Pro ...". Rulesets cannot
+    // exist there, so the classic reading stands.
+    let upgrade = ApiError {
+        status: Some(403),
+        message: "Upgrade to GitHub Pro or make this repository public to enable this feature."
+            .into(),
+    };
+    let repo = repo_with_origin(Some(ORIGIN));
+    let (result, _) = run_with(
+        repo.path(),
+        vec![
+            (REPO, Ok(repo_info("main"))),
+            (BRANCH, Ok(unprotected_branch())),
+            (RULES, Err(upgrade)),
+        ],
+    );
+    assert_eq!(subjects(&result.unwrap()), vec!["acme/widgets@main"]);
+}
+
+#[test]
+fn a_rules_lookup_that_fails_for_another_reason_is_an_error_never_a_verdict() {
+    let boom = ApiError {
+        status: Some(500),
+        message: "Server Error".into(),
+    };
+    let repo = repo_with_origin(Some(ORIGIN));
+    let (result, _) = run_with(
+        repo.path(),
+        vec![
+            (REPO, Ok(repo_info("main"))),
+            (BRANCH, Ok(unprotected_branch())),
+            (RULES, Err(boom)),
+        ],
+    );
+    let err = result.unwrap_err().to_string();
+    assert!(err.contains("500") && err.contains("rules"), "{err}");
+}
+
+#[test]
+fn rules_are_not_even_asked_for_when_classic_protection_already_requires_checks() {
+    let repo = repo_with_origin(Some(ORIGIN));
+    let (result, asked) = run_with(
+        repo.path(),
+        vec![
+            (REPO, Ok(repo_info("main"))),
+            (BRANCH, Ok(protected_branch())),
+        ],
+    );
+    assert!(result.unwrap().is_empty());
+    assert!(!asked.iter().any(|p| p.contains("/rules/")), "{asked:?}");
+}
````

  Run: `cargo test --test ci_protection`
  Expected: FAIL — `a_ruleset_that_requires_checks_counts_even_without_classic_protection`, `rules_that_require_no_status_checks_do_not_rescue_an_unprotected_branch`, `a_rules_lookup_that_fails_for_another_reason_is_an_error_never_a_verdict` (3 of 17). The other two new tests pass already (they pin behaviour that must not change).

- [ ] **Step 2: Implement** — the `mod.rs` part of the same commit (it also updates `the_default_branch_is_whatever_github_says_not_main`, which now sees the rules lookup):

````diff
diff --git a/src/checks/ci_protection_presence/mod.rs b/src/checks/ci_protection_presence/mod.rs
--- a/src/checks/ci_protection_presence/mod.rs
+++ b/src/checks/ci_protection_presence/mod.rs
@@ -9,7 +9,9 @@
 //! "no force pushes", and for branches that require zero status checks: it reads `true` on branches
 //! that enforce nothing. What counts is `protection.required_status_checks`: its `enforcement_level`
 //! (`off` / `non_admins` / `everyone`) and how many `contexts` it requires. A finding when the level is
-//! `off` or no context is required; `non_admins` (admins may bypass) and `everyone` both pass.
+//! `off` or no context is required; `non_admins` (admins may bypass) and `everyone` both pass. A repository
+//! RULESET that requires checks (GitHub's newer mechanism, which needs no classic protection) also passes, so
+//! a branch is only called unprotected after `rules/branches/<branch>` has been read too.
 //!
 //! Network tier: it runs under `coderipper sweep` or `coderipper check ci-protection-presence`, never in
 //! `fast`. Reads only (`gh api` GETs), as whichever account `gh` has active; it never switches accounts.
@@ -125,6 +127,22 @@ impl Check for CiProtectionPresenceCheck {
             return Ok(Vec::new());
         }
 
+        // Classic branch protection requires nothing. A repository ruleset can require checks WITHOUT any
+        // classic protection, so look there before calling the branch unprotected.
+        let rules_path = format!("repos/{slug}/rules/branches/{branch}");
+        let rules = match self.api.get(&rules_path) {
+            Ok(rules) => rules,
+            // Real answer for a private repo on a free plan: rulesets cannot exist there.
+            Err(e) if e.status == Some(403) && e.message.contains("Upgrade to GitHub Pro") => {
+                serde_json::Value::Array(Vec::new())
+            }
+            Err(e) => anyhow::bail!("cannot read {rules_path} from GitHub: {e}"),
+        };
+        if ruleset_requires_checks(&rules) {
+            return Ok(Vec::new());
+        }
+        let rule_count = rules.as_array().map_or(0, Vec::len);
+
         let sha = doc["commit"]["sha"].as_str().unwrap_or("?");
         let sha = &sha[..sha.len().min(7)];
         let flag = doc["protected"].as_bool();
@@ -139,8 +157,9 @@ impl Check for CiProtectionPresenceCheck {
         let detail = format!(
             "CireSnave's rule: every repo gets CI and branch protection that REQUIRES it. GitHub reports \
              `protection.enabled: {enabled}`, `required_status_checks.enforcement_level: \"{level}\"` and \
-             {required} required context(s) for `{branch}`. (`protected: {}` is not evidence: it is true for a \
-             branch whose rules require no checks.)",
+             {required} required context(s) for `{branch}`. No active ruleset requires status checks either \
+             (GET {rules_path}: {rule_count} rule(s), none a `required_status_checks`). (`protected: {}` is not \
+             evidence: it is true for a branch whose rules require no checks.)",
             flag.map_or("absent".to_string(), |f| f.to_string())
         );
         Ok(vec![Finding {
@@ -165,6 +184,18 @@ impl Check for CiProtectionPresenceCheck {
     }
 }
 
+/// Does any active ruleset rule require at least one status check?
+fn ruleset_requires_checks(rules: &serde_json::Value) -> bool {
+    rules.as_array().is_some_and(|rules| {
+        rules.iter().any(|rule| {
+            rule["type"].as_str() == Some("required_status_checks")
+                && rule["parameters"]["required_status_checks"]
+                    .as_array()
+                    .is_some_and(|checks| !checks.is_empty())
+        })
+    })
+}
+
 fn info_finding(repo: &RepoRef, branch: Option<&str>, summary: String, control: String) -> Finding {
     let slug = format!("{}/{}", repo.owner, repo.name);
     Finding {
````

  Update `the_default_branch_is_whatever_github_says_not_main`'s expected requests to `[repos/acme/widgets, repos/acme/widgets/branches/trunk, repos/acme/widgets/rules/branches/trunk]` (already in the diff above for the test file).

- [ ] **Step 3: Run the suite.** Run: `cargo fmt && cargo test --no-fail-fast && cargo clippy --all-targets -- -D warnings`
  Expected: PASS — 145 lib, 17 in `tests/ci_protection.rs`; clippy silent.

- [ ] **Step 4: Prove two guards can fail.** (a) Change `if ruleset_requires_checks(&rules) {` to `if false && ruleset_requires_checks(&rules) {`: `a_ruleset_that_requires_checks_counts_even_without_classic_protection` must FAIL. (b) Change the free-plan arm's condition to `if false => {`: `a_plan_that_cannot_have_rulesets_is_not_an_error` must FAIL. Revert each.

- [ ] **Step 5: Commit** — `git add -A src tests && git commit -m "feat: a repository ruleset that requires checks also counts"` (+ trailers).

---

### Task 4: CLI test, docs, live acceptance

**Files:** Modify `tests/cli.rs`, `README.md`, `docs/superpowers/specs/2026-09-30-audit-host-design.md`.

- [ ] **Step 1: CLI test and docs.** The CLI test needs no network (a GitLab origin must fail loudly before any call):

````diff
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -14,6 +14,7 @@ coderipper check reachability        # one check by id
 coderipper check unused-return-values  # is a function's return value ever consumed?
 coderipper check unused-parameters   # which function parameters are never used?
 coderipper check version-consistency # does every package of the project share one version?
+coderipper check ci-protection-presence # does the default branch require status checks? (network; sweep tier)
 ```
 
 A server mode (`coderipper serve`) is planned, for a free hosted instance on
@@ -50,6 +51,18 @@ A package can also be silenced with an ordinary `[[allow]]` entry (check `versio
 manifest relative to the workspace root, `symbol` = the package name). Rust (cargo) manifests only; it reads the
 working tree, not HEAD.
 
+## CI and branch protection
+
+`coderipper check ci-protection-presence` reads GitHub's branch-protection settings for the repository `origin`
+points at (through `gh api`, as whichever account `gh` has active; read-only) and reports a default branch that
+requires no status checks: protection missing, or enabled but with `enforcement_level` `off` or zero required
+contexts. It never trusts `protected`, which is `true` on branches that enforce nothing. It is a network check, so it
+runs under `coderipper sweep` or by name, never in `fast`. It cannot silently pass: no `origin`, a non-GitHub origin, an
+API failure, or a reply without the `protection` object (hidden from a token without push access to a private
+repository) is an error; an archived repository and one with no commits yet each get an `Info` note. To accept a
+repository on purpose, use an `[[allow]]` entry with `check = "ci-protection-presence"`,
+`file = "github:branch-protection"` and `symbol = "owner/repo@branch"`.
+
 ## Workspaces
 
 A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
diff --git a/docs/superpowers/specs/2026-09-30-audit-host-design.md b/docs/superpowers/specs/2026-09-30-audit-host-design.md
--- a/docs/superpowers/specs/2026-09-30-audit-host-design.md
+++ b/docs/superpowers/specs/2026-09-30-audit-host-design.md
@@ -216,6 +216,13 @@ Findings: `High` severity if protection is absent or enforces zero contexts. **V
 GitHub-API-backed check fits the same `NetworkRequired`/`sweep` bucket as a registry-backed one without
 needing a third axis.
 
+**`ci-protection-presence` implemented 2026-10-02** (`docs/superpowers/plans/2026-10-02-ci-protection-presence.md`).
+`Project` scope, `NetworkRequired`, so `sweep` tier; GitHub only (`gh api`, read-only). A finding (`High`/`High`) when the
+default branch's `protection.required_status_checks.enforcement_level` is `off` or it requires zero `contexts`; `non_admins`
+(admins may bypass) and `everyone` pass; `protected` is never consulted. GitHub is reached through a `Github` trait so the
+controls need no network; the canned responses are copies of real ones. Unreadable settings are an error, never clean. Not
+built: the `sweep` runner that decides where and how often this runs, and non-GitHub forges.
+
 **`unused-return-values`** — *`Project` scope, `LocalOnly`, `fast` tier (for Rust; see below).* Added
 2026-10-01, CireSnave's call: of the two value-flow checks sketched here, this one ships first —
 genuinely useful and not well-covered by existing tooling in any language this host is likely to target.
diff --git a/tests/cli.rs b/tests/cli.rs
--- a/tests/cli.rs
+++ b/tests/cli.rs
@@ -426,3 +426,21 @@ fn the_version_consistency_check_runs_by_id_and_reports_the_outlier() {
         ))
         .stdout(predicate::str::contains("(version-consistency)"));
 }
+
+#[test]
+fn ci_protection_presence_on_a_non_github_origin_fails_loudly_without_touching_the_network() {
+    let tmp = tempfile::tempdir().unwrap();
+    for args in [
+        vec!["init", "-q"],
+        vec!["remote", "add", "origin", "https://gitlab.com/acme/widgets.git"],
+    ] {
+        StdCommand::new("git").args(args).current_dir(tmp.path()).status().unwrap();
+    }
+    Command::cargo_bin("coderipper")
+        .unwrap()
+        .args(["check", "ci-protection-presence", "--project"])
+        .arg(tmp.path())
+        .assert()
+        .failure()
+        .stderr(predicate::str::contains("not a GitHub repository"));
+}
````

- [ ] **Step 2: Run everything.** Run: `cargo fmt && cargo test --no-fail-fast && cargo clippy --all-targets -- -D warnings`
  Expected: PASS — 145 lib, 8 allowlist, 12 cli, 17 ci-protection, 20 reachability, 11 unused-parameters, 13 unused-return-values, 18 version-consistency, 13 workspace; clippy silent.

- [ ] **Step 3: Live acceptance (manual, read-only; needs `gh` authenticated).** For each URL, make an empty scratch repo, add the remote, and run the built binary:

```bash
d=$(mktemp -d); (cd "$d" && git init -q && git remote add origin https://github.com/<owner>/<repo>.git)
<built-binary> check ci-protection-presence --project "$d"
```

  Expected (measured 2026-10-02, as `ciresnave-bot`): `ciresnave/coderipper` and `ciresnave/fuel` → `coderipper: no issues found`; `ciresnave/smskit` →
  ``[High/High] smskit — ciresnave/smskit: the default branch `main` has no branch protection (ci-protection-presence)``; `ciresnave/ee-amt` and
  `ciresnave/HumboldtUnifiedKidTracker` (private, free plan) → the same `High`; `ciresnave/bayes-optimal` (empty) → `[Info/High] ... has no commits yet ...`;
  `ciresnave/does-not-exist-xyz` → `check error: ... cannot read ciresnave/does-not-exist-xyz from GitHub: Not Found (HTTP 404)`; a `https://gitlab.com/a/b.git` origin → `check error: ... is not a GitHub repository`.

- [ ] **Step 4: Commit, push, report.** `git add -A tests README.md docs && git commit -m "test+docs: ci-protection-presence CLI test, README, spec note"` (+ trailers), push, and report to the PM (who opens and merges the PR and allocates the version). Read the PR's checks and **unresolved review threads** (GraphQL `reviewThreads{isResolved}`) before calling it READY.
