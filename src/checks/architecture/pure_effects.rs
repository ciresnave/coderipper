// SPDX-License-Identifier: MIT OR Apache-2.0
//! Components declared pure avoid effectful APIs (repository scope, Rust): does a component tagged `pure = true` name a file, network,
//! environment, process, clock, randomness or thread API?
//!
//! Rule record: `EFX-001` in `rules/efx.toml`. It reads the `[architecture]` declaration (`pure = true` on a component, and the optional
//! `[architecture.effects]` table; design: `docs/superpowers/specs/2026-11-12-architecture-declaration-extension-proposal.md`, section 2.1).
//!
//! **What it judges.** For each component with `pure = true`, in the tracked `.rs` files under `src/` of each of its crates at `HEAD`
//! (files of test code by name and `#[cfg(test)]` / `#[test]` items skipped), every path an expression or a type names, resolved through
//! the file's own `use` declarations (a renamed import is still seen), compared with the effect list. The list is the built-in one
//! (`std::fs`, `tokio::fs`, `std::net`, `tokio::net`, `reqwest`, `hyper`, `std::env`, `std::process`, `tokio::process`,
//! `std::time::SystemTime`, `std::time::Instant`, `chrono::Utc::now`, `chrono::Local::now`, `rand`, `getrandom`, `std::thread`,
//! `tokio::spawn`) plus `[architecture.effects] add` minus `remove`, **one list for the whole repository, not per component**. An entry
//! matches a path that starts with all of its segments. One finding per site and effect, at Medium severity (the record's own).
//!
//! **What `pure` means here.** *None of the listed effects is named*, not referentially pure: a `static`, `unsafe`, interior mutability,
//! `println!` and the like are outside the list, so a component can pass this rule and still not be a pure function of its inputs.
//!
//! **What it cannot see (the record says so):** an effect behind a trait object the component is handed (that is the point of a pure core:
//! the effect lives in the adapter), macro-generated or macro-argument calls, a type alias in another crate, a glob import
//! (`use std::fs::*;`: counted in a note, its names are not resolved), a method call on a value whose type is inferred
//! (`reader.read_to_string()` where `reader` came from elsewhere), and an effect reached through a dependency that is not on the list.
//!
//! **No declaration, or no pure component, is not clean:** the rule reports "not run" with what to add ([`Check::not_configured`]),
//! unless the declaration says `none = ["pure"]` (then it runs and reports 0 sites by declaration).
//!
//! **Deliberate scope:** crate granularity (a component of module paths inside a crate is noted, not judged); only `src/` is read.

use super::api_budget::flatten_use;
use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig, NoneInput};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::visit::Visit;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::EFX_001;

/// Reports effectful APIs named in components declared pure (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct PureEffectsCheck;

impl PureEffectsCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so EFX-001 has no component declared pure to read; a missing declaration is a gap, not a pass (docs/rules/efx.md)".into());
    };
    if a.declares_none(NoneInput::Pure) {
        return None;
    }
    if !a.component.iter().any(|c| c.pure == Some(true)) {
        return Some("no [[architecture.component]] has pure = true, so there is no component whose effects to read; tag one, or say none = [\"pure\"] (docs/rules/efx.md)".into());
    }
    None
}

/// One reference to a listed effect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Hit {
    line: usize,
    /// The resolved path as written, `std::fs::read_to_string`.
    path: String,
    /// The list entry it matched, `std::fs`.
    entry: String,
}

struct Scan<'a> {
    uses: &'a BTreeMap<String, Vec<String>>,
    entries: &'a [Vec<String>],
    hits: Vec<Hit>,
}

