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
//! `off` or no context is required; `non_admins` (admins may bypass) and `everyone` both pass. A repository
//! RULESET that requires checks (GitHub's newer mechanism, which needs no classic protection) also passes, so
//! a branch is only called unprotected after `rules/branches/<branch>` has been read too.
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
use crate::github::{origin_url, parse_github_remote, GhCli, Github};

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
        let slug = slug.as_str();

        let info = self
            .api
            .get(&format!("repos/{slug}"))
            .map_err(|e| anyhow::anyhow!("cannot read {slug} from GitHub: {e}"))?;
        if info["archived"].as_bool() == Some(true) {
            return Ok(vec![info_finding(
                &canonical_name(&info, slug),
                None,
                format!("{slug} is archived, so branch protection is not applicable"),
                format!("GET repos/{slug} reported `archived: true`"),
            )]);
        }
        let branch = info["default_branch"].as_str().ok_or_else(|| {
            anyhow::anyhow!("GitHub's answer for {slug} has no `default_branch`; cannot tell which branch to read")
        })?;

        // The name an `[[allow]]` entry is written with is GitHub's, not the origin URL's spelling.
        let canonical = canonical_name(&info, slug);

        let branch_path = format!("repos/{slug}/branches/{}", encode_path(branch));
        let doc = match self.api.get(&branch_path) {
            Ok(doc) => doc,
            Err(e) if e.status == Some(404) => {
                // A 404 alone is not "empty": a deleted default branch answers it too. Only the
                // repository itself saying it has no commits makes this informational.
                let commits_path = format!("repos/{slug}/commits?per_page=1");
                return match self.api.get(&commits_path) {
                    Err(c) if c.status == Some(409) && c.message.contains("empty") => {
                        Ok(vec![info_finding(&canonical,
                            Some(branch),
                            format!("{canonical} has no commits yet, so its default branch `{branch}` does not exist to protect"),
                            format!("GET {branch_path} answered 404 and GET {commits_path} answered 409 \"{}\"", c.message),
                        )])
                    }
                    Err(c) => anyhow::bail!(
                        "GET {branch_path} answered 404 and the repository's emptiness could not be confirmed: \
                         cannot read {commits_path} from GitHub: {c}"
                    ),
                    Ok(_) => anyhow::bail!(
                        "GET {branch_path} answered 404 but GET {commits_path} shows the repository has commits, \
                         so the default branch `{branch}` is unreadable, not absent; not reporting clean"
                    ),
                };
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
        // Settings that are present but unreadable must not be guessed into "off" (a false High).
        let level = match checks["enforcement_level"].as_str() {
            Some(level) => level,
            None if checks.is_object() => anyhow::bail!(
                "GitHub's answer for {branch_path} has `required_status_checks` without an \
                 `enforcement_level`; cannot tell whether it is enforced"
            ),
            None => "off",
        };
        // GitHub reports the requirement twice (`contexts`, `checks`); either list counts.
        let required = ["contexts", "checks"]
            .iter()
            .filter_map(|key| checks[*key].as_array().map(Vec::len))
            .max()
            .unwrap_or(0);

        if level != "off" && required > 0 {
            return Ok(Vec::new());
        }

        // Classic branch protection requires nothing. A repository ruleset can require checks WITHOUT any
        // classic protection, so look there before calling the branch unprotected.
        // (100 is the page maximum; a branch with more rules than that is not a case worth a pager.)
        let rules_path = format!(
            "repos/{slug}/rules/branches/{}?per_page=100",
            encode_path(branch)
        );
        let rules_note = match self.api.get(&rules_path) {
            Ok(rules) => {
                let list = rules.as_array().ok_or_else(|| {
                    anyhow::anyhow!(
                        "GitHub's answer for {rules_path} is not a list of rules; not reporting clean"
                    )
                })?;
                if ruleset_requires_checks(list) {
                    return Ok(Vec::new());
                }
                format!(
                    "GET {rules_path}: {} rule(s), none requiring status checks or workflows",
                    list.len()
                )
            }
            // Real answer for a private repo on a free plan: rulesets cannot exist there.
            Err(e) if e.status == Some(403) && e.message.contains("Upgrade to GitHub Pro") => {
                format!(
                    "GET {rules_path} answered 403 \"{}\": this repository's plan cannot have rulesets",
                    e.message
                )
            }
            Err(e) => anyhow::bail!("cannot read {rules_path} from GitHub: {e}"),
        };

        let sha = doc["commit"]["sha"].as_str().unwrap_or("?");
        let sha = &sha[..sha.len().min(7)];
        let flag = doc["protected"].as_bool();
        let summary = if enabled {
            format!(
                "{canonical}: the default branch `{branch}` is protected but requires no status checks \
                 (enforcement `{level}`, {required} required)"
            )
        } else {
            format!("{canonical}: the default branch `{branch}` has no branch protection")
        };
        let detail = format!(
            "CireSnave's rule: every repo gets CI and branch protection that REQUIRES it. GitHub reports \
             `protection.enabled: {enabled}`, `required_status_checks.enforcement_level: \"{level}\"` and \
             {required} required context(s) for `{branch}`. No active ruleset requires status checks either \
             ({rules_note}). (`protected: {}` is not \
             evidence: it is true for a branch whose rules require no checks.)",
            flag.map_or("absent".to_string(), |f| f.to_string())
        );
        Ok(vec![Finding {
            check_id: CHECK_ID.into(),
            severity: Severity::High,
            confidence: Confidence::High,
            project: project_of(&canonical),
            location: Some(Location {
                file: SETTINGS_FILE.into(),
                line: None,
            }),
            subject: Some(format!("{canonical}@{branch}")),
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

/// The repository-name half of `owner/repo`: what every other check calls the project.
fn project_of(slug: &str) -> String {
    slug.rsplit('/').next().unwrap_or(slug).to_string()
}

/// GitHub's own spelling of `owner/repo` (an origin may be spelled `Acme/Widgets`).
fn canonical_name(info: &serde_json::Value, fallback: &str) -> String {
    info["full_name"].as_str().unwrap_or(fallback).to_string()
}

/// Percent-encode a branch name for a URL path: everything but unreserved characters and `/`
/// (`dev#2` would otherwise be read as `dev` plus a fragment).
fn encode_path(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Does any active ruleset rule require CI: at least one status check, or a required workflow?
fn ruleset_requires_checks(rules: &[serde_json::Value]) -> bool {
    let non_empty = |v: &serde_json::Value| v.as_array().is_some_and(|a| !a.is_empty());
    rules.iter().any(|rule| match rule["type"].as_str() {
        Some("required_status_checks") => non_empty(&rule["parameters"]["required_status_checks"]),
        Some("workflows") => non_empty(&rule["parameters"]["workflows"]),
        _ => false,
    })
}

fn info_finding(slug: &str, branch: Option<&str>, summary: String, control: String) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        severity: Severity::Info,
        confidence: Confidence::High,
        project: project_of(slug),
        location: Some(Location {
            file: SETTINGS_FILE.into(),
            line: None,
        }),
        subject: Some(match branch {
            Some(b) => format!("{slug}@{b}"),
            None => slug.to_string(),
        }),
        detail: "Nothing to protect: this is informational, not a defect.".into(),
        summary,
        positive_control: Some(control),
    }
}
