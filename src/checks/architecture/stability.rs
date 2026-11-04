// SPDX-License-Identifier: MIT OR Apache-2.0
//! Depend toward stability (repository scope, Rust): does a component depend on one that is less stable than itself?
//!
//! Rule record: `ARC-005` in `rules/arc.toml`. Background: `docs/rules/arc.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.3.
//!
//! **What it measures.** The component graph `ARC-002` reads (workspace crates by `path` dependency, module groups of a crate from the
//! module graph, each mapped to the component that holds its end). For each component the instability `I = Ce / (Ca + Ce)`, where `Ce` is
//! the number of other components it depends on and `Ca` the number of other components that depend on it. An edge `A -> B` is reported
//! when `I(B) - I(A)` reaches the margin (default 0.3, `[arc005] min_margin` in the repository's own `.coderipper.toml`), ranked by the
//! margin. A component with no edges has no instability and no edge to judge.
//!
//! **Not reported.** An edge to a component declared `volatility = "volatile"` (depending on it is the project's decision, and `ARC-003` /
//! `ARC-004` judge it); an edge inside one strongly connected set (those are `ARC-002`'s); an edge to the standard library or to a
//! crate outside the workspace (the graph has only workspace members and module groups).
//!
//! **No declaration still runs:** without an `[architecture]` table every workspace member is its own component, so a workspace of
//! several crates is read at crate level. A repository of one crate and no declared module groups has no edge; the note says so (a gap is
//! not a pass).
//!
//! **Weak evidence by construction (V1, Info, low confidence).** Instability counts neighbours, not their size or how often they
//! change; a stable-looking component can be churned daily and a volatile-looking one can be frozen. Martin's abstractness is not used
//! (a weak signal in Rust). Measured before the rule was written, the crate-level reading almost never fires
//! (`docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 2).
//!
//! **Known limitations / deliberate scope:**
//! - A module-group edge is a reference the module graph saw, a lower bound (macro tokens, aliases and glob re-exports are not followed).
//! - Dev-dependencies are not edges (as `ARC-001`).

use super::cycles::{component_edges, sccs};
use super::{modgraph, read_graph, Graph};
use crate::architecture::{self, ArchitectureConfig, Volatility};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_005;

/// The margin used when the repository sets none.
pub(crate) const DEFAULT_MIN_MARGIN: f64 = 0.3;

/// Reports edges toward a less stable component (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct StabilityCheck;

impl StabilityCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for StabilityCheck {
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

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let margin = match std::fs::read_to_string(ctx.project_root.join(crate::config::FILE_NAME))
        {
            Ok(text) => crate::config::arc005_min_margin(&text)?.unwrap_or(DEFAULT_MIN_MARGIN),
            Err(_) => DEFAULT_MIN_MARGIN,
        };
        let loaded = architecture::load(&ctx.project_root)?;
        let mut graph = read_graph(&ctx.project_root)?;
        if let Some((arch, file)) = &loaded {
            let names = graph.members.iter().map(|m| m.name.clone()).collect();
            arch.validate(&names, file)?;
            modgraph::add_module_groups(arch, &mut graph, &ctx.project_root)?;
        }
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Ok(judge(
            loaded.as_ref().map(|(a, _)| a),
            &graph,
            margin,
            &project,
        ))
    }
}

/// The nodes and edges to judge: the declared components, or the workspace members when nothing is declared.
fn nodes_and_edges(
    arch: Option<&ArchitectureConfig>,
    graph: &Graph,
) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    match arch {
        Some(a) if !a.component.is_empty() || !a.layer.is_empty() => component_edges(a, graph),
        _ => (
            graph.members.iter().map(|m| m.name.clone()).collect(),
            graph
                .edges
                .iter()
                .map(|e| (e.from.clone(), e.to.clone()))
                .collect(),
        ),
    }
}

/// `(afferent, efferent)` counts of distinct neighbours per node.
fn couplings(
    nodes: &BTreeSet<String>,
    edges: &BTreeSet<(String, String)>,
) -> BTreeMap<String, (usize, usize)> {
    let mut out: BTreeMap<String, (usize, usize)> =
        nodes.iter().map(|n| (n.clone(), (0, 0))).collect();
    for (a, b) in edges {
        out.entry(a.clone()).or_default().1 += 1;
        out.entry(b.clone()).or_default().0 += 1;
    }
    out
}

