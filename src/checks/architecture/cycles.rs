// SPDX-License-Identifier: MIT OR Apache-2.0
//! No cycles between components (repository scope, Rust): does a chain of dependencies between the components of the declaration lead
//! back to where it began?
//!
//! Rule record: `ARC-002` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; background: `docs/rules/arc.md`.
//!
//! **What it judges.** The component graph `ARC-001` reads: an edge between two workspace crates (normal and build dependencies by
//! `path`, `cargo metadata --no-deps`) and an edge between two module groups of a crate (the module graph, `modgraph.rs`), each mapped to
//! the component that holds its end. Two components in one **strongly connected set** (each reaches the other) are a cycle: they cannot
//! be built, tested or replaced apart. One **Medium** finding per set, with the number of components it spans, the edges between them and
//! the edges whose removal dissolves it: every single edge that does when one is enough, otherwise a set found by a depth-first walk in
//! name order, which dissolves the cycle but is not necessarily the smallest.
//!
//! **Cargo already refuses a cycle between crates**, so a cycle here is between components that are more than one crate, or between
//! module groups of a crate. An `allow` edge or a waiver of `ARC-001` does not excuse a cycle (it says an edge is permitted, not that
//! the loop is fine); allowlist the finding with the subject `a,b,c` (the component names, sorted) and a reason.
//!
//! **Weak evidence by construction.** A module-group edge is a reference the module graph saw, a lower bound: a cycle found is real as
//! far as its edges are, a cycle through a macro call's tokens or an alias is not seen. Confidence is High for crate edges only and
//! Medium when a module group is in the set.
//!
//! **No declaration is not clean:** a repository without an `[architecture]` table gets "not run" with what to add.
//!
//! **Known limitations / deliberate scope:**
//! - Cycles *inside* one component (the files of one module group referring to each other) are not reported: in Rust a parent module and
//!   its child routinely name each other, so the Info finding the design sketched would be noise on almost every module.
//! - Dev-dependencies are not edges (as `ARC-001`); a component that matches no crate or file has no node.

use super::{modgraph, read_graph, Graph, Resolved};
use crate::architecture::{self, ArchitectureConfig};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_002;

/// Reports strongly connected sets of components (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ComponentCyclesCheck;

impl ComponentCyclesCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    match a {
        None => Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-002 has no components to look for cycles between; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into()),
        Some(a) if a.component.is_empty() && a.layer.is_empty() => Some("the [architecture] table declares no components or layers, so there is no component graph to look for cycles in (docs/rules/arc.md)".into()),
        Some(_) => None,
    }
}

impl Check for ComponentCyclesCheck {
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
            Ok(Some((a, _))) => missing_declaration(Some(&a)),
            Ok(None) => missing_declaration(None),
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
        let mut graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        modgraph::add_module_groups(&arch, &mut graph, &ctx.project_root)?;
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Ok(judge(&arch, &graph, &project))
    }
}

/// The components that have a member, and the edges between them: `(from, to)`, each mapped from the member edge that gave it.
pub(crate) fn component_edges(
    arch: &ArchitectureConfig,
    graph: &Graph,
) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let r = Resolved::new(arch, &graph.members);
    let place = |m: &str| r.component.get(m).and_then(Clone::clone);
    let mut nodes = BTreeSet::new();
    for m in &graph.members {
        if let Some(c) = place(&m.name) {
            nodes.insert(c);
        }
    }
    let mut edges = BTreeSet::new();
    for e in &graph.edges {
        if let (Some(a), Some(b)) = (place(&e.from), place(&e.to)) {
            if a != b {
                edges.insert((a, b));
            }
        }
    }
    (nodes, edges)
}

