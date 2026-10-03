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
            // no rulesets unless a test says otherwise (the real answer for the whole portfolio)
            if path.contains("/rules/branches/") {
                return Ok(json!([]));
            }
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
            "repos/acme/widgets/branches/trunk".to_string(),
            // classic protection requires nothing, so the rulesets are read before the verdict
            "repos/acme/widgets/rules/branches/trunk".to_string(),
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

// ---- repository rulesets: GitHub's newer way to require checks, with no classic branch protection ----

fn rules_requiring_checks() -> Value {
    json!([
        { "type": "deletion", "ruleset_source_type": "Repository", "ruleset_id": 1 },
        {
            "type": "required_status_checks",
            "parameters": {
                "strict_required_status_checks_policy": false,
                "required_status_checks": [ { "context": "ci / build" } ]
            },
            "ruleset_source_type": "Repository",
            "ruleset_id": 1
        }
    ])
}

const RULES: &str = "repos/acme/widgets/rules/branches/main";

#[test]
fn a_ruleset_that_requires_checks_counts_even_without_classic_protection() {
    // Without this a repo protected only by a ruleset would be reported as unprotected: a false High.
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Ok(rules_requiring_checks())),
        ],
    );
    assert!(result.unwrap().is_empty());
    assert!(asked.contains(&RULES.to_string()), "{asked:?}");
}

#[test]
fn rules_that_require_no_status_checks_do_not_rescue_an_unprotected_branch() {
    let rules =
        json!([{ "type": "deletion", "ruleset_source_type": "Repository", "ruleset_id": 1 }]);
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Ok(rules)),
        ],
    );
    let found = result.unwrap();
    assert_eq!(subjects(&found), vec!["acme/widgets@main"]);
    assert!(found[0].detail.contains("ruleset"), "{}", found[0].detail);
}

#[test]
fn a_plan_that_cannot_have_rulesets_is_not_an_error() {
    // Real answer for a private repo on a free plan: 403 "Upgrade to GitHub Pro ...". Rulesets cannot
    // exist there, so the classic reading stands.
    let upgrade = ApiError {
        status: Some(403),
        message: "Upgrade to GitHub Pro or make this repository public to enable this feature."
            .into(),
    };
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Err(upgrade)),
        ],
    );
    assert_eq!(subjects(&result.unwrap()), vec!["acme/widgets@main"]);
}

#[test]
fn a_rules_lookup_that_fails_for_another_reason_is_an_error_never_a_verdict() {
    let boom = ApiError {
        status: Some(500),
        message: "Server Error".into(),
    };
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Err(boom)),
        ],
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("500") && err.contains("rules"), "{err}");
}

#[test]
fn rules_are_not_even_asked_for_when_classic_protection_already_requires_checks() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(protected_branch())),
        ],
    );
    assert!(result.unwrap().is_empty());
    assert!(!asked.iter().any(|p| p.contains("/rules/")), "{asked:?}");
}
