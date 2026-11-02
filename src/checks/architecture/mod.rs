// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dependencies follow the declared component graph (repository scope, Rust): does the workspace's crate graph agree with the
//! `[architecture]` the project wrote down?
//!
//! Rule record: `ARC-001` in `rules/arc.toml`. The declaration format is
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; the rule's background is `docs/rules/arc.md`.
//!
//! **What it judges.** Every edge between two workspace members (a normal or build dependency on a member by `path`, from
//! `cargo metadata --no-deps`, which runs no build script) is compared with the declaration: the edge is fine when both ends are in the
//! same component, when the layer of the depending component is the same as or above the layer of the one it depends on, or when an
//! `[[architecture.allow]]` names it. Anything else is a finding, unless an unexpired `[[architecture.waiver]]` names it, in which case it is
//! printed as a note on every run (a waiver is visible, never silent).
//!
//! **The declaration is judged too.** A component that names no member, a member that is in no component or layer, a waiver that names an
//! edge that is not a violation, and a waiver whose end date has passed are findings (Info, Info, Info, Low): a declaration that has gone stale
//! passes forever.
//!
//! **No declaration is not clean.** A repository without an `[architecture]` table (or one with no layers) gets "not run" in the coverage and
//! a note that says what to add ([`Check::not_configured`]).
//!
//! **Known limitations / deliberate scope:**
//! - Crate granularity only in this version. A component whose `paths` point inside a crate (module groups) is noted and not judged;
//!   the module graph of a single crate is the next step.
//! - `dev-dependencies` are not judged (a test may reach up the layers); the note says how many edges that left out.
//! - A dependency on a member by version alone (no `path`) resolves to the registry copy, not the member, and is not an edge.
//! - Edges to or from a member the declaration does not place are not judged; the member is reported as unplaced.

use crate::architecture::{self, ArchitectureConfig, Date};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_001;

/// Reports dependencies between components that the declared architecture does not allow (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ComponentGraphCheck;

impl ComponentGraphCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for ComponentGraphCheck {
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

    fn not_configured(&self, ctx: &CheckContext) -> Option<String> {
        match architecture::load(&ctx.project_root) {
            Ok(Some((a, _))) if !a.layer.is_empty() => None,
            Ok(Some(_)) => Some(
                "the [architecture] table declares no layers, so there is no direction to judge dependencies against; add [[architecture.layer]] blocks (docs/rules/arc.md)"
                    .into(),
            ),
            Ok(None) => Some(
                "this repository declares no [architecture] table in .coderipper.toml, so ARC-001 has nothing to compare the dependency graph with; a missing declaration is a gap, not a pass (docs/rules/arc.md)"
                    .into(),
            ),
            // an unreadable declaration is an error of the run, raised by `run`
            Err(_) => None,
        }
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let Some((arch, file)) = architecture::load(&ctx.project_root)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        let graph = read_graph(&ctx.project_root)?;
        let names: BTreeSet<String> = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Ok(judge(&arch, &graph, Date::today(), &project))
    }
}

/// A workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    pub name: String,
    /// The crate's directory relative to the repository root, forward slashes, no trailing slash (`""` for the root crate).
    pub dir: String,
    /// The manifest relative to the repository root.
    pub manifest: String,
}

/// An edge of the crate graph: `from` depends on `to` (a normal or build dependency by path).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Default)]
pub(crate) struct Graph {
    pub members: Vec<Member>,
    pub edges: BTreeSet<Edge>,
    /// Dev-dependency edges between members, not judged.
    pub dev_edges: usize,
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

/// The workspace's members and the dependencies among them, from `cargo metadata --no-deps` run at `root`.
fn read_graph(root: &Path) -> anyhow::Result<Graph> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "cargo metadata failed in {}: {}",
        root.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    graph_from_metadata(&serde_json::from_slice(&out.stdout)?, root)
}

pub(crate) fn graph_from_metadata(json: &serde_json::Value, root: &Path) -> anyhow::Result<Graph> {
    // cargo reports canonical paths, and the root as given can be a symlink away from them (macOS: /var is /private/var): try both
    // spellings of the root, and drop the `\\?\` prefix Windows puts on a canonical path
    let mut bases = vec![norm(&root.display().to_string())];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        let canonical = norm(&canonical.display().to_string());
        bases.push(canonical.trim_start_matches("//?/").to_string());
    }
    let relative = |abs: &str| -> String {
        let abs = norm(abs);
        for base in &bases {
            if let Some(rest) = abs.strip_prefix(base.as_str()) {
                if rest.is_empty() || rest.starts_with('/') {
                    return rest.trim_start_matches('/').to_string();
                }
            }
        }
        abs
    };
    let packages = json["packages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("cargo metadata has no packages array"))?;
    let mut graph = Graph::default();
    let mut by_dir: BTreeMap<String, String> = BTreeMap::new();
    for p in packages {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        let manifest_abs = norm(p["manifest_path"].as_str().unwrap_or_default());
        let dir_abs = manifest_abs
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_string());
        by_dir.insert(dir_abs.clone(), name.clone());
        graph.members.push(Member {
            name,
            dir: relative(&dir_abs),
            manifest: relative(&manifest_abs),
        });
    }
    for p in packages {
        let from = p["name"].as_str().unwrap_or_default();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let Some(path) = d["path"].as_str() else {
                continue;
            };
            let Some(to) = by_dir.get(&norm(path)) else {
                continue;
            };
            if to == from {
                continue;
            }
            if d["kind"].as_str() == Some("dev") {
                graph.dev_edges += 1;
                continue;
            }
            graph.edges.insert(Edge {
                from: from.to_string(),
                to: to.clone(),
            });
        }
    }
    graph.members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(graph)
}

