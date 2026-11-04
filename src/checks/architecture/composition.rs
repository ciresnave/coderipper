// SPDX-License-Identifier: MIT OR Apache-2.0
//! Construction only in composition roots (repository scope, Rust): does policy code build a concrete adapter itself, instead of
//! receiving it from the place that wires the program together?
//!
//! Rule record: `ARC-012` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; the lane brief is `docs/lanes/package-c.md`.
//!
//! **What it judges.** The *adapter types* are the structs declared in the crates of components whose `role` is `adapter` or
//! `infrastructure` and that implement (`impl Port for X`, not blanket) a *port*: a trait that is declared in a workspace crate, or, when
//! `[architecture.composition] ports` is set, one of the traits it names. In every tracked `.rs` file under `src/` of the crates of each
//! *policy* component (`role = "policy"`, or no role and `volatility = "stable"`), minus the composition roots, binaries and test code, a
//! *construction* of an adapter type is reported: a struct literal `X { .. }`, a unit struct used as a value (`X`), or an associated call that looks like a constructor
//! (`X::new`, `default`, `open`, `connect`, `create`, `build`, `init`, `try_new`, `new_*`, `with_*`, `from_*`) where `X` resolves, through the
//! file's `use` declarations, to a path rooted in the adapter's crate.
//!
//! **Exempt.** A file named by `[architecture.composition] roots` (a file, a directory, or a `*`/`**` glob, relative to the repository),
//! `src/main.rs` and `src/bin/**` (the program's own entry), a component with `role = "composition"` or `"test-support"`, and `#[cfg(test)]`
//! items and test files.
//!
//! **Weak evidence by construction (medium confidence).** Syntactic: a type is matched by name through `use` paths; a glob import, a type
//! alias, a construction through a macro or a factory function that lives elsewhere are not seen. A name that looks like a constructor
//! but returns something else is reported. The whole file's `use` declarations are read as one scope. A repository that declares no roots
//! and no `composition` component gets "not run" (everything would be outside a root): declare them first.
//!
//! **Known limitations / deliberate scope:** crate granularity (module-path components are noted, not judged); only `src/` is read; a
//! dependency-injection container consulted outside a root (the second half of the catalog statement) is not looked for.

use super::api_budget::flatten_use;
use super::layers::collect;
use super::modgraph::matches_path;
use super::{is_module_path, norm, read_graph, Member, Resolved};
use crate::architecture::{self, ArchitectureConfig, ComponentDecl, Role, Volatility};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_012;

/// Reports concrete adapters built in policy code outside a composition root (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct CompositionRootsCheck;

impl CompositionRootsCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn is_policy(c: &ComponentDecl) -> bool {
    c.role == Some(Role::Policy) || (c.role.is_none() && c.volatility == Some(Volatility::Stable))
}

fn is_adapter(c: &ComponentDecl) -> bool {
    matches!(c.role, Some(Role::Adapter | Role::Infrastructure))
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-012 has no composition root to compare; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into());
    };
    let roots = a.composition.as_ref().is_some_and(|c| !c.roots.is_empty())
        || a.component
            .iter()
            .any(|c| c.role == Some(Role::Composition));
    if !roots {
        return Some("the [architecture] table declares no [architecture.composition] roots and no component with role = \"composition\", so every construction would be outside a root; declare where wiring happens (docs/rules/arc.md)".into());
    }
    if !a.component.iter().any(is_adapter) {
        return Some("no [[architecture.component]] has role = \"adapter\" or \"infrastructure\", so there is no concrete adapter to look for (docs/rules/arc.md)".into());
    }
    if !a.component.iter().any(is_policy) {
        return Some("no [[architecture.component]] has role = \"policy\" (or no role and volatility = \"stable\"), so there is no policy code to read (docs/rules/arc.md)".into());
    }
    None
}

