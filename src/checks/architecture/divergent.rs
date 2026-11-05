// SPDX-License-Identifier: MIT OR Apache-2.0
//! Divergent change (repository scope, committed history): is a source file edited in groups of commits that have nothing to do with each
//! other?
//!
//! Rule record: `MOD-006` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 5 (history-based rules are advisory).
//!
//! **What it reads.** `git log` over the newest `window` non-merge commits of the repository at `HEAD` (default 500; no network, no
//! checkout). A commit that touches more than `max_files` files (default 25) is a bulk change (a rename, a format run, a release) and is
//! left out; a file that more than a fifth of the remaining commits touch (`Cargo.lock`, the changelog, the README) is a file everything
//! changes with and is left out of every commit's file set.
//!
//! **What it computes.** For each source file `M` that exists at `HEAD`, the commits that touched `M` and at least one other file; two of
//! those commits are joined when they touched a source file in common besides `M` (only source files are partners: a README or a
//! workflow edited with a file is the change being documented, not a second reason for the file to change; a registry is no partner
//! either, see below), and the groups are the connected sets (union-find). A group
//! needs at least `min_commits` commits (default 3) to count. `M` is reported when two or more groups count: its history breaks into
//! independent sets of files that never change together.
//!
//! **Which files are judged.** Tracked source files at `HEAD` with a known code extension, except the registries a file is added to
//! whenever something is added next to it (`lib.rs`, `main.rs`, `mod.rs`, `__init__.py`, `index.ts` and `index.js`), build scripts, tests and
//! examples (a path with a `tests`, `test`, `benches` or `examples` component, `tests.rs` and `*_test*` names).
//!
//! **Weak evidence by construction (low confidence, Info).** Commit granularity decides everything: a repository that squash-merges
//! records one commit per pull request, so the files of unrelated changes in one pull request join, and a repository that commits
//! per file hides every co-change. A file that is a real hub (a registry, a table) is edited for many reasons by design. The finding names
//! the groups and the commits and says what to look at; it does not rank files, score them or propose a split (patent watch, the
//! Trimble row of `docs/PATENT-SWEEP.md`, US9268665B2: no fault count, no fault likelihood).
//!
//! **Known limitations / deliberate scope:** renames are not followed (`--no-renames`: a moved file's history starts at the move); a
//! shallow clone cuts the history, which is said in a note; a file with fewer than two counted groups is silent, whatever its history.

use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_006;

/// Reports source files whose history breaks into independent groups of commits (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct DivergentChangeCheck;

impl DivergentChangeCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// The window and thresholds (`[mod006]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Params {
    pub window: usize,
    pub min_commits: usize,
    pub max_files: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            window: 500,
            min_commits: 3,
            max_files: 25,
        }
    }
}

impl Params {
    fn from(s: &crate::config::Mod006Settings) -> Self {
        let d = Self::default();
        Self {
            window: s.window.unwrap_or(d.window).max(1),
            min_commits: s.min_commits.unwrap_or(d.min_commits).max(1),
            max_files: s.max_files.unwrap_or(d.max_files).max(2),
        }
    }
}

/// One commit: its id and the files it touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Commit {
    pub id: String,
    pub files: BTreeSet<String>,
}

/// One group of commits of a file, joined by files they share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub commits: Vec<String>,
    pub files: BTreeSet<String>,
}

/// A file with two or more counted groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Divergent {
    pub file: String,
    /// The counted groups, the largest first.
    pub groups: Vec<Group>,
    /// Commits of the file that were in a group too small to count.
    pub small: usize,
    /// Commits of the file that touched nothing else (once the ubiquitous files are left out).
    pub alone: usize,
}

/// What the history came to.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub read: usize,
    pub bulk: usize,
    pub ubiquitous: Vec<String>,
    pub judged: usize,
}

