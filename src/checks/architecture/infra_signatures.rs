// SPDX-License-Identifier: MIT OR Apache-2.0
//! Infrastructure types kept out of policy signatures (repository scope, Rust): does a policy component's public API name, in a
//! parameter, a return type or a field, a type that belongs to infrastructure?
//!
//! Rule record: `ARC-004` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; background: `docs/rules/arc.md`; the lane brief is
//! `docs/lanes/package-c.md`.
//!
//! **What it judges.** For each *policy* component (`role = "policy"`, or no role with `volatility = "stable"`), in the library
//! (`src/lib.rs`) of each of its crates at `HEAD` (test items skipped), every type named by the signature of a *public* item:
//! a `pub` function, a `pub` method of an inherent `impl` of a public type, the methods and associated items of a public trait, the `pub`
//! fields of a public struct or union, the fields of the variants of a public enum, and the target of a public type alias, constant or
//! static. An item counts as public only when every module between it and the crate root is `pub` (as in ARC-006).
//!
//! **What is infrastructure.** A path is infrastructure when, after the names it uses are resolved through the module's own `use`
//! declarations, it
//! - starts with a namespace in `[architecture.volatile] namespaces` or a path pattern in `[arc004] patterns` (an entry with `::`), or
//! - ends in a name in `[architecture.volatile] names` or a name pattern in `[arc004] patterns` (an entry without `::`), or
//! - starts with a workspace crate that belongs to a component declared `volatility = "volatile"` or `role = "adapter"` or
//!   `"infrastructure"`.
//!
//! A public type alias in the same crate whose target is infrastructure is looked through (one level), so the alias does not hide it.
//! One finding per signature site, naming every infrastructure type it contains. The finding's subject is `<component>:<item>`, so
//! an exception for a deliberate case (a value type that only derives an annotation) is an allowlist entry with a reason.
//!
//! **Relation to ARC-003.** ARC-003 counts the *tokens* of volatile names and namespaces anywhere in a stable component (one finding
//! per component, imports included). This rule reports *where a public signature leaks* one; it neither counts imports nor repeats
//! ARC-003's per-component total, and it reads the resolved path rather than the token, so a renamed import is still seen.
//!
//! **Weak evidence by construction (medium confidence).** Syntactic: a glob import (`use sqlx::*;`) does not resolve the names it
//! brings in, a type alias in another crate or a re-export through a facade hides the origin, macro-generated items are not seen, and
//! a `#[path]` module is not read. A name pattern matches the last segment of any path with that name. Medium severity.
//!
//! **No declaration, no policy component, or nothing to look for is not clean:** the rule reports "not run" with what to add
//! ([`Check::not_configured`]).
//!
//! **Known limitations / deliberate scope:** crate granularity (module-path components are noted, not judged); only `src/` is read; trait
//! `impl` items are not judged (their signature is dictated by the trait).

use super::api_budget::flatten_use;
use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig, ComponentDecl, Role, Volatility};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_004;

/// Reports infrastructure types named in the public signatures of policy components (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct InfraSignatureCheck;

impl InfraSignatureCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// A component this rule judges.
fn is_policy(c: &ComponentDecl) -> bool {
    c.role == Some(Role::Policy) || (c.role.is_none() && c.volatility == Some(Volatility::Stable))
}

/// A component whose crates are infrastructure by declaration.
fn is_infrastructure(c: &ComponentDecl) -> bool {
    c.volatility == Some(Volatility::Volatile)
        || matches!(c.role, Some(Role::Adapter | Role::Infrastructure))
}

fn missing_declaration(a: Option<&ArchitectureConfig>, patterns: &[String]) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-004 has no policy component to judge; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into());
    };
    if !a.component.iter().any(is_policy) {
        return Some("no [[architecture.component]] has role = \"policy\" (or no role and volatility = \"stable\"), so there is no policy component whose signatures to read (docs/rules/arc.md)".into());
    }
    let volatile_empty = a
        .volatile
        .as_ref()
        .is_none_or(|v| v.names.is_empty() && v.namespaces.is_empty());
    if volatile_empty && patterns.is_empty() && !a.component.iter().any(is_infrastructure) {
        return Some("nothing is declared as infrastructure: add [architecture.volatile] names or namespaces, `[arc004] patterns`, or a component with volatility = \"volatile\" or role = \"adapter\"/\"infrastructure\" (docs/rules/arc.md)".into());
    }
    None
}

