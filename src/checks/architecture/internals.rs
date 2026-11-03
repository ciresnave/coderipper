// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reaching into a module's internals (project scope, Rust): does code name an item through the inner modules of a sibling when that
//! sibling re-exports the item at its own entry point?
//!
//! Rule record: `MOD-003` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.4 (the depth rule of its question 3 follows the proposal:
//! a reach-through is reported only where a re-export was bypassed).
//!
//! **What it reads.** The module graph of the package (`modgraph.rs`: the tracked `.rs` files under `src/`, test items skipped) with the
//! line of every path written in an item, and every `pub`, `pub(crate)` and `pub(super)` `use` (a **re-export**: the module offers the
//! item at its own entry point).
//!
//! **A reach-through** is a path written in module `M` (or a `use` of one) that resolves to an item `X` inside a module `a::b::c`, where
//! `a` is a sibling of `M` or an ancestor's other child (the path enters `a`, and then goes at least one module deeper to `b`), and a
//! module on the way (`a`, or `a::b`) re-exports `X` (by name, under another name, or by a glob of the module that holds it). The
//! caller bypassed the entry point the owner offers. A path that stays inside its own module subtree is never one, and the `use` that
//! builds the re-export is not one.
//!
//! **Weak evidence by construction (medium confidence).** Syntactic: a re-export is matched on the resolved path, so one made through
//! a macro, an alias or another crate is not seen; a glob re-export is taken to offer every item of the module it globs, which is
//! not true of a private one; a path inside a macro call's tokens is not read. A reach with no re-export in the way is not reported
//! (the owner never offered a shorter path, so the rule cannot say the caller bypassed it); that is why this is the narrow reading
//! of the design question. One **Low** finding per file with the count and the first lines.
//!
//! **Known limitations / deliberate scope:** one package at a time, at `HEAD`; a file `syn` cannot parse is an error, never a
//! smaller count; `#[path]` modules and files declared by no `mod` are not read (noted).

use super::modgraph::{self, Facts};
use crate::check::{Check, CheckContext, Network, Scope};
use crate::checks::rust_sources::read_sources;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_003;

/// Reports paths that bypass an entry point a module offers (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct InternalsCheck;

impl InternalsCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for InternalsCheck {
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
        let (package, _) = crate::package::locate(&ctx.project_root)?;
        let files = read_sources(&package.dir)?;
        let Some(facts) = modgraph::read(&files)? else {
            return Ok((
                Vec::new(),
                vec!["no src/lib.rs or src/main.rs among the tracked sources; no module graph, so no reach-through is judged".into()],
            ));
        };
        let reaches = reach_throughs(&facts);
        let project = package.dir.file_name().map_or_else(
            || package.name.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut notes = facts.notes.clone();
        notes.push(format!(
            "MOD-003 read {} file(s), {} path(s) and {} re-export(s); {} reach-through(s)",
            facts.files.len(),
            facts.refs.len(),
            facts.reexports.len(),
            reaches.len()
        ));
        Ok((judge(&reaches, &project), notes))
    }
}

/// One path that bypasses a re-export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reach {
    pub file: String,
    pub line: usize,
    /// The path as it resolves, `crate::a::b::X`.
    pub written: String,
    /// The shorter path the module on the way offers, `crate::a::X`.
    pub via: String,
}

fn crate_path(segs: &[String]) -> String {
    if segs.is_empty() {
        "crate".to_string()
    } else {
        format!("crate::{}", segs.join("::"))
    }
}

/// Every path in `f` that names an item inside a sibling's inner module when a module on the way re-exports it.
pub(crate) fn reach_throughs(f: &Facts) -> Vec<Reach> {
    // the `use` that builds a re-export is the interface itself, not a caller of it
    let reexport_paths: Vec<(&str, &[String])> = f
        .reexports
        .iter()
        .map(|e| (e.module.as_str(), e.segs.as_slice()))
        .collect();
    let mut out = Vec::new();
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
        let n = modgraph::module_prefix(&f.mods, &abs);
        let Some(item) = abs.get(n) else {
            continue;
        };
        let mp = &abs[..n];
        let here: Vec<&str> = r.module.split("::").filter(|s| !s.is_empty()).collect();
        let common = here
            .iter()
            .zip(mp.iter())
            .take_while(|(a, b)| *a == b)
            .count();
        // the path must enter a sibling root (mp[common]) and then go at least one module deeper
        if mp.len() < common + 2 {
            continue;
        }
        let via = (common + 1..mp.len()).find_map(|k| {
            let holder = mp[..k].join("::");
            f.reexports.iter().find_map(|e| {
                if e.module != holder {
                    return None;
                }
                let t = modgraph::resolve_abs(&f.mods, &e.module, &e.segs)?;
                let offers = if e.name == "*" {
                    t == mp
                } else {
                    t.len() > mp.len() && t.starts_with(mp) && &t[mp.len()] == item
                };
                offers.then(|| {
                    let mut p = mp[..k].to_vec();
                    p.push(if e.name == "*" {
                        item.clone()
                    } else {
                        e.name.clone()
                    });
                    crate_path(&p)
                })
            })
        });
        if let Some(via) = via {
            out.push(Reach {
                file: r.file.clone(),
                line: r.line,
                written: crate_path(&abs),
                via,
            });
        }
    }
    out.sort_by(|a, b| (&a.file, a.line, &a.written).cmp(&(&b.file, b.line, &b.written)));
    out.dedup();
    out
}

/// One finding per file.
pub(crate) fn judge(reaches: &[Reach], project: &str) -> Vec<Finding> {
    let mut by_file: BTreeMap<&str, Vec<&Reach>> = BTreeMap::new();
    for r in reaches {
        by_file.entry(r.file.as_str()).or_default().push(r);
    }
    by_file
        .into_iter()
        .map(|(file, rs)| {
            let first = rs[0].line;
            let shown = rs
                .iter()
                .take(5)
                .map(|r| format!("line {}: `{}` (offered as `{}`)", r.line, r.written, r.via))
                .collect::<Vec<_>>()
                .join("; ");
            Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: Severity::Low,
                confidence: Confidence::Medium,
                project: project.into(),
                location: Some(Location {
                    file: file.into(),
                    line: u32::try_from(first).ok(),
                }),
                subject: Some(file.into()),
                summary: format!(
                    "{file} names {} item(s) through a sibling's inner modules that the sibling re-exports at its entry point, the first at line {first}",
                    rs.len()
                ),
                detail: format!(
                    "{shown}{}. A path that goes through a module's inner files binds the caller to a layout the owner expects to change; the owner already offers the item at a shorter path. Use the shorter path, or, if the deep path is deliberate, allowlist the file with a reason. Syntactic: a re-export made through a macro or an alias is not seen, and a glob re-export is taken to offer every item of the module it globs.",
                    if rs.len() > 5 { "; ..." } else { "" }
                ),
                member: None,
                positive_control: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