impl Check for DivergentChangeCheck {
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
        let dir = &ctx.project_root;
        let params = Params::from(
            &std::fs::read_to_string(dir.join(crate::config::FILE_NAME))
                .map(|t| crate::config::mod006_settings(&t))
                .unwrap_or_default(),
        );
        let mut notes = Vec::new();
        let head = String::from_utf8_lossy(&git(dir, &["ls-files", "-z", "--", "."])?)
            .split('\0')
            .filter(|p| judged_path(p))
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let commits = read_history(dir, params.window)?;
        if git(dir, &["rev-parse", "--is-shallow-repository"])
            .is_ok_and(|o| String::from_utf8_lossy(&o).trim() == "true")
        {
            notes.push("MOD-006: this is a shallow clone, so the history is cut at its depth and what follows is a lower bound".into());
        }
        let (found, tally) = analyse(&commits, &head, params);
        notes.push(format!(
            "MOD-006 read {} commit(s) (window {}), set aside {} bulk commit(s) touching more than {} files and {} file(s) that over a fifth of the commits touch{}; judged {} source file(s)",
            tally.read,
            params.window,
            tally.bulk,
            params.max_files,
            tally.ubiquitous.len(),
            if tally.ubiquitous.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    tally.ubiquitous.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                )
            },
            tally.judged
        ));
        let project = dir.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Ok((judge(&found, &project, params), notes))
    }
}

/// Whether `path` has the extension of a source file. Only code files are partners of a change: a README, a workflow or a changelog
/// edited with a file is the change being documented, not a second reason for the file to change.
pub(crate) fn is_code(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| {
        matches!(
            ext,
            "rs" | "py"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "go"
                | "java"
                | "kt"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "hpp"
                | "cs"
        )
    })
}

/// Whether `path` is a registry: a file that is edited whenever something is added next to it, so that it says nothing about what changed
/// with it, as a judged file or as a partner.
pub(crate) fn is_registry(path: &str) -> bool {
    let name = path.rsplit_once('/').map_or(path, |(_, n)| n);
    matches!(
        name,
        "lib.rs" | "main.rs" | "mod.rs" | "__init__.py" | "index.ts" | "index.js"
    )
}

/// Whether `path` is a source file this rule judges (see the module docs).
pub(crate) fn judged_path(path: &str) -> bool {
    let Some((dir, name)) = path.rsplit_once('/').or(Some(("", path))) else {
        return false;
    };
    let Some((stem, _)) = name.rsplit_once('.') else {
        return false;
    };
    if !is_code(path) {
        return false;
    }
    if is_registry(path)
        || name == "build.rs"
        || matches!(stem, "index" | "tests")
        || stem.contains("_test")
        || stem.starts_with("test_")
        || stem.contains(".test")
        || stem.contains(".spec")
    {
        return false;
    }
    !dir.split('/').any(|c| {
        matches!(
            c,
            "tests" | "test" | "benches" | "examples" | "node_modules"
        )
    })
}

/// The newest `window` non-merge commits, newest first, with the files each touched (paths relative to `dir`).
pub(crate) fn read_history(dir: &Path, window: usize) -> anyhow::Result<Vec<Commit>> {
    let out = git(
        dir,
        &[
            "-c",
            "core.quotepath=false",
            "log",
            "--no-merges",
            "--no-renames",
            "--relative",
            "--name-only",
            "--pretty=format:%x01%H",
            &format!("--max-count={window}"),
            "--",
            ".",
        ],
    )?;
    Ok(parse_log(&String::from_utf8_lossy(&out)))
}

/// Parses `git log --name-only --pretty=format:%x01%H`.
pub(crate) fn parse_log(text: &str) -> Vec<Commit> {
    let mut commits: Vec<Commit> = Vec::new();
    for line in text.lines() {
        if let Some(id) = line.strip_prefix('\u{1}') {
            commits.push(Commit {
                id: id.trim().to_string(),
                files: BTreeSet::new(),
            });
        } else if !line.trim().is_empty() {
            if let Some(c) = commits.last_mut() {
                c.files.insert(line.trim().to_string());
            }
        }
    }
    commits
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        if self.0[x] != x {
            let r = self.find(self.0[x]);
            self.0[x] = r;
        }
        self.0[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[rb.max(ra)] = ra.min(rb);
        }
    }
}