/// Whether a component path points inside a crate (module group), rather than at a crate directory.
fn is_module_path(path: &str, members: &[Member]) -> bool {
    path.contains('*')
        || path.ends_with(".rs")
        || members
            .iter()
            .any(|m| !m.dir.is_empty() && path.starts_with(&format!("{}/", m.dir)))
        || members
            .iter()
            .any(|m| m.dir.is_empty() && path.starts_with("src"))
}

struct Resolved {
    /// member name -> component name (`None`: the declaration does not place the member).
    component: BTreeMap<String, Option<String>>,
    /// component name -> layer index, bottom = 0.
    layer: BTreeMap<String, usize>,
    layer_name: Vec<String>,
}

impl Resolved {
    fn new(arch: &ArchitectureConfig, members: &[Member]) -> Self {
        let mut mentioned: BTreeSet<&str> = BTreeSet::new();
        for l in &arch.layer {
            mentioned.extend(l.components.iter().map(String::as_str));
        }
        for a in &arch.allow {
            mentioned.insert(a.from.as_str());
            mentioned.insert(a.to.as_str());
        }
        for w in &arch.waiver {
            mentioned.extend(w.edge.iter().map(String::as_str));
        }
        let mut component = BTreeMap::new();
        for m in members {
            let by_path = arch
                .component
                .iter()
                .find(|c| c.paths.iter().any(|p| norm(p) == m.dir))
                .map(|c| c.name.clone());
            let by_name = arch
                .component
                .iter()
                .any(|c| c.name == m.name)
                .then(|| m.name.clone());
            // a member that holds a component's module group is placed (its inside is not judged in this version)
            let holds_module_group = arch.component.iter().any(|c| {
                c.paths.iter().any(|p| {
                    let p = norm(p);
                    is_module_path(&p, members)
                        && if m.dir.is_empty() {
                            p.starts_with("src")
                        } else {
                            p.starts_with(&format!("{}/", m.dir))
                        }
                })
            });
            let placed = by_path.or(by_name).or_else(|| {
                (mentioned.contains(m.name.as_str()) || holds_module_group).then(|| m.name.clone())
            });
            component.insert(m.name.clone(), placed);
        }
        let mut layer = BTreeMap::new();
        for (i, l) in arch.layer.iter().enumerate() {
            for c in &l.components {
                layer.insert(c.clone(), i);
            }
        }
        Self {
            component,
            layer,
            layer_name: arch.layer.iter().map(|l| l.name.clone()).collect(),
        }
    }

    /// Whether a name written in an `allow` edge or a waiver is this member (by its component, or by its own name).
    fn is(&self, written: &str, member: &str) -> bool {
        written == member
            || self
                .component
                .get(member)
                .and_then(Option::as_deref)
                .is_some_and(|c| c == written)
    }
}

