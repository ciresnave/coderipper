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
