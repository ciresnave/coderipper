// SPDX-License-Identifier: MIT OR Apache-2.0
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
            // the portfolio's settings unless a test says otherwise
            if path.ends_with("/protection") {
                return Ok(branch_settings(false, false, false));
            }
            Err(ApiError::new(
                Some(404),
                format!("no canned answer for {path}"),
            ))
        })
    }
}

/// A git command that cannot be redirected by an inherited `GIT_DIR`, and that must succeed.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn repo_with_origin(url: Option<&str>) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init", "-q"]);
    if let Some(url) = url {
        git(tmp.path(), &["remote", "add", "origin", url]);
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
    let result = check.run(&CheckContext::new(dir.to_path_buf()));
    let asked = asked.lock().unwrap().clone();
    (result, asked)
}

fn repo_info(default_branch: &str) -> Value {
    json!({
        "default_branch": default_branch, "archived": false, "private": false, "fork": false,
        "allow_squash_merge": true, "allow_merge_commit": false, "allow_rebase_merge": false,
        "delete_branch_on_merge": true
    })
}

/// Real shape of `GET branches/main/protection` (the fields the settings audit reads).
fn branch_settings(enforce_admins: bool, force_pushes: bool, deletions: bool) -> Value {
    json!({
        "enforce_admins": { "enabled": enforce_admins },
        "allow_force_pushes": { "enabled": force_pushes },
        "allow_deletions": { "enabled": deletions }
    })
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
const COMMITS: &str = "repos/acme/widgets/commits?per_page=1";

/// Real answer for a repository with no commits: `409 "Git Repository is empty."`.
fn empty_repository() -> ApiError {
    ApiError::new(Some(409), "Git Repository is empty.")
}

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
            "repos/acme/widgets/rules/branches/trunk?per_page=100".to_string(),
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
    let not_found = ApiError::new(Some(404), "Branch not found");
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Err(not_found)),
            (COMMITS, Err(empty_repository())),
        ],
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
    let forbidden = ApiError::new(Some(403), "API rate limit exceeded");
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

#[test]
fn fast_does_not_run_it_and_sweep_does() {
    // A GitLab origin makes the check fail without any network, which makes "was it run?" observable.
    let repo = repo_with_origin(Some("https://gitlab.com/acme/widgets.git"));
    let ctx = CheckContext::new(repo.path().to_path_buf());
    let mentions = |errors: &[String]| errors.iter().any(|e| e.contains("ci-protection-presence"));
    let fast = coderipper::run_checks(&ctx, Tier::Fast, None);
    assert!(!mentions(&fast.errors), "{:?}", fast.errors);
    let sweep = coderipper::run_checks(&ctx, Tier::Sweep, None);
    assert!(mentions(&sweep.errors), "{:?}", sweep.errors);
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

const RULES: &str = "repos/acme/widgets/rules/branches/main?per_page=100";

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
    let upgrade = ApiError::new(
        Some(403),
        "Upgrade to GitHub Pro or make this repository public to enable this feature.",
    );
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
    let boom = ApiError::new(Some(500), "Server Error");
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

// ---- review fixes ----

#[test]
fn a_404_on_the_branch_is_only_informational_when_the_repository_proves_it_is_empty() {
    // Review finding: any 404 used to become "no commits yet", exit 0, with nothing confirming it.
    let repo = repo_with_origin(Some(ORIGIN));
    let not_found = ApiError::new(Some(404), "Branch not found");
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Err(not_found.clone())),
            (COMMITS, Ok(json!([{ "sha": "abc1234" }]))),
        ],
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("404") && err.contains("commits"), "{err}");

    // and when the emptiness check itself fails, that is an error too
    let down = ApiError::new(Some(500), "Server Error");
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Err(not_found)),
            (COMMITS, Err(down)),
        ],
    );
    assert!(result.unwrap_err().to_string().contains("500"));
}