impl Scan<'_> {
    fn note(&mut self, p: &syn::Path) {
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        let Some(first) = segs.first() else { return };
        if matches!(first.as_str(), "crate" | "self" | "super" | "Self") {
            return;
        }
        let full: Vec<String> = match self.uses.get(first) {
            Some(u) => u.iter().chain(segs.iter().skip(1)).cloned().collect(),
            None => segs,
        };
        for e in self.entries {
            if full.len() >= e.len() && full[..e.len()] == e[..] {
                self.hits.push(Hit {
                    line: p.segments[0].ident.span().start().line,
                    path: full.join("::"),
                    entry: e.join("::"),
                });
                break;
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_cfg_test(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_cfg_test(&f.attrs) {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_cfg_test(&i.attrs) {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_cfg_test(&f.attrs) {
            syn::visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {
        // an import is not an effect; the uses of what it names are read where they occur
    }

    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        if e.qself.is_none() {
            self.note(&e.path);
        }
        syn::visit::visit_expr_path(self, e);
    }

    fn visit_expr_struct(&mut self, e: &'ast syn::ExprStruct) {
        self.note(&e.path);
        syn::visit::visit_expr_struct(self, e);
    }

    fn visit_type_path(&mut self, t: &'ast syn::TypePath) {
        if t.qself.is_none() {
            self.note(&t.path);
        }
        syn::visit::visit_type_path(self, t);
    }
}

/// The `use` names of a file (nested modules read as one scope, test modules skipped): introduced name -> full path; and the glob
/// prefixes.
fn collect_uses(
    items: &[syn::Item],
    uses: &mut BTreeMap<String, Vec<String>>,
    globs: &mut Vec<Vec<String>>,
) {
    for item in items {
        match item {
            syn::Item::Use(u) if !is_cfg_test(&u.attrs) => {
                let mut leaves = Vec::new();
                flatten_use(
                    &u.tree,
                    &mut Vec::new(),
                    u.leading_colon.is_some(),
                    &mut leaves,
                );
                for (mut segs, name, glob, _) in leaves {
                    if glob {
                        segs.pop();
                        globs.push(segs);
                        continue;
                    }
                    if segs.last().is_some_and(|l| l == "self") {
                        segs.pop();
                    }
                    uses.insert(name, segs);
                }
            }
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    collect_uses(inner, uses, globs);
                }
            }
            _ => {}
        }
    }
}

/// The effect references of one file, and the glob imports of an effect namespace it has (names a glob brings in are not resolved).
fn scan(f: &SourceFile, entries: &[Vec<String>]) -> anyhow::Result<(Vec<Hit>, usize)> {
    let parsed = syn::parse_file(&f.text).map_err(|e| {
        anyhow::anyhow!(
            "could not parse {} with syn ({e}); refusing to guess",
            f.path
        )
    })?;
    let mut uses = BTreeMap::new();
    let mut globs = Vec::new();
    collect_uses(&parsed.items, &mut uses, &mut globs);
    let glob_effects = globs
        .iter()
        .filter(|g| entries.iter().any(|e| e.starts_with(g) || g.starts_with(e)))
        .count();
    let mut s = Scan {
        uses: &uses,
        entries,
        hits: Vec::new(),
    };
    s.visit_file(&parsed);
    s.hits.sort();
    s.hits.dedup();
    Ok((s.hits, glob_effects))
}

fn finding(component: &str, manifest: &str, file: &str, h: &Hit, project: &str) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity: Severity::Medium,
        confidence: Confidence::High,
        project: project.into(),
        location: Some(Location {
            file: file.into(),
            line: u32::try_from(h.line).ok(),
        }),
        subject: Some(format!("{component}:{}", h.path)),
        summary: format!(
            "the component `{component}` is declared pure and names `{}` (effect `{}`)",
            h.path, h.entry
        ),
        detail: format!(
            "`{component}` (manifest {manifest}) is tagged `pure = true` in [architecture], and this code names `{}`, which is on the effect list under `{}`. \
             Move the effectful call out to the caller and pass in the value, or pass a capability the tests can replace; or, if the component is not meant to be pure, \
             remove its `pure` tag; or change the list with [architecture.effects]. Pure here means none of the listed effects is named, not referentially pure. \
             Read from the syntax and the file's `use` declarations only: a glob import, a type alias in another crate, a macro and a method on an inferred type are not seen.",
            h.path, h.entry
        ),
        member: None,
        positive_control: None,
    }
}

impl Check for PureEffectsCheck {
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
        let graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        if arch.declares_none(NoneInput::Pure) {
            return Ok((
                Vec::new(),
                vec![architecture::none_note(CHECK_ID, NoneInput::Pure)],
            ));
        }
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let entries: Vec<Vec<String>> = arch
            .effect_list()
            .iter()
            .map(|e| e.split("::").map(str::to_string).collect())
            .collect();
        let resolved = Resolved::new(&arch, &graph.members);
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        let (mut files_read, mut globs) = (0usize, 0usize);
        for c in arch.component.iter().filter(|c| c.pure == Some(true)) {
            if c.paths
                .iter()
                .any(|p| is_module_path(&norm(p), &graph.members))
            {
                notes.push(format!(
                    "component `{}` has module paths inside a crate; module groups are not judged in this version, only whole crates",
                    c.name
                ));
                continue;
            }
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                for f in read_sources(&ctx.project_root.join(&m.dir))? {
                    let shown = if m.dir.is_empty() {
                        f.path.clone()
                    } else {
                        format!("{}/{}", m.dir, f.path)
                    };
                    files_read += 1;
                    let (hits, g) = scan(&f, &entries)?;
                    globs += g;
                    findings.extend(
                        hits.iter()
                            .map(|h| finding(&c.name, &m.manifest, &shown, h, &project)),
                    );
                }
            }
        }
        findings.sort_by(|a, b| {
            (&a.location.as_ref().map(|l| (&l.file, l.line)), &a.subject)
                .cmp(&(&b.location.as_ref().map(|l| (&l.file, l.line)), &b.subject))
        });
        notes.push(format!(
            "EFX-001 read {files_read} file(s) of components declared pure against {} effect entries; {} effect reference(s) found",
            entries.len(),
            findings.len()
        ));
        if globs > 0 {
            notes.push(format!(
                "EFX-001: {globs} glob import(s) of an effect namespace (for example `use std::fs::*;`); the names they bring in are not resolved, so uses through them are not seen"
            ));
        }
        Ok((findings, notes))
    }
}

#[cfg(test)]
mod tests;
