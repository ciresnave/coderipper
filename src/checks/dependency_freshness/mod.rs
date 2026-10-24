// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dependency-freshness check (project scope, NETWORK): is every locked package at the newest version the registry has?
//!
//! Rule record: `dependency-freshness` in `rules/sup.toml` (the rule catalog). Coverage row: "Newest dependency versions at all
//! times" in `docs/portfolio-ci-coverage.md`.
//!
//! CireSnave's standing rule (CLAUDE.md 9): *"I want all of my projects' dependencies on their most recent versions at all
//! times."* The lockfile, not the manifest, says what a build uses, so this reads `Cargo.lock`: `cargo update --dry-run --verbose`
//! lists, without changing anything, every package that has a newer compatible version (`Updating a v1.0.0 -> v1.0.5`) and
//! every package held at its version by a manifest requirement while a newer one exists (`Unchanged a v1.0.0 (available: v2.0.0)`).
//! `Adding` and `Removing` lines are not staleness (a platform-specific package the lock does not carry yet) and are not read.
//!
//! One finding per package, named by its package name in `Cargo.lock`: **Medium** for a dependency a workspace member declares,
//! **Low** for a transitive one (a parent crate's release has to move first, so the fix may not be ours to make). A package
//! that cannot move is waived, with the reason, by an `[[allow]]` entry (`check = "dependency-freshness"`, `file = "Cargo.lock"`,
//! `symbol = <package name>`).
//!
//! Network tier, scheduled rather than per PR: a PR would turn red the day any dependency published. It runs `cargo update
//! --dry-run`, which refreshes the registry index and runs no project code, and it never writes `Cargo.lock`.
//!
//! **A check that cannot see must not report clean.** A cargo failure, or output with no sign that cargo reached the end of the
//! dry run, is an error. The one honest non-finding is a project with no `Cargo.lock` (an `Info` finding saying so: SUP-001 is
//! the rule that asks for one). Rust (cargo) only; npm, pip and the rest are not read.

use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::DEPENDENCY_FRESHNESS;

/// `Finding.location.file` of every finding here: the lockfile an `[[allow]]` entry would point at.
pub const LOCKFILE: &str = "Cargo.lock";

/// What cargo says about a project's lockfile: the two reads the check needs, so a test can answer them without a network.
pub trait Cargo: Send + Sync {
    /// The text `cargo update --dry-run --verbose` printed in `dir`; an error when cargo failed.
    fn dry_run(&self, dir: &Path) -> anyhow::Result<String>;
    /// The package names the workspace's members declare as dependencies (not the members themselves).
    fn direct_dependencies(&self, dir: &Path) -> anyhow::Result<BTreeSet<String>>;
}

/// The real cargo.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealCargo;

impl Cargo for RealCargo {
    fn dry_run(&self, dir: &Path) -> anyhow::Result<String> {
        let output = Command::new("cargo")
            .args(["update", "--dry-run", "--verbose"])
            .current_dir(dir)
            .output()?;
        let text = String::from_utf8_lossy(&output.stderr).into_owned();
        anyhow::ensure!(
            output.status.success(),
            "`cargo update --dry-run` failed in {dir:?}: {}",
            text.trim()
        );
        Ok(text)
    }

    fn direct_dependencies(&self, dir: &Path) -> anyhow::Result<BTreeSet<String>> {
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(dir)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "cargo metadata failed in {dir:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let packages = json["packages"].as_array().into_iter().flatten();
        let members: BTreeSet<&str> = packages
            .clone()
            .filter_map(|p| p["name"].as_str())
            .collect();
        Ok(packages
            .flat_map(|p| p["dependencies"].as_array().into_iter().flatten())
            .filter_map(|d| d["name"].as_str())
            .filter(|name| !members.contains(name))
            .map(str::to_string)
            .collect())
    }
}

/// Reports a locked package that is not at the newest version (see the module docs).
pub struct DependencyFreshnessCheck {
    cargo: Box<dyn Cargo>,
}

impl DependencyFreshnessCheck {
    /// The real check, asking cargo.
    pub fn new() -> Self {
        Self::with_cargo(Box::new(RealCargo))
    }

    /// The check over any [`Cargo`]: what tests use, with canned output.
    pub fn with_cargo(cargo: Box<dyn Cargo>) -> Self {
        Self { cargo }
    }
}

impl Default for DependencyFreshnessCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl Check for DependencyFreshnessCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    /// One lockfile per workspace: a finding per package of it, not one per member.
    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::NetworkRequired
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let project = ctx.project_root.file_name().map_or_else(
            || "project".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        if !ctx.project_root.join(LOCKFILE).is_file() {
            return Ok(vec![no_lockfile(&project)]);
        }
        let text = self.cargo.dry_run(&ctx.project_root)?;
        anyhow::ensure!(
            reached_the_end(&text),
            "`cargo update --dry-run` printed no sign that it finished (no registry refresh and no dry-run notice), so an empty \
             list would not mean fresh. Output: {}",
            text.trim()
        );
        let direct = self.cargo.direct_dependencies(&ctx.project_root)?;
        Ok(behind(&text)
            .into_iter()
            .map(|b| finding(&project, &b, direct.contains(&b.name)))
            .collect())
    }
}

/// Did cargo get as far as the dry run's closing notice, or at least a registry refresh?
fn reached_the_end(text: &str) -> bool {
    text.contains("not updating lockfile due to dry run")
        || text.contains("Updating crates.io index")
}

/// One package whose locked version is not the newest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Behind {
    name: String,
    locked: String,
    newest: String,
    /// A newer version the manifest's requirement already allows (`cargo update -p NAME` takes it), as against one it excludes.
    compatible: bool,
}

