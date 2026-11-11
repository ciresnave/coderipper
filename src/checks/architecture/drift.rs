// SPDX-License-Identifier: MIT OR Apache-2.0
//! Architectural drift (repository scope, Rust): has a crate's structure grown past a budget since the last release?
//!
//! Rule record: `ARC-009` in `rules/arc.toml`. Background: `docs/rules/arc.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 5 (history-based rules are advisory).
//!
//! **What it compares.** For each workspace crate, two numbers, now (`HEAD`) and at a base revision:
//! - **cycles:** the strongly connected sets of two or more modules (the count `MOD-002` reports);
//! - **surface:** the public items plus the members of the public types of the library (the sum `ARC-006` budgets).
//!
//! **The base.** `[arc009] base` in the repository's own `.coderipper.toml` when set (a tag, a branch, a commit); otherwise the newest
//! release tag `v<version>` reachable from `HEAD` that is not `HEAD` itself (`tools/tag-release.sh` makes them). A repository with no
//! such tag and no `base` gets "not run" with what to set, never clean. The base is **read from the object store** (`git ls-tree` and
//! `git show`): nothing is checked out and no worktree is registered.
//!
//! **The budget.** More module cycles than at the base is reported at **Low**. A surface that grew by more than `surface_growth_percent`
//! (default 25) **and** by at least `surface_min_items` (default 10) is reported at **Info**; both bounds, so a small crate adding two
//! items and a large crate adding a handful are not drift. A crate that did not exist at the base is not compared (it is new, not drifting).
//!
//! **Weak evidence by construction (low confidence).** Two numbers do not say whether the growth was wanted: a release that added a feature
//! grows its surface on purpose. The finding gives both numbers and the base, and asks; it does not rank crates or propose a change
//! (patent watch: no cost-growth model, no co-change matrix; see the addendum in `docs/PATENT-SWEEP.md`). The syntactic limits of
//! `MOD-002` and `ARC-006` apply to both readings.
//!
//! **Not done in this version.** The rule record also names instability margins and violations of declared layers over time; those need
//! the crate graph and the declaration at the base and are noted, not compared. Waiver counts and ages need an allowlist history.

use super::api_budget::surface;
use super::cycles::sccs;
use super::modcycles::module_edges;
use super::modgraph;
use super::read_graph;
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, read_sources_at};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_009;

/// Reports crates whose module cycles or public surface grew past the budget since the last release (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct DriftCheck;

impl DriftCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// The budget (`[arc009]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Budget {
    pub growth_percent: usize,
    pub min_items: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            growth_percent: 25,
            min_items: 10,
        }
    }
}

/// What one crate came to at one revision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Metrics {
    pub cycles: usize,
    pub surface: usize,
}

/// One crate's growth past the budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Growth {
    Cycles { from: usize, to: usize },
    Surface { from: usize, to: usize },
}

/// What a crate's two readings say about the budget.
pub(crate) fn compare(base: Metrics, head: Metrics, budget: Budget) -> Vec<Growth> {
    let mut out = Vec::new();
    if head.cycles > base.cycles {
        out.push(Growth::Cycles {
            from: base.cycles,
            to: head.cycles,
        });
    }
    let grew = head.surface.saturating_sub(base.surface);
    if grew >= budget.min_items && grew * 100 > base.surface * budget.growth_percent {
        out.push(Growth::Surface {
            from: base.surface,
            to: head.surface,
        });
    }
    out
}

impl Check for DriftCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn not_applicable(&self, ctx: &CheckContext) -> Option<String> {
        match base_of(&ctx.project_root) {
            Ok(Some(_)) => None,
            // a repository that has never published a release has nothing to drift from; to compare with something else, set a base
            Ok(None) => Some("ARC-009 compares the crates with the last release, and no release tag `v<version>` is reachable from HEAD besides HEAD itself (a repository that has not published has nothing to drift from; it is not clean, it is not compared); once a release is tagged (`tools/tag-release.sh`) it runs, or set `[arc009] base = \"<tag, branch or commit>\"` in .coderipper.toml to compare with something now".into()),
            // an unreadable repository is an error of the run, raised by `run`
            Err(_) => None,
        }
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let root = &ctx.project_root;
        let Some(base) = base_of(root)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        let settings = std::fs::read_to_string(root.join(crate::config::FILE_NAME))
            .map(|t| crate::config::arc009_settings(&t))
            .unwrap_or_default();
        let budget = Budget {
            growth_percent: settings
                .surface_growth_percent
                .unwrap_or(Budget::default().growth_percent),
            min_items: settings
                .surface_min_items
                .unwrap_or(Budget::default().min_items),
        };
        let graph = read_graph(root)?;
        let idents: BTreeSet<String> = graph
            .members
            .iter()
            .map(|m| m.name.replace('-', "_"))
            .collect();
        let project = root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut notes = Vec::new();
        let mut findings = Vec::new();
        let mut compared = 0usize;
        for m in &graph.members {
            let dir = root.join(&m.dir);
            let at_base = read_sources_at(&dir, &base.rev);
            let Ok(base_files) = at_base else {
                notes.push(format!(
                    "crate `{}` is not at {}, so it is new and not compared",
                    m.name, base.name
                ));
                continue;
            };
            if base_files.is_empty() {
                notes.push(format!(
                    "crate `{}` has no tracked source under src/ at {}, so it is new and not compared",
                    m.name, base.name
                ));
                continue;
            }
            let head_files = read_sources(&dir)?;
            let before = metrics(&base_files, &m.dir, &idents)?;
            let after = metrics(&head_files, &m.dir, &idents)?;
            compared += 1;
            for g in compare(before, after, budget) {
                findings.push(finding(&project, &m.name, &m.manifest, &g, &base, budget));
            }
        }
        notes.push(format!(
            "ARC-009 compared {compared} crate(s) of {} with {} ({}): module cycles and public surface; instability margins and violations of declared layers are not compared in this version",
            graph.members.len(),
            base.name,
            base.sha.get(..8).unwrap_or(&base.sha)
        ));
        Ok((findings, notes))
    }
}

