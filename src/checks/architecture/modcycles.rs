// SPDX-License-Identifier: MIT OR Apache-2.0
//! No cycles between modules (project scope, Rust): do the modules of a package refer to each other in a loop?
//!
//! Rule record: `MOD-002` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.2. The same graph and the same set-finding as `ARC-002`
//! (`cycles.rs`), at the grain of a module, not of a declared component.
//!
//! **What it reads.** The module graph of the package (`modgraph.rs`: the tracked `.rs` files under `src/` reached from `lib.rs`/`main.rs`
//! through `mod`, test items skipped). A path written in an item (or a `use`) in module `M` that resolves to an item inside module `T` is
//! an edge `M -> T`. Strongly connected sets of two or more modules (Tarjan) are reported.
//!
//! **Which edges count (the design note's question 2, answered as it proposed).** A child that names its parent's items (`use super::X`) is
//! idiomatic Rust and would make nearly every module a cycle with its parent, so:
//! - an edge between modules neither of which contains the other, and an edge from a module to a module inside it, count;
//! - an edge from a module to one that contains it counts **only when written from the crate root** (`crate::a::X`), not through `super`
//!   or `self`: a descendant that reaches back to its ancestor by an absolute path;
//! - the crate root takes no part: `crate::Error` or `crate::Result` defined in `lib.rs` is the crate's façade, not a module a child
//!   depends on, and a root that uses its children is how a crate is assembled;
//! - the `use` that builds a `pub use` re-export is the interface, not a dependency, and is not an edge.
//!
//! **Severity.** **Low** when a set spans two or more top-level modules; **Info** when every module of the set is inside one top-level
//! module. One finding per set with the number of modules, the edges (with a file and line each), and the edges whose removal dissolves it
//! (every single edge that does when one is enough, otherwise a depth-first set in name order, not necessarily the smallest).
//!
//! **Weak evidence by construction (medium confidence).** A lower bound: a path inside a macro call's tokens, an alias, a glob re-export
//! and an item named only by its type are not followed; `#[path]` modules are not read. A cycle found is real as far as its edges are.
//! The finding names edges and nothing else: no levels, no tree, no metric, no ranking (patent watch of `ARC-002`, row 62 of
//! `docs/PATENT-SWEEP.md`).
//!
//! **Known limitations / deliberate scope:** one package at a time, at `HEAD`; a file `syn` cannot parse is an error, never a smaller
//! graph; cycles between crates cannot occur in cargo (that is `ARC-002`'s ground, with components).

use super::cycles::{dissolving, sccs};
use super::modgraph::{self, Facts};
use crate::check::{Check, CheckContext, Network, Scope};
use crate::checks::rust_sources::read_sources;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_002;

/// Reports strongly connected sets of modules (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ModuleCyclesCheck;

impl ModuleCyclesCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for ModuleCyclesCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        crate::package::fan_out(ctx, |_, package| {
            let files = read_sources(&package.dir)?;
            let Some(facts) = modgraph::read(&files)? else {
                return Ok((
                Vec::new(),
                vec!["no src/lib.rs or src/main.rs among the tracked sources; no module graph, so no cycle is judged".into()],
            ));
            };
            let project = package.dir.file_name().map_or_else(
                || package.name.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
            let (edges, nodes) = module_edges(&facts);
            let sets = sccs(&nodes, &edges.keys().cloned().collect());
            let mut notes = facts.notes.clone();
            notes.push(format!(
                "MOD-002 read {} module(s) and {} edge(s) between them; {} cycle(s)",
                nodes.len(),
                edges.len(),
                sets.len()
            ));
            Ok((judge(&facts, &edges, &sets, &project), notes))
        })
    }
}

/// Where an edge was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub file: String,
    pub line: usize,
}

fn contains(outer: &str, inner: &str) -> bool {
    outer.is_empty() || inner == outer || inner.starts_with(&format!("{outer}::"))
}