/// The packages `cargo update --dry-run --verbose` reports as behind, in the order cargo printed them.
fn behind(text: &str) -> Vec<Behind> {
    let mut out = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let available = line.find('(').and_then(|at| available_in(&line[at..]));
        let found = match words.as_slice() {
            ["Updating", name, locked, "->", target, ..]
                if locked.starts_with('v') && target.starts_with('v') =>
            {
                Some(Behind {
                    name: (*name).into(),
                    locked: locked.trim_start_matches('v').into(),
                    newest: available
                        .clone()
                        .unwrap_or_else(|| target.trim_start_matches('v').into()),
                    compatible: true,
                })
            }
            ["Unchanged", name, locked, ..] if locked.starts_with('v') => {
                available.map(|newest| Behind {
                    name: (*name).into(),
                    locked: locked.trim_start_matches('v').into(),
                    newest,
                    compatible: false,
                })
            }
            _ => None,
        };
        out.extend(found);
    }
    out
}

/// The version after `available:` or `latest:` in a parenthesis such as `(available: v2.0.0, requires Rust 1.85)`.
fn available_in(paren: &str) -> Option<String> {
    let rest = ["available: ", "latest: "]
        .iter()
        .find_map(|key| paren.find(key).map(|at| &paren[at + key.len()..]))?;
    let end = rest.find([',', ')']).unwrap_or(rest.len());
    let version = rest[..end].trim().trim_start_matches('v');
    (!version.is_empty()).then(|| version.to_string())
}

fn finding(project: &str, b: &Behind, direct: bool) -> Finding {
    let how = if b.compatible {
        format!("`cargo update -p {}` takes it", b.name)
    } else {
        "the manifest's version requirement excludes it, so the requirement has to be raised".into()
    };
    let (severity, who) = if direct {
        (Severity::Medium, "a dependency a workspace member declares")
    } else {
        (
            Severity::Low,
            "a transitive dependency, so a parent crate may have to release first",
        )
    };
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity,
        confidence: Confidence::High,
        project: project.into(),
        location: Some(Location {
            file: LOCKFILE.into(),
            line: None,
        }),
        subject: Some(b.name.clone()),
        summary: format!("{} is locked at {} and {} is available", b.name, b.locked, b.newest),
        detail: format!(
            "{} is {who}; {how}. CireSnave's rule (CLAUDE.md 9) is the newest version at all times; a package that cannot move is \
             waived with its reason by an `[[allow]]` entry (file = \"{LOCKFILE}\", symbol = \"{}\").",
            b.name, b.name
        ),
        positive_control: None,
        member: None,
    }
}

fn no_lockfile(project: &str) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Absence,
        severity: Severity::Info,
        confidence: Confidence::High,
        project: project.into(),
        location: Some(Location {
            file: LOCKFILE.into(),
            line: None,
        }),
        subject: None,
        summary: format!("no {LOCKFILE}, so there are no locked versions to compare with the registry"),
        detail: "Nothing was judged: freshness is read from the lockfile. SUP-001 is the rule that asks a project to commit one.".into(),
        positive_control: Some(format!("{LOCKFILE} was looked for at the project root and is not a file there")),
        member: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
    Updating crates.io index
     Locking 4 packages to highest compatible versions
      Adding bumpalo v3.20.3
    Updating cc v1.6.0 -> v1.10.0
    Updating toml v1.1.6+spec-1.1.0 -> v1.1.8+spec-1.1.0
    Updating zip v9.0.0 -> v9.1.0 (available: v10.0.0)
 Unchanged ureq v2.0.0 (available: v3.5.0, requires Rust 1.85)
 Unchanged old v0.1.0 (latest: v0.2.0)
 Unchanged same v1.0.0
    Updating git repository `https://example.com/x`
note: to see how you depend on a package, run `cargo tree --invert <dep>@<ver>`
warning: not updating lockfile due to dry run
";

    fn names(b: &[Behind]) -> Vec<(&str, &str, &str, bool)> {
        b.iter()
            .map(|b| {
                (
                    b.name.as_str(),
                    b.locked.as_str(),
                    b.newest.as_str(),
                    b.compatible,
                )
            })
            .collect()
    }

    #[test]
    fn updating_and_unchanged_behind_lines_are_read_and_the_rest_is_not() {
        let got = behind(SAMPLE);
        assert_eq!(
            names(&got),
            vec![
                ("cc", "1.6.0", "1.10.0", true),
                ("toml", "1.1.6+spec-1.1.0", "1.1.8+spec-1.1.0", true),
                // the compatible bump is not the newest: the newer major is what the finding names
                ("zip", "9.0.0", "10.0.0", true),
                ("ureq", "2.0.0", "3.5.0", false),
                ("old", "0.1.0", "0.2.0", false),
            ]
        );
    }

    #[test]
    fn a_dry_run_that_shows_nothing_behind_is_empty() {
        let quiet = "    Updating crates.io index\nwarning: not updating lockfile due to dry run\n";
        assert!(behind(quiet).is_empty());
        assert!(reached_the_end(quiet));
        assert!(!reached_the_end("error: something\n"));
    }

    #[test]
    fn a_direct_dependency_is_medium_and_a_transitive_one_low() {
        let b = Behind {
            name: "serde".into(),
            locked: "1.0.1".into(),
            newest: "1.0.5".into(),
            compatible: true,
        };
        assert_eq!(finding("p", &b, true).severity, Severity::Medium);
        let low = finding("p", &b, false);
        assert_eq!(low.severity, Severity::Low);
        assert_eq!(low.subject.as_deref(), Some("serde"));
        assert!(low.detail.contains("cargo update -p serde"));
    }
}
