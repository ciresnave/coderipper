// SPDX-License-Identifier: MIT OR Apache-2.0
//! Signs of a distributed monolith (repository scope, Rust, committed history): do two top-level modules of a crate refer to each
//! other **and** change together?
//!
//! Rule record: `ARC-011` in `rules/arc.toml`. Background: `docs/rules/arc.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md` (history-based rules are advisory). A repository of services is not
//! visible to this check: the unit it judges is the top-level module of a crate, the smallest piece the rest of the rules already
//! treat as a component (`MOD-002` reports its cycles), and the "services" of a Rust workspace cannot refer to each other in a loop
//! (cargo refuses a cycle of crates), so the mutual reference that makes a monolith in disguise can only be between modules.
//!
//! **What it reads.** For each workspace crate, the module graph (`modgraph`, the same edges `MOD-002` reports) reduced to top-level
//! modules, and `git log` over the newest `window` non-merge commits of the crate's directory (default 500; no network, no checkout), as
//! `MOD-006` reads it. A commit that touches more than `max_files` files (default 25) is a bulk change and is left out.
//!
//! **What it computes.** For each pair of top-level modules `A` and `B` such that a path written in `A` names an item of `B` **and** a
//! path written in `B` names an item of `A`: the commits that touched a file of `A` (the files of `A`, its submodules included; the
//! crate root file is nobody's), the commits that touched a file of `B`, how many touched both and how many touched either. The pair is
//! reported when at least `min_commits` commits (default 3) touched both and they are at least `min_share_percent` (default 60) of
//! the commits that touched either: two modules that cannot be understood apart and are never edited apart.
//!
//! **Weak evidence by construction (low confidence, Info).** The same limits as `MOD-006`: commit granularity decides, a repository that
//! squash-merges records one commit per pull request so unrelated modules appear together, and a young module has few commits. The
//! finding names the two edges (with file and line) and the commits, and says what to look at: merge the two modules, or decide the
//! direction and remove the back reference. It does not rank pairs, score them or generate a refactoring (patent watch, the Infosys
//! row of `docs/PATENT-SWEEP.md`, US20100070948A1: no generated prescriptions, no evaluation of their effect, no automatic change), and it
//! reads no defect or issue identifier from a commit message (the IBM rows).
//!
//! **Known limitations / deliberate scope:** only direct mutual references between two top-level modules are read, not a longer loop
//! through a third; the crate root and `#[path]` modules are not read as `MOD-002` does not read them; renames are not followed; a
//! shallow clone cuts the history, which is said in a note; a pair that does not refer to each other is silent, whatever its history.

use super::divergent::{git, read_history, Commit};
use super::modcycles::{module_edges, Site};
use super::modgraph;
use super::read_graph;
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::read_sources;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_011;

/// Reports pairs of modules that refer to each other and change together (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct EntangledCheck;

impl EntangledCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// The window and thresholds (`[arc011]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Params {
    pub window: usize,
    pub min_commits: usize,
    pub min_share_percent: usize,
    pub max_files: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            window: 500,
            min_commits: 3,
            min_share_percent: 60,
            max_files: 25,
        }
    }
}

impl Params {
    fn from(s: &crate::config::Arc011Settings) -> Self {
        let d = Self::default();
        Self {
            window: s.window.unwrap_or(d.window).max(1),
            min_commits: s.min_commits.unwrap_or(d.min_commits).max(1),
            min_share_percent: s.min_share_percent.unwrap_or(d.min_share_percent).min(100),
            max_files: s.max_files.unwrap_or(d.max_files).max(1),
        }
    }
}

/// A pair of top-level modules that refer to each other and were changed together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entangled {
    pub a: String,
    pub b: String,
    /// Commits that touched a file of both.
    pub both: usize,
    /// Commits that touched a file of either.
    pub either: usize,
    /// The newest commits that touched both, newest first (up to three).
    pub sample: Vec<String>,
    /// Where `A` first names `B`, and where `B` first names `A`.
    pub a_to_b: Site,
    pub b_to_a: Site,
}

/// What the run read, for the positive control in the notes.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub read: usize,
    pub bulk: usize,
    /// Pairs of top-level modules that refer to each other and were judged.
    pub pairs: usize,
}

impl Check for EntangledCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let root = &ctx.project_root;
        if !root.join("Cargo.toml").is_file() {
            return Ok((
                Vec::new(),
                vec!["ARC-011 reads Rust crates and there is no Cargo.toml at the root, so it judged nothing".into()],
            ));
        }
        let params = Params::from(
            &std::fs::read_to_string(root.join(crate::config::FILE_NAME))
                .map(|t| crate::config::arc011_settings(&t))
                .unwrap_or_default(),
        );
        let project = root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let graph = read_graph(root)?;
        let mut notes = Vec::new();
        if git(root, &["rev-parse", "--is-shallow-repository"])
            .is_ok_and(|o| String::from_utf8_lossy(&o).trim() == "true")
        {
            notes.push("ARC-011: this is a shallow clone, so the history is cut at its depth and what follows is a lower bound".into());
        }
        let mut findings = Vec::new();
        let mut total = Tally::default();
        for m in &graph.members {
            let dir = root.join(&m.dir);
            let files = read_sources(&dir)?;
            let Some(facts) = modgraph::read(&files)? else {
                continue;
            };
            let (edges, _) = module_edges(&facts);
            let pairs = mutual_pairs(&edges);
            if pairs.is_empty() {
                continue;
            }
            let commits = read_history(&dir, params.window)?;
            let (found, tally) = analyse(&pairs, &files_by_top(&facts), &commits, params);
            total.read = total.read.max(tally.read);
            total.bulk = total.bulk.max(tally.bulk);
            total.pairs += tally.pairs;
            findings.extend(
                found
                    .iter()
                    .map(|e| judge(&project, &m.name, &m.dir, e, params)),
            );
        }
        notes.push(format!(
            "ARC-011 read {} crate(s); {} pair(s) of top-level modules refer to each other and were compared with the newest {} commit(s) of their crate (the longest history read: {}, of which {} bulk commit(s) touching more than {} files were set aside)",
            graph.members.len(),
            total.pairs,
            params.window,
            total.read,
            total.bulk,
            params.max_files
        ));
        Ok((findings, notes))
    }
}