/// The edges between modules (the crate root excluded) with the first place each was written, and the modules.
pub(crate) fn module_edges(f: &Facts) -> (BTreeMap<(String, String), Site>, BTreeSet<String>) {
    let reexport_paths: Vec<(&str, &[String])> = f
        .reexports
        .iter()
        .map(|e| (e.module.as_str(), e.segs.as_slice()))
        .collect();
    let nodes: BTreeSet<String> = f.mods.keys().filter(|m| !m.is_empty()).cloned().collect();
    let mut edges: BTreeMap<(String, String), Site> = BTreeMap::new();
    for r in &f.refs {
        if reexport_paths
            .iter()
            .any(|(m, s)| *m == r.module && *s == r.segs.as_slice())
        {
            continue;
        }
        let Some(abs) = modgraph::resolve_abs(&f.mods, &r.module, &r.segs) else {
            continue;
        };
        let to = abs[..modgraph::module_prefix(&f.mods, &abs)].join("::");
        let from = r.module.clone();
        if to.is_empty() || from.is_empty() || to == from {
            continue;
        }
        // a module reaching back to one that contains it counts only through an absolute path
        if contains(&to, &from) && r.segs.first().map(String::as_str) != Some("crate") {
            continue;
        }
        edges.entry((from, to)).or_insert(Site {
            file: r.file.clone(),
            line: r.line,
        });
    }
    (edges, nodes)
}

fn top(module: &str) -> &str {
    module.split("::").next().unwrap_or(module)
}

/// One finding per set.
pub(crate) fn judge(
    f: &Facts,
    edges: &BTreeMap<(String, String), Site>,
    sets: &[Vec<String>],
    project: &str,
) -> Vec<Finding> {
    let all: BTreeSet<(String, String)> = edges.keys().cloned().collect();
    sets.iter()
        .map(|scc| {
            let inside: Vec<(&(String, String), &Site)> = edges
                .iter()
                .filter(|((a, b), _)| scc.contains(a) && scc.contains(b))
                .collect();
            let (cut, single) = dissolving(scc, &all);
            let tops: BTreeSet<&str> = scc.iter().map(|m| top(m)).collect();
            let across = tops.len() >= 2;
            let name = |m: &str| format!("crate::{m}");
            let list = |es: &[(String, String)]| {
                es.iter()
                    .map(|(a, b)| format!("`{}` -> `{}`", name(a), name(b)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let shown = inside
                .iter()
                .take(10)
                .map(|((a, b), s)| format!("`{}` -> `{}` ({}:{})", name(a), name(b), s.file, s.line))
                .collect::<Vec<_>>()
                .join(", ");
            let first = f.mods.get(&scc[0]).cloned();
            let where_ = if across {
                " across top-level modules".to_string()
            } else {
                format!(
                    " inside the top-level module `crate::{}`",
                    tops.iter().next().copied().unwrap_or_default()
                )
            };
            Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: if across { Severity::Low } else { Severity::Info },
                confidence: Confidence::Medium,
                project: project.into(),
                location: first.map(|file| Location { file, line: None }),
                subject: Some(scc.join(",")),
                summary: format!(
                    "{} modules refer to each other in a cycle{where_}: `{}`",
                    scc.len(),
                    scc.iter().map(|m| name(m)).collect::<Vec<_>>().join("`, `")
                ),
                detail: format!(
                    "Each of these modules reaches every other through the paths written in them, so none can be understood, tested or moved apart. The {} edge(s) between them: {shown}{}. {} {}. A descendant reaching back to an ancestor counts only when written from the crate root; `super` and `self` paths, the crate root itself and the `use` of a `pub use` are not edges. A lower bound: a path in a macro call's tokens, an alias or a glob re-export is not followed. Break it by moving the shared piece into a module both sides depend on, or by inverting one edge; to accept it, allowlist the subject `{}` with a reason.",
                    inside.len(),
                    if inside.len() > 10 { ", ..." } else { "" },
                    if single {
                        "Removing any one of these edges dissolves it"
                    } else {
                        "No single edge dissolves it; removing all of these does (a depth-first set in name order, not necessarily the smallest)"
                    },
                    list(&cut),
                    scc.join(",")
                ),
                member: None,
                positive_control: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