/// The strongly connected sets of two or more nodes (Tarjan), each sorted, in name order.
pub(crate) fn sccs(
    nodes: &BTreeSet<String>,
    edges: &BTreeSet<(String, String)>,
) -> Vec<Vec<String>> {
    struct T<'a> {
        adj: BTreeMap<&'a str, Vec<&'a str>>,
        index: BTreeMap<&'a str, usize>,
        low: BTreeMap<&'a str, usize>,
        on: BTreeSet<&'a str>,
        stack: Vec<&'a str>,
        next: usize,
        out: Vec<Vec<String>>,
    }
    fn visit<'a>(t: &mut T<'a>, v: &'a str) {
        t.index.insert(v, t.next);
        t.low.insert(v, t.next);
        t.next += 1;
        t.stack.push(v);
        t.on.insert(v);
        for w in t.adj.get(v).cloned().unwrap_or_default() {
            if !t.index.contains_key(w) {
                visit(t, w);
                let l = t.low[v].min(t.low[w]);
                t.low.insert(v, l);
            } else if t.on.contains(w) {
                let l = t.low[v].min(t.index[w]);
                t.low.insert(v, l);
            }
        }
        if t.low[v] == t.index[v] {
            let mut comp = Vec::new();
            while let Some(w) = t.stack.pop() {
                t.on.remove(w);
                comp.push(w.to_string());
                if w == v {
                    break;
                }
            }
            if comp.len() >= 2 {
                comp.sort();
                t.out.push(comp);
            }
        }
    }
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (a, b) in edges {
        adj.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut t = T {
        adj,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        on: BTreeSet::new(),
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for n in nodes {
        if !t.index.contains_key(n.as_str()) {
            visit(&mut t, n.as_str());
        }
    }
    t.out.sort();
    t.out
}

fn acyclic(nodes: &[String], edges: &BTreeSet<(String, String)>) -> bool {
    let set: BTreeSet<String> = nodes.iter().cloned().collect();
    sccs(&set, edges).is_empty()
}

/// The edges of the set `scc` whose removal dissolves it, and whether they are each alone enough.
pub(crate) fn dissolving(
    scc: &[String],
    edges: &BTreeSet<(String, String)>,
) -> (Vec<(String, String)>, bool) {
    let inside: BTreeSet<(String, String)> = edges
        .iter()
        .filter(|(a, b)| scc.contains(a) && scc.contains(b))
        .cloned()
        .collect();
    let single: Vec<(String, String)> = inside
        .iter()
        .filter(|e| {
            let mut rest = inside.clone();
            rest.remove(*e);
            acyclic(scc, &rest)
        })
        .cloned()
        .collect();
    if !single.is_empty() {
        return (single, true);
    }
    // no one edge is enough: the back edges of a depth-first walk in name order
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (a, b) in &inside {
        adj.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    let mut back = Vec::new();
    fn dfs<'a>(
        v: &'a str,
        adj: &BTreeMap<&'a str, Vec<&'a str>>,
        state: &mut BTreeMap<&'a str, u8>,
        back: &mut Vec<(String, String)>,
    ) {
        state.insert(v, 1);
        for &w in adj.get(v).into_iter().flatten() {
            match state.get(w) {
                None => dfs(w, adj, state, back),
                Some(1) => back.push((v.to_string(), w.to_string())),
                _ => {}
            }
        }
        state.insert(v, 2);
    }
    for n in scc {
        if !state.contains_key(n.as_str()) {
            dfs(n.as_str(), &adj, &mut state, &mut back);
        }
    }
    back.sort();
    (back, false)
}

fn edge_list(edges: &[(String, String)]) -> String {
    edges
        .iter()
        .map(|(a, b)| format!("`{a}` -> `{b}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The findings and notes for the component graph of `graph`.
pub(crate) fn judge(
    arch: &ArchitectureConfig,
    graph: &Graph,
    project: &str,
) -> (Vec<Finding>, Vec<String>) {
    let r = Resolved::new(arch, &graph.members);
    let (nodes, edges) = component_edges(arch, graph);
    let sets = sccs(&nodes, &edges);
    let mut findings = Vec::new();
    for scc in &sets {
        let inside: Vec<(String, String)> = edges
            .iter()
            .filter(|(a, b)| scc.contains(a) && scc.contains(b))
            .cloned()
            .collect();
        let (cut, single) = dissolving(scc, &edges);
        let modular = scc.iter().any(|c| graph.module_groups.contains_key(c));
        let manifest = scc.iter().find_map(|c| {
            graph
                .members
                .iter()
                .find(|m| r.component.get(&m.name).and_then(Option::as_deref) == Some(c.as_str()))
                .map(|m| m.manifest.clone())
        });
        findings.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Medium,
            confidence: if modular { Confidence::Medium } else { Confidence::High },
            project: project.into(),
            location: manifest.map(|file| Location { file, line: None }),
            subject: Some(scc.join(",")),
            summary: format!(
                "{} components depend on each other in a cycle: `{}`",
                scc.len(),
                scc.join("`, `")
            ),
            detail: format!(
                "Each of these components reaches every other through the dependencies between them, so they cannot be built, tested or replaced apart. The {} edge(s) between them: {}. {} {}. The edges come from {}; a cycle through a macro call's tokens or an alias is not seen. Break it by inverting one dependency (the owner defines a trait, the other implements it) or merge the components; to accept it, allowlist the subject `{}` with a reason.",
                inside.len(),
                edge_list(&inside),
                if single {
                    "Removing any one of these edges dissolves it"
                } else {
                    "No single edge dissolves it; removing all of these does (a depth-first set in name order, not necessarily the smallest)"
                },
                edge_list(&cut),
                if modular {
                    "the crate graph and the module graph of the crate that holds a module group (references between files, a lower bound)"
                } else {
                    "the crate graph (`cargo metadata --no-deps`)"
                },
                scc.join(",")
            ),
            member: None,
            positive_control: None,
        });
    }
    let notes = vec![format!(
        "ARC-002 read {} component(s) and {} edge(s) between them; {} cycle(s)",
        nodes.len(),
        edges.len(),
        sets.len()
    )];
    (findings, notes)
}

#[cfg(test)]
mod tests;