fn top(module: &str) -> &str {
    module.split("::").next().unwrap_or(module)
}

/// The pairs `(a, b)`, `a < b`, of top-level modules such that a path written in `a` names `b` and one written in `b` names `a`, with
/// the place each was first written.
pub(crate) fn mutual_pairs(
    edges: &BTreeMap<(String, String), Site>,
) -> BTreeMap<(String, String), (Site, Site)> {
    let mut directed: BTreeMap<(String, String), Site> = BTreeMap::new();
    for ((from, to), site) in edges {
        let (f, t) = (top(from), top(to));
        if f != t {
            directed
                .entry((f.to_string(), t.to_string()))
                .or_insert_with(|| site.clone());
        }
    }
    let mut out = BTreeMap::new();
    for ((a, b), forward) in &directed {
        if a < b {
            if let Some(back) = directed.get(&(b.clone(), a.clone())) {
                out.insert((a.clone(), b.clone()), (forward.clone(), back.clone()));
            }
        }
    }
    out
}

/// The files of each top-level module (its submodules included), without the crate root file.
pub(crate) fn files_by_top(f: &modgraph::Facts) -> BTreeMap<String, BTreeSet<String>> {
    let root = f.mods.get("").cloned().unwrap_or_default();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (module, file) in &f.mods {
        if module.is_empty() || *file == root {
            continue;
        }
        out.entry(top(module).to_string())
            .or_default()
            .insert(file.clone());
    }
    out
}

/// Judges each pair against the history (see the module docs).
pub(crate) fn analyse(
    pairs: &BTreeMap<(String, String), (Site, Site)>,
    files: &BTreeMap<String, BTreeSet<String>>,
    commits: &[Commit],
    params: Params,
) -> (Vec<Entangled>, Tally) {
    let kept: Vec<&Commit> = commits
        .iter()
        .filter(|c| c.files.len() <= params.max_files)
        .collect();
    let mut tally = Tally {
        read: commits.len(),
        bulk: commits.len() - kept.len(),
        pairs: 0,
    };
    let mut out = Vec::new();
    for ((a, b), (a_to_b, b_to_a)) in pairs {
        let (Some(fa), Some(fb)) = (files.get(a), files.get(b)) else {
            continue;
        };
        tally.pairs += 1;
        let mut both = Vec::new();
        let mut either = 0usize;
        for c in &kept {
            let in_a = c.files.iter().any(|f| fa.contains(f));
            let in_b = c.files.iter().any(|f| fb.contains(f));
            if in_a || in_b {
                either += 1;
            }
            if in_a && in_b {
                both.push(c.id.clone());
            }
        }
        if both.len() >= params.min_commits && both.len() * 100 >= either * params.min_share_percent
        {
            out.push(Entangled {
                a: a.clone(),
                b: b.clone(),
                both: both.len(),
                either,
                sample: both.into_iter().take(3).collect(),
                a_to_b: a_to_b.clone(),
                b_to_a: b_to_a.clone(),
            });
        }
    }
    (out, tally)
}

fn judge(project: &str, krate: &str, dir: &str, e: &Entangled, params: Params) -> Finding {
    let repo_path = |file: &str| {
        if dir.is_empty() {
            file.to_string()
        } else {
            format!("{dir}/{file}")
        }
    };
    let ids = e
        .sample
        .iter()
        .map(|id| id.get(..8).unwrap_or(id).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity: Severity::Info,
        confidence: Confidence::Low,
        project: project.into(),
        location: Some(Location {
            file: repo_path(&e.a_to_b.file),
            line: u32::try_from(e.a_to_b.line).ok(),
        }),
        subject: Some(format!("{krate}:{},{}", e.a, e.b)),
        summary: format!(
            "`crate::{}` and `crate::{}` of `{krate}` refer to each other and were changed together in {} of the {} commits that touched either",
            e.a, e.b, e.both, e.either
        ),
        detail: format!(
            "`{a}` names `{b}` at {}:{} and `{b}` names `{a}` at {}:{}. Of the newest {} non-merge commits of the crate, {} touched a file of both and {} touched a file of either ({}%; reported from {} commits and {}%: `[arc011] min_commits` and `min_share_percent`). Newest of them: {ids}. Two modules that cannot be understood or moved apart and are never edited apart are one module split in two (or two services that are one deployment): merge them, or decide which depends on which and remove the back reference (`MOD-002` reports the cycle). Weak evidence: a repository that squash-merges records one commit per pull request, so unrelated modules can appear together. To accept this, allowlist the subject `{krate}:{a},{b}` with a reason.",
            repo_path(&e.a_to_b.file),
            e.a_to_b.line,
            repo_path(&e.b_to_a.file),
            e.b_to_a.line,
            params.window,
            e.both,
            e.either,
            e.both * 100 / e.either.max(1),
            params.min_commits,
            params.min_share_percent,
            a = e.a,
            b = e.b,
        ),
        member: None,
        positive_control: None,
    }
}

#[cfg(test)]
mod tests;