impl Check for InfraSignatureCheck {
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
        let patterns = crate::config::arc004_patterns(&ctx.project_root).ok()?;
        match architecture::load(&ctx.project_root) {
            Ok(Some((a, _))) => missing_declaration(Some(&a), &patterns),
            Ok(None) => missing_declaration(None, &patterns),
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
        let patterns = crate::config::arc004_patterns(&ctx.project_root)?;
        let graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = Resolved::new(&arch, &graph.members);
        let banned = Banned::new(&arch, &patterns, &graph.members, &resolved);
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        let mut sites_read = 0usize;
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
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                let files = read_sources(&ctx.project_root.join(&m.dir))?;
                let (sites, n) = signature_sites(&files, &m.dir)?;
                sites_read += sites.len();
                notes.extend(n.into_iter().map(|s| format!("crate `{}`: {s}", m.name)));
                findings.extend(judge(&c.name, &m.manifest, &sites, &banned, &project));
            }
        }
        findings.sort_by(|a, b| (&a.subject, &a.summary).cmp(&(&b.subject, &b.summary)));
        notes.push(format!(
            "ARC-004 read {sites_read} public signature site(s) in policy components; {} leak infrastructure",
            findings.len()
        ));
        Ok((findings, notes))
    }
}

/// What counts as infrastructure.
struct Banned {
    names: BTreeSet<String>,
    prefixes: Vec<Vec<String>>,
    /// Crate identifier (`my_db`) -> the infrastructure component it belongs to.
    crates: BTreeMap<String, String>,
}

impl Banned {
    fn new(
        arch: &ArchitectureConfig,
        patterns: &[String],
        members: &[super::Member],
        resolved: &Resolved,
    ) -> Self {
        let mut names = BTreeSet::new();
        let mut prefixes = Vec::new();
        let mut add = |entry: &str, as_path: bool| {
            let e = entry.trim().trim_end_matches('*').trim_end_matches("::");
            if e.is_empty() {
                return;
            }
            if as_path || e.contains("::") {
                prefixes.push(e.split("::").map(str::to_string).collect());
            } else {
                names.insert(e.to_string());
            }
        };
        if let Some(v) = &arch.volatile {
            v.names.iter().for_each(|n| add(n, false));
            v.namespaces.iter().for_each(|n| add(n, true));
        }
        patterns.iter().for_each(|p| add(p, false));
        let infra: BTreeSet<&str> = arch
            .component
            .iter()
            .filter(|c| is_infrastructure(c))
            .map(|c| c.name.as_str())
            .collect();
        let crates = members
            .iter()
            .filter_map(|m| {
                let comp = resolved.component.get(m.name.as_str())?.as_deref()?;
                infra
                    .contains(comp)
                    .then(|| (m.name.replace('-', "_"), comp.to_string()))
            })
            .collect();
        Self {
            names,
            prefixes,
            crates,
        }
    }

    /// Why a resolved path is infrastructure, or `None`.
    fn why(&self, path: &[String]) -> Option<String> {
        let first = path.first()?;
        if let Some(comp) = self.crates.get(first) {
            return Some(format!(
                "`{}` (from the infrastructure component `{comp}`)",
                path.join("::")
            ));
        }
        if self
            .prefixes
            .iter()
            .any(|p| path.len() >= p.len() && path[..p.len()] == p[..])
        {
            return Some(format!(
                "`{}` (a declared infrastructure namespace)",
                path.join("::")
            ));
        }
        let last = path.last()?;
        self.names.contains(last).then(|| {
            format!(
                "`{}` (a declared infrastructure type name)",
                path.join("::")
            )
        })
    }
}

/// One public signature: where it is, what it is called, and the paths of the types it names (use-resolved).
#[derive(Debug)]
struct Site {
    label: String,
    file: String,
    line: usize,
    paths: Vec<Vec<String>>,
    /// For a method or associated item: the type or trait that owns it, which must be public for the site to count.
    owner: Option<String>,
}

/// The type paths of a signature, as written, in order.
#[derive(Default)]
struct Paths(Vec<Vec<String>>);

impl<'ast> Visit<'ast> for Paths {
    fn visit_type_path(&mut self, t: &'ast syn::TypePath) {
        if t.qself.is_none() {
            self.0.push(segments(&t.path));
        }
        syn::visit::visit_type_path(self, t);
    }

    fn visit_trait_bound(&mut self, b: &'ast syn::TraitBound) {
        self.0.push(segments(&b.path));
        syn::visit::visit_trait_bound(self, b);
    }
}