impl Check for CompositionRootsCheck {
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
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = Resolved::new(&arch, &graph.members);
        let component_of = |m: &Member| {
            resolved
                .component
                .get(m.name.as_str())
                .and_then(Option::as_deref)
                .and_then(|n| arch.component.iter().find(|c| c.name == n))
        };
        let mut notes = Vec::new();
        // read every member once
        let mut sources: BTreeMap<&str, Vec<SourceFile>> = BTreeMap::new();
        for m in &graph.members {
            sources.insert(&m.name, read_sources(&ctx.project_root.join(&m.dir))?);
        }
        // the ports: traits of the workspace, or the ones the declaration names
        let declared_ports: BTreeSet<&str> = arch
            .composition
            .as_ref()
            .map(|c| c.ports.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let mut workspace_traits: BTreeSet<String> = BTreeSet::new();
        let mut facts = BTreeMap::new();
        for m in &graph.members {
            let f = collect(&sources[m.name.as_str()])?;
            workspace_traits.extend(f.traits.iter().map(|t| t.name.clone()));
            facts.insert(m.name.as_str(), f);
        }
        let is_port = |t: &str| {
            if declared_ports.is_empty() {
                workspace_traits.contains(t)
            } else {
                declared_ports.contains(t)
            }
        };
        // adapter types: crate identifier -> struct names that implement a port
        let mut adapters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for m in &graph.members {
            let Some(c) = component_of(m) else { continue };
            if !is_adapter(c) {
                continue;
            }
            let f = &facts[m.name.as_str()];
            let structs: BTreeSet<&str> = f.structs.iter().map(|s| s.name.as_str()).collect();
            let set: BTreeSet<String> = f
                .impls
                .iter()
                .filter(|(tr, ty, blanket)| {
                    !blanket && is_port(tr) && structs.contains(ty.as_str())
                })
                .map(|(_, ty, _)| ty.clone())
                .collect();
            if !set.is_empty() {
                adapters.insert(m.name.replace('-', "_"), set);
            }
        }
        let roots: Vec<String> = arch
            .composition
            .as_ref()
            .map(|c| c.roots.clone())
            .unwrap_or_default();
        let mut findings = Vec::new();
        let mut files_read = 0usize;
        for c in arch.component.iter().filter(|c| is_policy(c)) {
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
            for m in graph
                .members
                .iter()
                .filter(|m| component_of(m).is_some_and(|x| x.name == c.name))
            {
                for f in &sources[m.name.as_str()] {
                    let shown = if m.dir.is_empty() {
                        f.path.clone()
                    } else {
                        format!("{}/{}", m.dir, f.path)
                    };
                    if f.path == "src/main.rs"
                        || f.path.starts_with("src/bin/")
                        || roots.iter().any(|r| matches_path(r, &shown))
                    {
                        continue;
                    }
                    files_read += 1;
                    findings.extend(constructions(f, &shown, &c.name, &adapters, &project)?);
                }
            }
        }
        findings.sort_by(|a, b| {
            (&a.location.as_ref().map(|l| (&l.file, l.line)), &a.subject)
                .cmp(&(&b.location.as_ref().map(|l| (&l.file, l.line)), &b.subject))
        });
        notes.push(format!(
            "ARC-012 read {files_read} file(s) of policy components against {} adapter type(s) in {} adapter crate(s); {} construction(s) outside a root",
            adapters.values().map(BTreeSet::len).sum::<usize>(),
            adapters.len(),
            findings.len()
        ));
        Ok((findings, notes))
    }
}

/// Names that read as a constructor.
fn looks_like_constructor(name: &str) -> bool {
    matches!(
        name,
        "new" | "default" | "open" | "connect" | "create" | "build" | "init" | "try_new" | "from"
    ) || ["new_", "with_", "from_"]
        .iter()
        .any(|p| name.starts_with(p))
}

struct Finder<'a> {
    uses: BTreeMap<String, Vec<String>>,
    adapters: &'a BTreeMap<String, BTreeSet<String>>,
    hits: Vec<(String, usize)>,
}