/// The files of `head` whose counted groups number two or more, in path order, and what was read.
pub(crate) fn analyse(
    commits: &[Commit],
    head: &BTreeSet<String>,
    params: Params,
) -> (Vec<Divergent>, Tally) {
    let mut tally = Tally {
        read: commits.len(),
        ..Tally::default()
    };
    let kept: Vec<&Commit> = commits
        .iter()
        .filter(|c| {
            let bulk = c.files.len() > params.max_files;
            if bulk {
                tally.bulk += 1;
            }
            !bulk
        })
        .collect();
    let mut touched: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &kept {
        for f in &c.files {
            *touched.entry(f).or_default() += 1;
        }
    }
    // a file touched by over a fifth of the commits (and by at least 5) is one everything changes with
    let ubiquitous: BTreeSet<&str> = touched
        .iter()
        .filter(|(_, n)| **n >= 5 && **n * 5 > kept.len())
        .map(|(f, _)| *f)
        .collect();
    tally.ubiquitous = ubiquitous.iter().map(|s| s.to_string()).collect();
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, c) in kept.iter().enumerate() {
        for f in &c.files {
            if head.contains(f) {
                by_file.entry(f).or_default().push(i);
            }
        }
    }
    tally.judged = by_file.len();
    let mut out = Vec::new();
    for (file, idxs) in by_file {
        let mut dsu = Dsu((0..idxs.len()).collect());
        let mut first_with: BTreeMap<&str, usize> = BTreeMap::new();
        let mut alone = 0usize;
        let mut linked = vec![false; idxs.len()];
        for (k, &i) in idxs.iter().enumerate() {
            for f in &kept[i].files {
                if f == file || ubiquitous.contains(f.as_str()) || !is_code(f) || is_registry(f) {
                    continue;
                }
                linked[k] = true;
                match first_with.get(f.as_str()) {
                    Some(&j) => dsu.union(j, k),
                    None => {
                        first_with.insert(f, k);
                    }
                }
            }
        }
        let mut sets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, &is_linked) in linked.iter().enumerate() {
            if is_linked {
                let r = dsu.find(k);
                sets.entry(r).or_default().push(k);
            } else {
                alone += 1;
            }
        }
        let (mut counted, mut small) = (Vec::new(), 0usize);
        for ks in sets.into_values() {
            if ks.len() >= params.min_commits {
                let mut files = BTreeSet::new();
                for &k in &ks {
                    for f in &kept[idxs[k]].files {
                        if f != file
                            && !ubiquitous.contains(f.as_str())
                            && is_code(f)
                            && !is_registry(f)
                        {
                            files.insert(f.clone());
                        }
                    }
                }
                counted.push(Group {
                    commits: ks.iter().map(|&k| kept[idxs[k]].id.clone()).collect(),
                    files,
                });
            } else {
                small += ks.len();
            }
        }
        if counted.len() >= 2 {
            counted.sort_by(|a, b| {
                b.commits
                    .len()
                    .cmp(&a.commits.len())
                    .then(a.files.cmp(&b.files))
            });
            out.push(Divergent {
                file: file.to_string(),
                groups: counted,
                small,
                alone,
            });
        }
    }
    (out, tally)
}

fn judge(found: &[Divergent], project: &str, params: Params) -> Vec<Finding> {
    found
        .iter()
        .map(|d| {
            let shown: Vec<String> = d
                .groups
                .iter()
                .take(4)
                .enumerate()
                .map(|(i, g)| {
                    let files: Vec<&str> = g.files.iter().take(4).map(String::as_str).collect();
                    let ids: Vec<&str> = g
                        .commits
                        .iter()
                        .take(3)
                        .map(|c| c.get(..8).unwrap_or(c))
                        .collect();
                    format!(
                        "group {}: {} commit(s) ({}{}) with `{}`{}",
                        i + 1,
                        g.commits.len(),
                        ids.join(", "),
                        if g.commits.len() > 3 { ", .." } else { "" },
                        files.join("`, `"),
                        if g.files.len() > 4 { ", .." } else { "" }
                    )
                })
                .collect();
            Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: Severity::Info,
                confidence: Confidence::Low,
                project: project.into(),
                location: Some(Location {
                    file: d.file.clone(),
                    line: None,
                }),
                subject: Some(d.file.clone()),
                summary: format!(
                    "`{}` was changed in {} separate groups of commits, each with its own other files",
                    d.file,
                    d.groups.len()
                ),
                detail: format!(
                    "In the commits read, `{}` was edited together with sets of other files that never overlap: {}. Each group has at least {} commits; {} commit(s) were in groups too small to count and {} touched nothing else. A file edited for unrelated reasons answers to several drivers of change, which is a cue that it mixes responsibilities, not a verdict: a registry or a table is edited for many reasons by design, and a repository that squash-merges records one commit per pull request, so the groups may be coarse. Look at what each group changed in the file and whether they touch different parts of it; to accept it, allowlist the subject `{}` with a reason. Low confidence: commit granularity decides the groups.",
                    d.file,
                    shown.join("; "),
                    params.min_commits,
                    d.small,
                    d.alone,
                    d.file
                ),
                member: None,
                positive_control: None,
            }
        })
        .collect()
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    anyhow::ensure!(
        out.status.success(),
        "git {} failed in {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

#[cfg(test)]
mod tests;