fn segments(p: &syn::Path) -> Vec<String> {
    p.segments.iter().map(|s| s.ident.to_string()).collect()
}

fn is_pub(v: &syn::Visibility) -> bool {
    matches!(v, syn::Visibility::Public(_))
}

fn is_hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("doc")
            && a.meta
                .require_list()
                .is_ok_and(|l| l.tokens.to_string().contains("hidden"))
    })
}

/// The `use` names of one module: introduced name -> full path.
type Uses = BTreeMap<String, Vec<String>>;

fn resolve(uses: &Uses, path: &[String]) -> Vec<String> {
    match path.first().and_then(|f| uses.get(f)) {
        Some(full) => full.iter().chain(path.iter().skip(1)).cloned().collect(),
        None => path.to_vec(),
    }
}

struct Reader<'a> {
    by_path: BTreeMap<&'a str, &'a SourceFile>,
    dir: &'a str,
    sites: Vec<Site>,
    /// Names public by declaration or `pub use`, per crate (a method's owner must be one).
    public: BTreeSet<String>,
    /// `pub type Name = ..`: the resolved paths of the target.
    aliases: BTreeMap<String, Vec<Vec<String>>>,
    notes: Vec<String>,
    seen: BTreeSet<String>,
}

/// The public signature sites of the library whose tracked sources are `files`, plus notes. A file `syn` cannot parse is an error.
fn signature_sites(files: &[SourceFile], dir: &str) -> anyhow::Result<(Vec<Site>, Vec<String>)> {
    let by_path: BTreeMap<&str, &SourceFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let Some(root) = by_path.get("src/lib.rs").copied() else {
        return Ok((
            Vec::new(),
            vec![
                "no src/lib.rs among the tracked sources, so no library API; nothing to read"
                    .into(),
            ],
        ));
    };
    let mut r = Reader {
        by_path,
        dir,
        sites: Vec::new(),
        public: BTreeSet::new(),
        aliases: BTreeMap::new(),
        notes: Vec::new(),
        seen: BTreeSet::new(),
    };
    r.file(root, "src", true)?;
    // a method's owner must be a public type; an alias is looked through by `judge` via the sites' own paths
    let public = std::mem::take(&mut r.public);
    let mut sites = std::mem::take(&mut r.sites);
    sites.retain(|s| s.owner.as_ref().is_none_or(|o| public.contains(o)));
    for (name, targets) in &r.aliases {
        // an alias is itself a site (its target is part of the API); users of the alias are expanded here
        for s in &mut sites {
            let mut extra = Vec::new();
            for p in &s.paths {
                if p.len() == 1 && p[0] == *name {
                    extra.extend(targets.iter().cloned());
                }
            }
            s.paths.extend(extra);
        }
    }
    Ok((sites, r.notes))
}