impl Finder<'_> {
    /// `crate::Type` of an adapter that `segs` names, through the file's `use` declarations.
    fn adapter(&self, segs: &[String]) -> Option<String> {
        let full: Vec<String> = match segs.first().and_then(|f| self.uses.get(f)) {
            Some(u) => u.iter().chain(segs.iter().skip(1)).cloned().collect(),
            None => segs.to_vec(),
        };
        let krate = full.first()?;
        let ty = full.last()?;
        self.adapters
            .get(krate)
            .filter(|set| set.contains(ty) && full.len() >= 2)
            .map(|_| format!("{krate}::{ty}"))
    }
}

fn segs(p: &syn::Path) -> Vec<String> {
    p.segments.iter().map(|s| s.ident.to_string()).collect()
}

impl<'ast> Visit<'ast> for Finder<'_> {
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

    fn visit_expr_struct(&mut self, e: &'ast syn::ExprStruct) {
        if let Some(a) = self.adapter(&segs(&e.path)) {
            self.hits
                .push((a, e.path.segments[0].ident.span().start().line));
        }
        syn::visit::visit_expr_struct(self, e);
    }

    /// A unit struct used as a value (`let a = Adapter;`).
    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        if e.qself.is_none() {
            if let Some(a) = self.adapter(&segs(&e.path)) {
                self.hits
                    .push((a, e.path.segments[0].ident.span().start().line));
            }
        }
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*e.func {
            let s = segs(&p.path);
            if s.len() >= 3 || (s.len() == 2 && self.uses.contains_key(&s[0])) {
                let (ctor, ty_path) = (&s[s.len() - 1], &s[..s.len() - 1]);
                if looks_like_constructor(ctor) {
                    if let Some(a) = self.adapter(ty_path) {
                        self.hits
                            .push((a, p.path.segments[0].ident.span().start().line));
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, e);
    }
}

/// The adapter constructions in one file of a policy component.
fn constructions(
    f: &SourceFile,
    shown: &str,
    component: &str,
    adapters: &BTreeMap<String, BTreeSet<String>>,
    project: &str,
) -> anyhow::Result<Vec<Finding>> {
    let parsed = syn::parse_file(&f.text).map_err(|e| {
        anyhow::anyhow!(
            "could not parse {} with syn ({e}); refusing to guess",
            f.path
        )
    })?;
    let mut uses = BTreeMap::new();
    collect_uses(&parsed.items, &mut uses);
    let mut v = Finder {
        uses,
        adapters,
        hits: Vec::new(),
    };
    for item in &parsed.items {
        match item {
            syn::Item::Mod(_) | syn::Item::Fn(_) | syn::Item::Impl(_) => v.visit_item(item),
            syn::Item::Const(_) | syn::Item::Static(_) => v.visit_item(item),
            _ => {}
        }
    }
    v.hits.sort();
    v.hits.dedup();
    Ok(v.hits
        .into_iter()
        .map(|(ty, line)| Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Medium,
            confidence: Confidence::Medium,
            project: project.into(),
            location: Some(Location {
                file: shown.to_string(),
                line: u32::try_from(line).ok(),
            }),
            subject: Some(format!("{component}:{ty}")),
            summary: format!(
                "the policy component `{component}` builds the adapter `{ty}` itself, outside a composition root"
            ),
            detail: format!(
                "`{ty}` is a concrete type of an adapter or infrastructure component that implements a port, and this code of the policy component `{component}` constructs it, so policy chooses its own implementation and the boundary dissolves. Take the port (a generic parameter or a trait object) and let the composition root pass the adapter in, or add the file to [architecture.composition] roots if it is where wiring belongs, or allowlist the subject `{component}:{ty}` with a reason. Read from `use` paths; a glob import, a type alias, a macro and a factory function elsewhere are not seen, and a constructor-looking name is taken to build the type."
            ),
            member: None,
            positive_control: None,
        })
        .collect())
}

/// Every `use` of the file (nested modules included, read as one scope): introduced name -> full path.
fn collect_uses(items: &[syn::Item], out: &mut BTreeMap<String, Vec<String>>) {
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
                        continue;
                    }
                    if segs.last().is_some_and(|l| l == "self") {
                        segs.pop();
                    }
                    out.insert(name, segs);
                }
            }
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    collect_uses(inner, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