/// The two numbers of one crate from its sources.
pub(crate) fn metrics(
    files: &[crate::checks::rust_sources::SourceFile],
    dir: &str,
    idents: &BTreeSet<String>,
) -> anyhow::Result<Metrics> {
    let cycles = match modgraph::read(files)? {
        Some(facts) => {
            let (edges, nodes) = module_edges(&facts);
            sccs(&nodes, &edges.keys().cloned().collect()).len()
        }
        None => 0,
    };
    let (totals, _) = surface(files, dir, idents)?;
    Ok(Metrics {
        cycles,
        surface: totals.items.values().sum::<usize>() + totals.members,
    })
}

fn finding(
    project: &str,
    krate: &str,
    manifest: &str,
    g: &Growth,
    base: &Base,
    budget: Budget,
) -> Finding {
    let (kind, severity, summary, detail) = match g {
        Growth::Cycles { from, to } => (
            "cycles",
            Severity::Low,
            format!(
                "crate `{krate}` has more cycles between modules than at {}",
                base.name
            ),
            format!(
                "Strongly connected sets of two or more modules in `{krate}`: {from} at {} ({}), {to} now. Run `MOD-002` for the modules and edges. A cycle that appeared since the last release is the cheapest to break while its cause is still in memory; to accept it, allowlist the subject `{krate}:cycles` with a reason.",
                base.name,
                base.sha.get(..8).unwrap_or(&base.sha)
            ),
        ),
        Growth::Surface { from, to } => (
            "surface",
            Severity::Info,
            format!(
                "the public surface of crate `{krate}` grew past its budget since {}",
                base.name
            ),
            format!(
                "Public items plus members of the public types of `{krate}`: {from} at {} ({}), {to} now, growth {} item(s), over the budget of {}% and {} item(s) (`[arc009] surface_growth_percent` and `surface_min_items`). A release that adds a feature grows its surface on purpose; the question is whether this growth was decided. `ARC-006` budgets the surface of a declared component; to accept this, allowlist the subject `{krate}:surface` with a reason.",
                base.name,
                base.sha.get(..8).unwrap_or(&base.sha),
                to - from,
                budget.growth_percent,
                budget.min_items
            ),
        ),
    };
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity,
        confidence: Confidence::Low,
        project: project.into(),
        location: Some(Location {
            file: manifest.into(),
            line: None,
        }),
        subject: Some(format!("{krate}:{kind}")),
        summary,
        detail,
        member: None,
        positive_control: None,
    }
}

/// The revision the crates are compared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Base {
    /// What it is called in messages (`v0.9.0`, `origin/main`).
    pub name: String,
    pub rev: String,
    pub sha: String,
}

/// The base revision, or `None` when there is none to find (see the module docs).
pub(crate) fn base_of(root: &Path) -> anyhow::Result<Option<Base>> {
    let configured = std::fs::read_to_string(root.join(crate::config::FILE_NAME))
        .ok()
        .and_then(|t| crate::config::arc009_settings(&t).base);
    let name = match configured {
        Some(b) => b,
        None => match newest_release_tag(root) {
            Some(t) => t,
            None => return Ok(None),
        },
    };
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{name}^{{commit}}")],
    )
    .map_err(|e| {
        anyhow::anyhow!("ARC-009: the base `{name}` is not a revision of this repository: {e}")
    })?;
    Ok(Some(Base {
        rev: sha.clone(),
        name,
        sha,
    }))
}

/// The newest `v<digit>..` tag reachable from `HEAD` whose commit is not `HEAD` itself.
fn newest_release_tag(root: &Path) -> Option<String> {
    let head = git(root, &["rev-parse", "HEAD^{commit}"]).ok()?;
    let described = |rev: &str| {
        git(
            root,
            &[
                "describe",
                "--tags",
                "--match",
                "v[0-9]*",
                "--abbrev=0",
                rev,
            ],
        )
        .ok()
    };
    let tag = described("HEAD")?;
    let at = git(root, &["rev-parse", &format!("{tag}^{{commit}}")]).ok()?;
    if at != head {
        return Some(tag);
    }
    described("HEAD^")
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    anyhow::ensure!(
        out.status.success(),
        "git {} failed in {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests;