#[test]
fn a_branch_name_is_percent_encoded_in_the_url_except_for_slashes() {
    // `#` would otherwise start a fragment: `dev#2` would be read as `dev`.
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("release/dev#2 x"))),
            (
                "repos/acme/widgets/branches/release/dev%232%20x",
                Ok(unprotected_branch()),
            ),
        ],
    );
    assert_eq!(
        subjects(&result.unwrap()),
        vec!["acme/widgets@release/dev#2 x"]
    );
    assert_eq!(asked[1], "repos/acme/widgets/branches/release/dev%232%20x");
    assert_eq!(
        asked[2],
        "repos/acme/widgets/rules/branches/release/dev%232%20x?per_page=100"
    );
}

#[test]
fn the_subject_uses_githubs_canonical_name_not_the_origins_spelling() {
    // An `[[allow]]` entry is written with the canonical name; an origin spelled `Acme/Widgets` must match it.
    let repo = repo_with_origin(Some("git@github.com:Acme/Widgets.git"));
    let mut info = repo_info("main");
    info["full_name"] = json!("acme/widgets");
    let (result, _) = run_with(
        repo.path(),
        vec![
            ("repos/Acme/Widgets", Ok(info)),
            ("repos/Acme/Widgets/branches/main", Ok(unprotected_branch())),
        ],
    );
    let found = result.unwrap();
    assert_eq!(subjects(&found), vec!["acme/widgets@main"]);
    assert_eq!(found[0].project, "widgets");
}

#[test]
fn required_status_checks_without_an_enforcement_level_is_an_error_not_a_guess() {
    let mut branch = protected_branch();
    branch["protection"]["required_status_checks"]
        .as_object_mut()
        .unwrap()
        .remove("enforcement_level");
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(branch))],
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("enforcement_level"), "{err}");
}

#[test]
fn the_larger_of_contexts_and_checks_counts() {
    // `contexts` empty but `checks` listing one: GitHub's two lists must not be able to disagree into a High.
    let mut branch = protected_branch();
    branch["protection"]["required_status_checks"]["contexts"] = json!([]);
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![(REPO, Ok(repo_info("main"))), (BRANCH, Ok(branch))],
    );
    assert!(result.unwrap().is_empty());
}

#[test]
fn a_required_workflows_ruleset_rule_also_requires_ci() {
    // Shape from GitHub's documentation of the `workflows` rule type (not yet seen on a real repo here).
    let rules = json!([{
        "type": "workflows",
        "parameters": { "workflows": [ { "path": ".github/workflows/ci.yml", "repository_id": 1 } ] },
        "ruleset_source_type": "Organization",
        "ruleset_id": 7
    }]);
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Ok(rules)),
        ],
    );
    assert!(result.unwrap().is_empty());
}

#[test]
fn a_rules_reply_that_is_not_a_list_is_an_error() {
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Ok(json!({ "message": "surprise" }))),
        ],
    );
    assert!(result.unwrap_err().to_string().contains("not a list"));
}

#[test]
fn the_finding_says_what_the_rules_lookup_actually_answered() {
    let upgrade = ApiError::new(
        Some(403),
        "Upgrade to GitHub Pro or make this repository public to enable this feature.",
    );
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(unprotected_branch())),
            (RULES, Err(upgrade)),
        ],
    );
    let detail = result.unwrap()[0].detail.clone();
    assert!(detail.contains("cannot have rulesets"), "{detail}");
    assert!(
        !detail.contains("0 rule(s)"),
        "must not claim a rule count it never read: {detail}"
    );
}

const PROTECTION: &str = "repos/acme/widgets/branches/main/protection";

/// Runs over a protected branch whose repository document is `info` and whose protection document is `settings`.
fn audit(info: Value, settings: Value) -> (anyhow::Result<Vec<Finding>>, Vec<String>) {
    let repo = repo_with_origin(Some(ORIGIN));
    run_with(
        repo.path(),
        vec![
            (REPO, Ok(info)),
            (BRANCH, Ok(protected_branch())),
            (PROTECTION, Ok(settings)),
        ],
    )
}