/// `Ce / (Ca + Ce)`, or `None` for a node with no edges.
pub(crate) fn instability(ca: usize, ce: usize) -> Option<f64> {
    (ca + ce > 0).then(|| ce as f64 / (ca + ce) as f64)
}

/// The findings and notes for `graph` judged against `arch` (or crate by crate when there is none).
pub(crate) fn judge(
    arch: Option<&ArchitectureConfig>,
    graph: &Graph,
    min_margin: f64,
    project: &str,
) -> (Vec<Finding>, Vec<String>) {
    let (nodes, edges) = nodes_and_edges(arch, graph);
    let coupling = couplings(&nodes, &edges);
    let volatile: BTreeSet<&str> = arch
        .map(|a| {
            a.component
                .iter()
                .filter(|c| c.volatility == Some(Volatility::Volatile))
                .map(|c| c.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    let cycles: Vec<BTreeSet<String>> = sccs(&nodes, &edges)
        .into_iter()
        .map(|s| s.into_iter().collect())
        .collect();
    let same_cycle = |a: &str, b: &str| cycles.iter().any(|s| s.contains(a) && s.contains(b));
    let manifest_of = |component: &str| -> Option<String> {
        graph
            .members
            .iter()
            .find(|m| m.name == component)
            .map(|m| m.manifest.clone())
    };
    let mut ranked: Vec<(f64, &(String, String), f64, f64)> = Vec::new();
    let (mut skipped_volatile, mut skipped_cycle) = (0usize, 0usize);
    for e in &edges {
        let (a, b) = (&e.0, &e.1);
        if volatile.contains(b.as_str()) {
            skipped_volatile += 1;
            continue;
        }
        if same_cycle(a, b) {
            skipped_cycle += 1;
            continue;
        }
        let (Some(ia), Some(ib)) = (
            coupling.get(a).and_then(|&(ca, ce)| instability(ca, ce)),
            coupling.get(b).and_then(|&(ca, ce)| instability(ca, ce)),
        ) else {
            continue;
        };
        let margin = ib - ia;
        // a small tolerance keeps a margin of exactly 0.3 in when the ratio is 0.30000000000000004
        if margin + 1e-9 >= min_margin {
            ranked.push((margin, e, ia, ib));
        }
    }
    ranked.sort_by(|x, y| {
        y.0.partial_cmp(&x.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.1.cmp(y.1))
    });
    let findings = ranked
        .iter()
        .map(|(margin, (a, b), ia, ib)| {
            let (ca_b, ce_b) = coupling[b];
            let (ca_a, ce_a) = coupling[a];
            let file = manifest_of(a).unwrap_or_else(|| crate::config::FILE_NAME.into());
            Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: Severity::Info,
                confidence: Confidence::Low,
                project: project.into(),
                location: Some(Location { file, line: None }),
                subject: Some(format!("{a}->{b}")),
                summary: format!(
                    "`{a}` (instability {ia:.2}) depends on the less stable `{b}` (instability {ib:.2}), a margin of {margin:.2}"
                ),
                detail: format!(
                    "Instability is Ce / (Ca + Ce): `{a}` depends on {ce_a} and is depended on by {ca_a} other component(s); `{b}` depends on {ce_b} and is depended on by {ca_b}. A component that is more likely to change than the one using it spreads that churn inward. Either depend on something that changes less (an abstraction `{a}` owns), or move the part of `{b}` that `{a}` needs down into a component that is depended on more than it depends on. This is a count of neighbours, not of change: if `{b}` is frozen, allowlist the subject `{a}->{b}` with the reason; if it really is volatile, declare `volatility = \"volatile\"` on it; or raise `[arc005] min_margin` (now {min_margin})."
                ),
                member: None,
                positive_control: None,
            }
        })
        .collect();
    let mut notes = vec![format!(
        "ARC-005 read {} component(s) and {} edge(s) (minimum margin {min_margin}): {} reported, {skipped_volatile} edge(s) to a volatile component and {skipped_cycle} inside a cycle left out",
        nodes.len(),
        edges.len(),
        ranked.len()
    )];
    if edges.is_empty() {
        notes.push("ARC-005 read zero edges between components, so it judged nothing: a repository of one crate needs `[[architecture.component]]` module groups (docs/rules/arc.md) to be read below crate level".into());
    }
    (findings, notes)
}

#[cfg(test)]
mod tests;