/// The findings and notes for `graph` judged against `arch` on `today`.
pub(crate) fn judge(
    arch: &ArchitectureConfig,
    graph: &Graph,
    today: Date,
    project: &str,
) -> (Vec<Finding>, Vec<String>) {
    let r = Resolved::new(arch, &graph.members);
    let manifest_of = |name: &str| {
        graph
            .members
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.manifest.clone())
    };
    let config_file = crate::config::FILE_NAME.to_string();
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let mut used_waivers: BTreeSet<usize> = BTreeSet::new();
    let (mut judged, mut unjudged) = (0usize, 0usize);
    let mut waived_count = 0usize;

    for e in &graph.edges {
        let (cf, ct) = (
            r.component.get(e.from.as_str()).and_then(Clone::clone),
            r.component.get(e.to.as_str()).and_then(Clone::clone),
        );
        let (Some(cf), Some(ct)) = (cf, ct) else {
            unjudged += 1;
            continue;
        };
        if cf == ct {
            judged += 1;
            continue;
        }
        judged += 1;
        let allowed_by_edge = arch
            .allow
            .iter()
            .any(|a| r.is(&a.from, &e.from) && r.is(&a.to, &e.to));
        let (lf, lt) = (r.layer.get(&cf).copied(), r.layer.get(&ct).copied());
        let by_layer = matches!((lf, lt), (Some(f), Some(t)) if f >= t);
        if allowed_by_edge || by_layer {
            continue;
        }
        let why = match (lf, lt) {
            (Some(f), Some(t)) => format!(
                "`{}` is in layer `{}` and `{}` in layer `{}`, which sits above it",
                e.from, r.layer_name[f], e.to, r.layer_name[t]
            ),
            (None, _) => format!(
                "`{}` is in a component that is in no layer, so it may use only `allow` edges",
                e.from
            ),
            (_, None) => format!(
                "`{}` is in a component that is in no layer, so only an `allow` edge lets another component depend on it",
                e.to
            ),
        };
        let waiver = arch
            .waiver
            .iter()
            .enumerate()
            .find(|(_, w)| r.is(&w.edge[0], &e.from) && r.is(&w.edge[1], &e.to));
        let mut expired_note = String::new();
        if let Some((i, w)) = waiver {
            used_waivers.insert(i);
            let until = Date::parse(&w.until).expect("validated");
            if until >= today {
                waived_count += 1;
                notes.push(format!(
                    "waived: `{}` -> `{}` ({why}); reason: {}; owner: {}; until {}",
                    e.from, e.to, w.reason, w.owner, w.until
                ));
                continue;
            }
            expired_note = format!(
                " Its waiver ended on {} (owner {}; reason given: {}), so it no longer covers this edge.",
                w.until, w.owner, w.reason
            );
        }
        findings.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Low,
            confidence: Confidence::High,
            project: project.into(),
            location: manifest_of(&e.from).map(|file| Location { file, line: None }),
            subject: Some(format!("{}->{}", e.from, e.to)),
            summary: format!(
                "`{}` depends on `{}` against the declared architecture",
                e.from, e.to
            ),
            detail: format!(
                "The crate graph has the edge `{}` -> `{}`; {why}. Either remove the dependency (usually by moving what is shared down, or by inverting the dependency), or, if the design has really changed, change the declaration in `.coderipper.toml` in the same change; a deliberate exception is an `[[architecture.allow]]` with a reason (permanent) or an `[[architecture.waiver]]` with an owner and an end date (temporary).{expired_note}",
                e.from, e.to
            ),
            member: None,
            positive_control: None,
        });
    }

    // the declaration itself
    for (name, comp) in &r.component {
        if comp.is_none() {
            findings.push(declaration_finding(
                project,
                &config_file,
                &format!("unplaced:{name}"),
                format!("the workspace member `{name}` is not placed in any component or layer of the declaration"),
                "A declaration that leaves a member out cannot judge the edges that touch it, and the member is skipped. Add it to a layer (or to a `[[architecture.component]]` with its `paths`), or, if the crate is meant to sit outside the architecture, say so with an `allow` edge.".into(),
            ));
        }
    }
    for c in &arch.component {
        let modular = c
            .paths
            .iter()
            .any(|p| is_module_path(&norm(p), &graph.members));
        if modular {
            notes.push(format!(
                "component `{}` has module paths inside a crate; module groups are not judged in this version, only crate-level edges",
                c.name
            ));
            continue;
        }
        let resolves = graph.members.iter().any(|m| {
            r.component.get(m.name.as_str()).and_then(Option::as_deref) == Some(c.name.as_str())
        });
        if !resolves {
            let mut stale = declaration_finding(
                project,
                &config_file,
                &format!("stale:{}", c.name),
                format!("the declared component `{}` matches no workspace member", c.name),
                format!(
                    "Its `paths` ({}) and its name match no crate of this workspace: the crate was renamed, moved or dissolved, and the declaration was not updated. Remove the component or correct its `paths`.",
                    if c.paths.is_empty() { "none given".to_string() } else { c.paths.join(", ") }
                ),
            );
            stale.claim = Claim::Absence;
            stale.positive_control = Some(format!(
                "the same cargo metadata read finds {} workspace member(s): {}",
                graph.members.len(),
                graph
                    .members
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            findings.push(stale);
        }
    }
    for (i, w) in arch.waiver.iter().enumerate() {
        if !used_waivers.contains(&i) {
            findings.push(declaration_finding(
                project,
                &config_file,
                &format!("waiver:{}->{}", w.edge[0], w.edge[1]),
                format!(
                    "the waiver for `{}` -> `{}` covers an edge that is not a violation",
                    w.edge[0], w.edge[1]
                ),
                format!(
                    "The crate graph has no such edge, or the layers or an `allow` edge already permit it (reason given: {}; owner {}; until {}). Remove the waiver.",
                    w.reason, w.owner, w.until
                ),
            ));
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            (
                f.location.as_ref().map(|l| l.file.clone()),
                f.subject.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    notes.push(format!(
        "ARC-001 judged {judged} dependency edge(s) between workspace members ({waived_count} waived); {unjudged} edge(s) touch a member the declaration does not place; {} dev-dependency edge(s) are not judged",
        graph.dev_edges
    ));
    (findings, notes)
}

fn declaration_finding(
    project: &str,
    file: &str,
    subject: &str,
    summary: String,
    detail: String,
) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity: Severity::Info,
        confidence: Confidence::High,
        project: project.into(),
        location: Some(Location {
            file: file.into(),
            line: None,
        }),
        subject: Some(subject.into()),
        summary,
        detail,
        member: None,
        positive_control: None,
    }
}

#[cfg(test)]
mod tests;