#[test]
fn the_portfolios_own_settings_are_clean_the_negative_control() {
    let (result, asked) = audit(repo_info("main"), branch_settings(false, false, false));
    assert!(result.unwrap().is_empty());
    assert!(asked.iter().any(|p| p == PROTECTION), "{asked:?}");
}

#[test]
fn each_setting_that_differs_is_its_own_finding_the_positive_control() {
    let mut info = repo_info("main");
    info["allow_merge_commit"] = json!(true);
    info["allow_rebase_merge"] = json!(true);
    info["allow_squash_merge"] = json!(false);
    info["delete_branch_on_merge"] = json!(false);
    let (result, _) = audit(info, branch_settings(true, true, true));
    let findings = result.unwrap();
    assert_eq!(findings.len(), 7, "{findings:#?}");
    assert!(findings.iter().all(|f| f.severity == Severity::Medium));
    let text: String = findings.iter().map(|f| f.detail.clone()).collect();
    for key in [
        "allow_squash_merge",
        "allow_merge_commit",
        "allow_rebase_merge",
        "delete_branch_on_merge",
        "enforce_admins",
        "allow_force_pushes",
        "allow_deletions",
    ] {
        assert!(text.contains(&format!("`{key}")), "{key} in {text}");
    }
    let subjects = subjects(&findings);
    assert_eq!(
        subjects
            .iter()
            .filter(|s| *s == "acme/widgets@main")
            .count(),
        3
    );
    assert_eq!(subjects.iter().filter(|s| *s == "acme/widgets").count(), 4);
}

#[test]
fn admins_who_can_bypass_are_the_wanted_state_not_a_finding() {
    // CireSnave 2026-09-18: enforce_admins is OFF on every protected branch.
    let (result, _) = audit(repo_info("main"), branch_settings(false, false, false));
    assert!(result.unwrap().is_empty());
    let (result, _) = audit(repo_info("main"), branch_settings(true, false, false));
    let findings = result.unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].summary.contains("admins cannot bypass"));
}

#[test]
fn merge_settings_a_token_cannot_see_are_an_error_never_clean() {
    let mut info = repo_info("main");
    info.as_object_mut().unwrap().remove("allow_squash_merge");
    let (result, _) = audit(info, branch_settings(false, false, false));
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("allow_squash_merge"), "{message}");
}

#[test]
fn a_protection_reply_without_a_setting_is_an_error_and_a_failed_read_is_one_too() {
    let (result, _) = audit(
        repo_info("main"),
        json!({ "enforce_admins": { "enabled": false } }),
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("allow_force_pushes"));

    let repo = repo_with_origin(Some(ORIGIN));
    let (result, _) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(repo_info("main"))),
            (BRANCH, Ok(protected_branch())),
            (
                PROTECTION,
                Err(ApiError::new(
                    Some(403),
                    "Must have admin rights to Repository.",
                )),
            ),
        ],
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("admin"));
}

#[test]
fn a_fork_is_not_audited_at_all() {
    let mut info = repo_info("main");
    info["fork"] = json!(true);
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(repo.path(), vec![(REPO, Ok(info))]);
    let findings = result.unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].summary.contains("fork"));
    assert_eq!(asked, vec![REPO.to_string()]);
}

#[test]
fn a_branch_governed_only_by_rulesets_gets_the_repository_audit_alone() {
    let mut info = repo_info("main");
    info["allow_merge_commit"] = json!(true);
    let repo = repo_with_origin(Some(ORIGIN));
    let (result, asked) = run_with(
        repo.path(),
        vec![
            (REPO, Ok(info)),
            (BRANCH, Ok(unprotected_branch())),
            (
                "repos/acme/widgets/rules/branches/main?per_page=100",
                Ok(rules_requiring_checks()),
            ),
        ],
    );
    let findings = result.unwrap();
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].detail.contains("allow_merge_commit"));
    assert!(!asked.iter().any(|p| p == PROTECTION), "{asked:?}");
}