impl Reader<'_> {
    fn shown(&self, file: &str) -> String {
        if self.dir.is_empty() {
            file.to_string()
        } else {
            format!("{}/{file}", self.dir)
        }
    }

    fn file(&mut self, f: &SourceFile, child_dir: &str, reachable: bool) -> anyhow::Result<()> {
        if !self.seen.insert(f.path.clone()) {
            return Ok(());
        }
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        let shown = self.shown(&f.path);
        self.items(&parsed.items, child_dir, &shown, reachable)
    }

    fn site(
        &mut self,
        label: String,
        owner: Option<&str>,
        file: &str,
        line: usize,
        uses: &Uses,
        mut visit: impl FnMut(&mut Paths),
    ) {
        let mut p = Paths::default();
        visit(&mut p);
        let paths: Vec<Vec<String>> = p.0.iter().map(|x| resolve(uses, x)).collect();
        if !paths.is_empty() {
            self.sites.push(Site {
                label,
                file: file.to_string(),
                line,
                paths,
                owner: owner.map(str::to_string),
            });
        }
    }

    #[expect(clippy::too_many_lines, reason = "one arm per item kind")]
    fn items(
        &mut self,
        items: &[syn::Item],
        child_dir: &str,
        shown: &str,
        reachable: bool,
    ) -> anyhow::Result<()> {
        // the module's own `use` names first: an item may be written above the `use` that names its types
        let mut uses = Uses::new();
        for item in items {
            if let syn::Item::Use(u) = item {
                let mut leaves = Vec::new();
                flatten_use(
                    &u.tree,
                    &mut Vec::new(),
                    u.leading_colon.is_some(),
                    &mut leaves,
                );
                for (segs, name, glob, _) in leaves {
                    if glob {
                        continue;
                    }
                    let mut segs = segs;
                    if segs.last().is_some_and(|l| l == "self") {
                        segs.pop();
                    }
                    uses.insert(name.clone(), segs);
                    if reachable && is_pub(&u.vis) {
                        self.public.insert(name);
                    }
                }
            }
        }
        for item in items {
            if is_cfg_test(super::api_budget::attrs_of(item)) {
                continue;
            }
            match item {
                syn::Item::Fn(f) if reachable && is_pub(&f.vis) && !is_hidden(&f.attrs) => {
                    let line = f.sig.ident.span().start().line;
                    self.public.insert(f.sig.ident.to_string());
                    self.site(f.sig.ident.to_string(), None, shown, line, &uses, |p| {
                        p.visit_signature(&f.sig);
                    });
                }
                syn::Item::Struct(s) if reachable && is_pub(&s.vis) && !is_hidden(&s.attrs) => {
                    let n = s.ident.to_string();
                    self.public.insert(n.clone());
                    for (i, fld) in s.fields.iter().enumerate().filter(|(_, f)| is_pub(&f.vis)) {
                        let fname = fld
                            .ident
                            .as_ref()
                            .map_or(i.to_string(), ToString::to_string);
                        let line = fld
                            .ident
                            .as_ref()
                            .map_or_else(|| s.ident.span().start().line, |x| x.span().start().line);
                        self.site(format!("{n}.{fname}"), None, shown, line, &uses, |p| {
                            p.visit_type(&fld.ty);
                        });
                    }
                }
                syn::Item::Union(s) if reachable && is_pub(&s.vis) && !is_hidden(&s.attrs) => {
                    let n = s.ident.to_string();
                    self.public.insert(n.clone());
                    for fld in s.fields.named.iter().filter(|f| is_pub(&f.vis)) {
                        let fname = fld
                            .ident
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_default();
                        let line = fld.ident.as_ref().map_or(0, |x| x.span().start().line);
                        self.site(format!("{n}.{fname}"), None, shown, line, &uses, |p| {
                            p.visit_type(&fld.ty);
                        });
                    }
                }
                syn::Item::Enum(e) if reachable && is_pub(&e.vis) && !is_hidden(&e.attrs) => {
                    let n = e.ident.to_string();
                    self.public.insert(n.clone());
                    for v in &e.variants {
                        let line = v.ident.span().start().line;
                        self.site(format!("{n}::{}", v.ident), None, shown, line, &uses, |p| {
                            for fld in &v.fields {
                                p.visit_type(&fld.ty);
                            }
                        });
                    }
                }
                syn::Item::Trait(t) if reachable && is_pub(&t.vis) && !is_hidden(&t.attrs) => {
                    let n = t.ident.to_string();
                    self.public.insert(n.clone());
                    let line = t.ident.span().start().line;
                    self.site(
                        format!("{n} (supertraits)"),
                        None,
                        shown,
                        line,
                        &uses,
                        |p| {
                            for b in &t.supertraits {
                                p.visit_type_param_bound(b);
                            }
                        },
                    );
                    for it in &t.items {
                        match it {
                            syn::TraitItem::Fn(f) => {
                                let line = f.sig.ident.span().start().line;
                                self.site(
                                    format!("{n}::{}", f.sig.ident),
                                    None,
                                    shown,
                                    line,
                                    &uses,
                                    |p| {
                                        p.visit_signature(&f.sig);
                                    },
                                );
                            }
                            syn::TraitItem::Type(ty) => {
                                let line = ty.ident.span().start().line;
                                self.site(
                                    format!("{n}::{}", ty.ident),
                                    None,
                                    shown,
                                    line,
                                    &uses,
                                    |p| {
                                        p.visit_trait_item_type(ty);
                                    },
                                );
                            }
                            syn::TraitItem::Const(c) => {
                                let line = c.ident.span().start().line;
                                self.site(
                                    format!("{n}::{}", c.ident),
                                    None,
                                    shown,
                                    line,
                                    &uses,
                                    |p| {
                                        p.visit_type(&c.ty);
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
                syn::Item::Type(t) if reachable && is_pub(&t.vis) && !is_hidden(&t.attrs) => {
                    let n = t.ident.to_string();
                    self.public.insert(n.clone());
                    let line = t.ident.span().start().line;
                    let mut p = Paths::default();
                    p.visit_type(&t.ty);
                    self.aliases
                        .insert(n.clone(), p.0.iter().map(|x| resolve(&uses, x)).collect());
                    self.site(n, None, shown, line, &uses, |p| p.visit_type(&t.ty));
                }
                syn::Item::Const(c) if reachable && is_pub(&c.vis) && !is_hidden(&c.attrs) => {
                    let line = c.ident.span().start().line;
                    self.site(c.ident.to_string(), None, shown, line, &uses, |p| {
                        p.visit_type(&c.ty)
                    });
                }
                syn::Item::Static(c) if reachable && is_pub(&c.vis) && !is_hidden(&c.attrs) => {
                    let line = c.ident.span().start().line;
                    self.site(c.ident.to_string(), None, shown, line, &uses, |p| {
                        p.visit_type(&c.ty)
                    });
                }
                syn::Item::Impl(im) if im.trait_.is_none() && !is_hidden(&im.attrs) => {
                    let syn::Type::Path(sp) = &*im.self_ty else {
                        continue;
                    };
                    let Some(owner) = sp.path.segments.last().map(|s| s.ident.to_string()) else {
                        continue;
                    };
                    for it in &im.items {
                        match it {
                            syn::ImplItem::Fn(f) if is_pub(&f.vis) && !is_hidden(&f.attrs) => {
                                let line = f.sig.ident.span().start().line;
                                self.site(
                                    format!("{owner}::{}", f.sig.ident),
                                    Some(&owner),
                                    shown,
                                    line,
                                    &uses,
                                    |p| {
                                        p.visit_signature(&f.sig);
                                    },
                                );
                            }
                            syn::ImplItem::Const(c) if is_pub(&c.vis) => {
                                let line = c.ident.span().start().line;
                                self.site(
                                    format!("{owner}::{}", c.ident),
                                    Some(&owner),
                                    shown,
                                    line,
                                    &uses,
                                    |p| {
                                        p.visit_type(&c.ty);
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
                syn::Item::Mod(m) => {
                    let name = m.ident.to_string();
                    let public = reachable && is_pub(&m.vis);
                    if m.attrs.iter().any(|a| a.path().is_ident("path")) {
                        self.notes.push(format!(
                            "module `{name}` uses #[path]; its contents are not read"
                        ));
                        continue;
                    }
                    if let Some((_, inner)) = &m.content {
                        let nested = format!("{child_dir}/{name}");
                        self.items(inner, &nested, shown, public)?;
                    } else {
                        let flat = format!("{child_dir}/{name}.rs");
                        let nested = format!("{child_dir}/{name}/mod.rs");
                        let found = self
                            .by_path
                            .get(flat.as_str())
                            .or_else(|| self.by_path.get(nested.as_str()))
                            .copied();
                        match found {
                            Some(f) => self.file(f, &format!("{child_dir}/{name}"), public)?,
                            None => self.notes.push(format!(
                                "module `{name}` has no tracked file {flat} or {nested}; its contents are not read"
                            )),
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// The findings for the sites of one crate of a policy component.
fn judge(
    component: &str,
    manifest: &str,
    sites: &[Site],
    banned: &Banned,
    project: &str,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for s in sites {
        let mut hits: Vec<String> = s.paths.iter().filter_map(|p| banned.why(p)).collect();
        hits.sort();
        hits.dedup();
        if hits.is_empty() {
            continue;
        }
        let shown = hits.join(", ");
        out.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Medium,
            confidence: Confidence::Medium,
            project: project.into(),
            location: Some(Location {
                file: s.file.clone(),
                line: u32::try_from(s.line).ok(),
            }),
            subject: Some(format!("{component}:{}", s.label)),
            summary: format!(
                "the public signature of `{}` in the policy component `{component}` names infrastructure: {shown}",
                s.label
            ),
            detail: format!(
                "`{}` is part of the public API of `{component}` (manifest {manifest}), and its signature names {shown}. A caller of policy code then depends on that technology even when no `use` of it appears in the caller. Translate at the boundary so the signature holds a type the policy owns, or, for a deliberate exception (for example a value type that only derives an annotation), allowlist the subject `{component}:{}` with a reason. Read from resolved `use` paths; a glob import, a macro-generated item and a type reached through another crate's alias are not seen.",
                s.label, s.label
            ),
            member: None,
            positive_control: None,
        });
    }
    out
}

#[cfg(test)]
mod tests;
