// SPDX-License-Identifier: MIT OR Apache-2.0
//! Component API within budget (repository scope, Rust): does a component offer the outside no more public items than the project
//! budgeted for it, and does it re-export no type owned by someone else's library?
//!
//! Rule record: `ARC-006` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; background: `docs/rules/arc.md`.
//!
//! **What it counts.** For each component with an entry in `[architecture.api_budget]`, in the library (`src/lib.rs`) of each of its
//! crates at `HEAD`, from the tracked `.rs` files under `src/` (test items skipped): the *items* a consumer can name, and the *members*
//! of the public types.
//! - **Items:** a `pub` function, struct, enum, union, trait, type alias, constant, static or module, a `#[macro_export]` macro, and each
//!   leaf of a `pub use` (a glob counts once). A declaration is counted only when every module between it and the crate root is `pub`
//!   (a `mod` that is not `pub` hides what is inside it; `pub(crate)`, `pub(super)` and `pub(in ..)` are not public). A `pub use` leaf
//!   that only repeats the name of a counted declaration is not counted twice.
//! - **Members:** the `pub` fields of a public struct, the variants of a public enum, the items of a public trait and the `pub`
//!   associated items of its inherent `impl` blocks, wherever in the crate those are written (a type declared in a private module and
//!   re-exported with `pub use` still has its members counted).
//!
//! The budget is compared with items plus members; the finding states both, so a narrow surface of wide types is not hidden by the sum.
//!
//! **Third-party re-exports.** A `pub use` whose first path segment names neither `crate`, `self`, `super`, `std`, `core`, `alloc`, a
//! workspace member, nor anything the crate itself declares is a re-export of a library the crate does not own: reported at Info, one
//! finding per component, with the paths.
//!
//! **Weak evidence by construction (medium confidence).** The count is syntactic: macro-generated items, `#[path]` modules and
//! `cfg`-gated items are not resolved (a gated item is counted as if on), two types of the same name in different modules share their
//! members, a glob re-export is a lower bound, and a third-party type that appears in a public *signature* (not in a `pub use`) is NOT
//! SEEN (that is API-008). Low severity over budget, Info for re-exports.
//!
//! **No declaration, or no budget, is not clean:** the rule reports "not run" with what to add ([`Check::not_configured`]).
//!
//! **Known limitations / deliberate scope:**
//! - Crate granularity (components are crate directories, as in ARC-001); a component of module paths inside a crate is noted, not judged.
//! - A library whose root is not `src/lib.rs`, and a binary-only crate, are noted and count zero.
//! - A budget entry that names no declared component is reported (Info): a stale declaration passes forever.

use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_006;

/// Reports a component whose public API is over its declared budget or re-exports third-party types (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ApiBudgetCheck;

impl ApiBudgetCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-006 has no API budget to compare; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into());
    };
    if a.api_budget.is_empty() {
        return Some("the [architecture] table has no [architecture.api_budget] entries (a component name and the most public items it may offer), so there is no budget to judge (docs/rules/arc.md)".into());
    }
    None
}

impl Check for ApiBudgetCheck {
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
        let member_idents: BTreeSet<String> = graph
            .members
            .iter()
            .map(|m| m.name.replace('-', "_"))
            .collect();
        let mut tallies = Vec::new();
        let mut notes = Vec::new();
        for (cname, budget) in &arch.api_budget {
            let Some(c) = arch.component.iter().find(|c| &c.name == cname) else {
                tallies.push(Tally::Stale(cname.clone()));
                continue;
            };
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
            let mut totals = Totals::default();
            let mut manifest = None;
            let mut crates = 0usize;
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                crates += 1;
                manifest.get_or_insert_with(|| m.manifest.clone());
                let files = read_sources(&ctx.project_root.join(&m.dir))?;
                let (t, n) = surface(&files, &m.dir, &member_idents)?;
                totals.merge(t);
                notes.extend(n.into_iter().map(|s| format!("crate `{}`: {s}", m.name)));
            }
            if crates == 0 {
                notes.push(format!(
                    "component `{}` matches no workspace member (ARC-001 reports the stale declaration); nothing to count",
                    c.name
                ));
                continue;
            }
            tallies.push(Tally::Counted {
                component: c.name.clone(),
                budget: *budget,
                manifest,
                totals,
            });
        }
        Ok(judge(tallies, &project, notes))
    }
}

/// What the public surface of one library (or a component's libraries together) comes to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Totals {
    /// Public items by kind.
    pub items: BTreeMap<&'static str, usize>,
    /// Members of the public types (fields, variants, trait items, inherent associated items).
    pub members: usize,
    /// `pub use` leaves whose root is a library the crate does not own: `(path, file)`.
    pub third_party: Vec<(String, String)>,
    /// Glob re-exports, each counted as one item and a lower bound.
    pub globs: usize,
    /// Public declarations and public inherent functions of public types that carry documentation.
    pub documented: usize,
    /// The ones that do not (a `#[doc(hidden)]` item is in neither list).
    pub undocumented: Vec<Undoc>,
    /// `(file, line)` of each documented one, for a reader that asks "is the item at this line public and documented?".
    pub documented_sites: std::collections::BTreeSet<(String, usize)>,
}

/// A public item with no documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Undoc {
    pub name: String,
    pub kind: &'static str,
    /// Relative to the repository when the crate directory was given, else to the crate.
    pub file: String,
    pub line: usize,
}

impl Totals {
    pub(crate) fn item_count(&self) -> usize {
        self.items.values().sum()
    }

    fn merge(&mut self, o: Totals) {
        for (k, v) in o.items {
            *self.items.entry(k).or_default() += v;
        }
        self.members += o.members;
        self.third_party.extend(o.third_party);
        self.globs += o.globs;
        self.documented += o.documented;
        self.undocumented.extend(o.undocumented);
        self.documented_sites.extend(o.documented_sites);
    }
}

/// A leaf of a `pub use`.
struct Leaf {
    /// `serde::Serialize`, `a::b::*`.
    path: String,
    /// The name it introduces (`*` for a glob).
    name: String,
    first: String,
    glob: bool,
    file: String,
}

#[derive(Default)]
struct Collect {
    decls: Vec<(String, &'static str)>,
    leaves: Vec<Leaf>,
    /// Members per type name, from every definition and inherent `impl` anywhere in the crate.
    types: BTreeMap<String, usize>,
    /// Names of the types declared public (reachable) by a declaration.
    public_types: BTreeSet<String>,
    /// Every name the crate declares, for telling a local path root from a library.
    local: BTreeSet<String>,
    notes: Vec<String>,
    /// Documented public declarations.
    documented: usize,
    documented_sites: Vec<(String, usize)>,
    undocumented: Vec<Undoc>,
    /// Public inherent functions per type name: `(function, documented, hidden, file, line)`.
    methods: BTreeMap<String, Vec<MethodFact>>,
}

/// A public inherent function of a type.
struct MethodFact {
    name: String,
    documented: bool,
    hidden: bool,
    file: String,
    line: usize,
}

fn is_hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("doc") && {
            let mut hidden = false;
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("hidden") {
                    hidden = true;
                }
                Ok(())
            });
            hidden
        }
    })
}

/// Whether the attributes carry documentation: `///`, `//!`-style `#[doc = ..]`, or `#[doc(..)]` other than `hidden`.
fn is_documented(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("doc")
            && !matches!(&a.meta, syn::Meta::List(l) if l.tokens.to_string().trim() == "hidden")
    })
}

/// The public surface of the library whose tracked sources are `files` (paths relative to the crate directory `dir`). A file `syn`
/// cannot parse is an error, never a smaller count. Returns the totals and notes.
pub(crate) fn surface(
    files: &[SourceFile],
    dir: &str,
    members: &BTreeSet<String>,
) -> anyhow::Result<(Totals, Vec<String>)> {
    let by_path: BTreeMap<&str, &SourceFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let Some(root) = by_path.get("src/lib.rs") else {
        let note = if by_path.contains_key("src/main.rs") {
            "a binary crate with no src/lib.rs offers no library API; counted as zero".to_string()
        } else {
            "no src/lib.rs among the tracked sources; counted as zero".to_string()
        };
        return Ok((Totals::default(), vec![note]));
    };
    let mut w = Walker {
        by_path: &by_path,
        dir,
        col: Collect::default(),
        seen: BTreeSet::new(),
    };
    w.walk_file(root, "src", true)?;
    let col = w.col;
    let mut totals = Totals::default();
    for (_, kind) in &col.decls {
        *totals.items.entry(kind).or_default() += 1;
    }
    let declared: BTreeSet<&str> = col.decls.iter().map(|(n, _)| n.as_str()).collect();
    let mut public_types = col.public_types.clone();
    for l in &col.leaves {
        if l.glob {
            totals.globs += 1;
            *totals.items.entry("re-export").or_default() += 1;
        } else if !declared.contains(l.name.as_str()) {
            *totals.items.entry("re-export").or_default() += 1;
        }
        if !l.glob {
            public_types.insert(l.name.clone());
        }
        let owned = matches!(
            l.first.as_str(),
            "crate" | "self" | "super" | "std" | "core" | "alloc"
        ) || members.contains(&l.first)
            || col.local.contains(&l.first);
        if !owned {
            totals.third_party.push((l.path.clone(), l.file.clone()));
        }
    }
    totals.members = public_types.iter().filter_map(|n| col.types.get(n)).sum();
    totals.documented = col.documented;
    totals.documented_sites = col.documented_sites.iter().cloned().collect();
    totals.undocumented = col.undocumented.clone();
    for n in &col.public_types {
        for m in col.methods.get(n).into_iter().flatten() {
            if m.hidden {
                continue;
            }
            if m.documented {
                totals.documented += 1;
                totals.documented_sites.insert((m.file.clone(), m.line));
            } else {
                totals.undocumented.push(Undoc {
                    name: format!("{n}::{}", m.name),
                    kind: "method",
                    file: m.file.clone(),
                    line: m.line,
                });
            }
        }
    }
    totals
        .undocumented
        .sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
    totals.third_party.sort();
    totals.third_party.dedup();
    Ok((totals, col.notes))
}

fn is_pub(v: &syn::Visibility) -> bool {
    matches!(v, syn::Visibility::Public(_))
}

pub(super) fn attrs_of(i: &syn::Item) -> &[syn::Attribute] {
    match i {
        syn::Item::Fn(x) => &x.attrs,
        syn::Item::Mod(x) => &x.attrs,
        syn::Item::Impl(x) => &x.attrs,
        syn::Item::Struct(x) => &x.attrs,
        syn::Item::Enum(x) => &x.attrs,
        syn::Item::Union(x) => &x.attrs,
        syn::Item::Use(x) => &x.attrs,
        syn::Item::Const(x) => &x.attrs,
        syn::Item::Static(x) => &x.attrs,
        syn::Item::Trait(x) => &x.attrs,
        syn::Item::Type(x) => &x.attrs,
        syn::Item::Macro(x) => &x.attrs,
        _ => &[],
    }
}

/// What a walk over one crate's sources reads and collects.
struct Walker<'a> {
    by_path: &'a BTreeMap<&'a str, &'a SourceFile>,
    /// The crate directory, for the paths shown in findings.
    dir: &'a str,
    col: Collect,
    seen: BTreeSet<String>,
}

impl Walker<'_> {
    /// Whether the file of the out-of-line module `name` (declared in a file whose children live in `child_dir`) starts with documentation
    /// (`//!` or `#![doc = ..]`); a `#[doc(hidden)]` module is not asked.
    fn file_module_has_inner_docs(&self, child_dir: &str, name: &str) -> bool {
        let flat = format!("{child_dir}/{name}.rs");
        let dir_mod = format!("{child_dir}/{name}/mod.rs");
        let by_path = self.by_path;
        let Some(f) = by_path
            .get(flat.as_str())
            .or_else(|| by_path.get(dir_mod.as_str()))
        else {
            return false;
        };
        syn::parse_file(&f.text).is_ok_and(|p| is_documented(&p.attrs))
    }

    /// Records whether a public declaration is documented (a `#[doc(hidden)]` one is not recorded at all).
    fn doc(
        &mut self,
        attrs: &[syn::Attribute],
        name: &str,
        kind: &'static str,
        line: usize,
        file: &str,
    ) {
        if is_hidden(attrs) {
            return;
        }
        if is_documented(attrs) {
            self.col.documented += 1;
            self.col.documented_sites.push((file.to_string(), line));
        } else {
            self.col.undocumented.push(Undoc {
                name: name.to_string(),
                kind,
                file: file.to_string(),
                line,
            });
        }
    }

    /// Parses `file` and walks its items. `child_dir` is where this file's `mod x;` children live.
    fn walk_file(
        &mut self,
        file: &SourceFile,
        child_dir: &str,
        reachable: bool,
    ) -> anyhow::Result<()> {
        if !self.seen.insert(file.path.clone()) {
            return Ok(());
        }
        let parsed = syn::parse_file(&file.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                file.path
            )
        })?;
        let shown = if self.dir.is_empty() {
            file.path.clone()
        } else {
            format!("{}/{}", self.dir, file.path)
        };
        self.walk_items(&parsed.items, child_dir, &shown, reachable)
    }

    fn walk_items(
        &mut self,
        items: &[syn::Item],
        child_dir: &str,
        shown: &str,
        reachable: bool,
    ) -> anyhow::Result<()> {
        for item in items {
            if is_cfg_test(attrs_of(item)) {
                continue;
            }
            match item {
                syn::Item::Fn(f) => {
                    self.col.local.insert(f.sig.ident.to_string());
                    if reachable && is_pub(&f.vis) {
                        self.col.decls.push((f.sig.ident.to_string(), "fn"));
                        let n = f.sig.ident.to_string();
                        self.doc(&f.attrs, &n, "fn", f.sig.ident.span().start().line, shown);
                    }
                }
                syn::Item::Struct(s) => {
                    let n = s.ident.to_string();
                    self.col.local.insert(n.clone());
                    let fields = s.fields.iter().filter(|f| is_pub(&f.vis)).count();
                    *self.col.types.entry(n.clone()).or_default() += fields;
                    if reachable && is_pub(&s.vis) {
                        self.col.decls.push((n.clone(), "struct"));
                        self.doc(&s.attrs, &n, "struct", s.ident.span().start().line, shown);
                        self.col.public_types.insert(n);
                    }
                }
                syn::Item::Union(s) => {
                    let n = s.ident.to_string();
                    self.col.local.insert(n.clone());
                    let fields = s.fields.named.iter().filter(|f| is_pub(&f.vis)).count();
                    *self.col.types.entry(n.clone()).or_default() += fields;
                    if reachable && is_pub(&s.vis) {
                        self.col.decls.push((n.clone(), "union"));
                        self.doc(&s.attrs, &n, "union", s.ident.span().start().line, shown);
                        self.col.public_types.insert(n);
                    }
                }
                syn::Item::Enum(e) => {
                    let n = e.ident.to_string();
                    self.col.local.insert(n.clone());
                    *self.col.types.entry(n.clone()).or_default() += e.variants.len();
                    if reachable && is_pub(&e.vis) {
                        self.col.decls.push((n.clone(), "enum"));
                        self.doc(&e.attrs, &n, "enum", e.ident.span().start().line, shown);
                        self.col.public_types.insert(n);
                    }
                }
                syn::Item::Trait(t) => {
                    let n = t.ident.to_string();
                    self.col.local.insert(n.clone());
                    *self.col.types.entry(n.clone()).or_default() += t.items.len();
                    if reachable && is_pub(&t.vis) {
                        self.col.decls.push((n.clone(), "trait"));
                        self.doc(&t.attrs, &n, "trait", t.ident.span().start().line, shown);
                        self.col.public_types.insert(n);
                    }
                }
                syn::Item::Type(t) => {
                    self.col.local.insert(t.ident.to_string());
                    if reachable && is_pub(&t.vis) {
                        self.col.decls.push((t.ident.to_string(), "type alias"));
                        let n = t.ident.to_string();
                        self.doc(
                            &t.attrs,
                            &n,
                            "type alias",
                            t.ident.span().start().line,
                            shown,
                        );
                        self.col.public_types.insert(t.ident.to_string());
                    }
                }
                syn::Item::Const(c) => {
                    self.col.local.insert(c.ident.to_string());
                    if reachable && is_pub(&c.vis) {
                        self.col.decls.push((c.ident.to_string(), "const"));
                        let n = c.ident.to_string();
                        self.doc(&c.attrs, &n, "const", c.ident.span().start().line, shown);
                    }
                }
                syn::Item::Static(c) => {
                    self.col.local.insert(c.ident.to_string());
                    if reachable && is_pub(&c.vis) {
                        self.col.decls.push((c.ident.to_string(), "static"));
                        let n = c.ident.to_string();
                        self.doc(&c.attrs, &n, "static", c.ident.span().start().line, shown);
                    }
                }
                syn::Item::Macro(m) => {
                    if reachable
                        && m.mac.path.is_ident("macro_rules")
                        && m.attrs.iter().any(|a| a.path().is_ident("macro_export"))
                    {
                        if let Some(id) = &m.ident {
                            self.col.decls.push((id.to_string(), "macro"));
                            self.doc(
                                &m.attrs,
                                &id.to_string(),
                                "macro",
                                id.span().start().line,
                                shown,
                            );
                        }
                    }
                }
                syn::Item::Impl(im) => {
                    // inherent impls only: a trait impl's items are not `pub` and are not budgeted
                    if im.trait_.is_none() {
                        if let syn::Type::Path(p) = &*im.self_ty {
                            if let Some(last) = p.path.segments.last() {
                                let n = im
                                    .items
                                    .iter()
                                    .filter(|i| match i {
                                        syn::ImplItem::Fn(f) => is_pub(&f.vis),
                                        syn::ImplItem::Const(c) => is_pub(&c.vis),
                                        syn::ImplItem::Type(t) => is_pub(&t.vis),
                                        _ => false,
                                    })
                                    .count();
                                *self.col.types.entry(last.ident.to_string()).or_default() += n;
                                for it in &im.items {
                                    if let syn::ImplItem::Fn(f) = it {
                                        if is_pub(&f.vis) {
                                            self.col
                                                .methods
                                                .entry(last.ident.to_string())
                                                .or_default()
                                                .push(MethodFact {
                                                    name: f.sig.ident.to_string(),
                                                    documented: is_documented(&f.attrs),
                                                    hidden: is_hidden(&f.attrs),
                                                    file: shown.to_string(),
                                                    line: f.sig.ident.span().start().line,
                                                });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                syn::Item::Use(u) => {
                    if reachable && is_pub(&u.vis) {
                        let mut leaves = Vec::new();
                        flatten_use(
                            &u.tree,
                            &mut Vec::new(),
                            u.leading_colon.is_some(),
                            &mut leaves,
                        );
                        for (segs, name, glob, abs) in leaves {
                            let first = if abs {
                                // `pub use ::serde::X` can only name a library
                                format!("::{}", segs.first().cloned().unwrap_or_default())
                            } else {
                                segs.first().cloned().unwrap_or_default()
                            };
                            self.col.leaves.push(Leaf {
                                path: if abs {
                                    format!("::{}", segs.join("::"))
                                } else {
                                    segs.join("::")
                                },
                                name,
                                first,
                                glob,
                                file: shown.to_string(),
                            });
                        }
                    }
                }
                syn::Item::Mod(m) => {
                    let name = m.ident.to_string();
                    self.col.local.insert(name.clone());
                    let public = reachable && is_pub(&m.vis);
                    if public {
                        self.col.decls.push((name.clone(), "mod"));
                        // `pub mod x;` is documented by an outer comment or by the `//!` inside the module's own file
                        let inner = m.content.is_none()
                            && self.file_module_has_inner_docs(child_dir, &name);
                        let line = m.ident.span().start().line;
                        if inner {
                            self.col.documented += 1;
                            self.col.documented_sites.push((shown.to_string(), line));
                        } else {
                            self.doc(&m.attrs, &name, "mod", line, shown);
                        }
                    }
                    if m.attrs.iter().any(|a| a.path().is_ident("path")) {
                        self.col.notes.push(format!(
                            "module `{name}` uses #[path]; its contents are not read"
                        ));
                        continue;
                    }
                    if let Some((_, inner)) = &m.content {
                        let nested = format!("{child_dir}/{name}");
                        self.walk_items(inner, &nested, shown, public)?;
                    } else {
                        let flat = format!("{child_dir}/{name}.rs");
                        let nested = format!("{child_dir}/{name}/mod.rs");
                        let by_path = self.by_path;
                        if let Some(f) = by_path.get(flat.as_str()) {
                            self.walk_file(f, &format!("{child_dir}/{name}"), public)?;
                        } else if let Some(f) = by_path.get(nested.as_str()) {
                            self.walk_file(f, &format!("{child_dir}/{name}"), public)?;
                        } else {
                            self.col.notes.push(format!(
                            "module `{name}` has no tracked file {flat} or {nested}; its contents are not read"
                        ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// The leaves of a use tree as `(path segments, introduced name, is glob, starts with ::)`.
fn flatten_use(
    t: &syn::UseTree,
    prefix: &mut Vec<String>,
    abs: bool,
    out: &mut Vec<(Vec<String>, String, bool, bool)>,
) {
    match t {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten_use(&p.tree, prefix, abs, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut segs = prefix.clone();
            segs.push(n.ident.to_string());
            let name = if n.ident == "self" {
                prefix.last().cloned().unwrap_or_default()
            } else {
                n.ident.to_string()
            };
            out.push((segs, name, false, abs));
        }
        syn::UseTree::Rename(r) => {
            let mut segs = prefix.clone();
            segs.push(r.ident.to_string());
            out.push((segs, r.rename.to_string(), false, abs));
        }
        syn::UseTree::Glob(_) => {
            let mut segs = prefix.clone();
            segs.push("*".into());
            out.push((segs, "*".into(), true, abs));
        }
        syn::UseTree::Group(g) => {
            for i in &g.items {
                flatten_use(i, prefix, abs, out);
            }
        }
    }
}

/// One budgeted component, counted, or a budget entry that names no component.
pub(crate) enum Tally {
    Counted {
        component: String,
        budget: u32,
        manifest: Option<String>,
        totals: Totals,
    },
    Stale(String),
}

fn kinds(t: &Totals) -> String {
    t.items
        .iter()
        .map(|(k, n)| format!("{n} {k}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The findings and notes for the counted components.
pub(crate) fn judge(
    tallies: Vec<Tally>,
    project: &str,
    mut notes: Vec<String>,
) -> (Vec<Finding>, Vec<String>) {
    let mut findings = Vec::new();
    let mut ranking: Vec<(String, usize, u32)> = Vec::new();
    for t in tallies {
        match t {
            Tally::Stale(name) => findings.push(Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: Severity::Info,
                confidence: Confidence::High,
                project: project.into(),
                location: None,
                subject: Some(name.clone()),
                summary: format!(
                    "[architecture.api_budget] names `{name}`, which is not a declared component"
                ),
                detail: format!(
                    "The budget entry `{name}` has no [[architecture.component]] of that name, so nothing is counted against it and the ceiling it states protects nothing. Rename the entry to a declared component or remove it."
                ),
                member: None,
                positive_control: None,
            }),
            Tally::Counted {
                component,
                budget,
                manifest,
                totals,
            } => {
                let total = totals.item_count() + totals.members;
                ranking.push((component.clone(), total, budget));
                let location = manifest.map(|file| Location { file, line: None });
                if total > budget as usize {
                    findings.push(Finding {
                        check_id: CHECK_ID.into(),
                        claim: Claim::Presence,
                        severity: Severity::Low,
                        confidence: Confidence::Medium,
                        project: project.into(),
                        location: location.clone(),
                        subject: Some(component.clone()),
                        summary: format!(
                            "the component `{component}` offers {total} public items and members; its budget is {budget}"
                        ),
                        detail: format!(
                            "Counted in the library of `{component}`: {} public items ({}) and {} members of the public types (fields, variants, trait items and inherent associated items), {total} in all, against a declared budget of {budget}. A wide API ties consumers to the component's internals. Shrink it to what consumers need, make the rest `pub(crate)`, or raise the budget in [architecture.api_budget] with the reason. The count is syntactic: macro-generated items and #[path] modules are not seen, a `cfg`-gated item is counted as if enabled, and a glob re-export counts once ({} glob(s)).",
                            totals.item_count(),
                            kinds(&totals),
                            totals.members,
                            totals.globs
                        ),
                        member: None,
                        positive_control: None,
                    });
                }
                if !totals.third_party.is_empty() {
                    let shown = totals
                        .third_party
                        .iter()
                        .take(8)
                        .map(|(p, f)| format!("`{p}` ({f})"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    findings.push(Finding {
                        check_id: CHECK_ID.into(),
                        claim: Claim::Presence,
                        severity: Severity::Info,
                        confidence: Confidence::Medium,
                        project: project.into(),
                        location,
                        subject: Some(format!("{component}:third-party")),
                        summary: format!(
                            "the component `{component}` re-exports {} item(s) from libraries it does not own",
                            totals.third_party.len()
                        ),
                        detail: format!(
                            "`pub use` of a path rooted in a library the crate does not own makes that library's release schedule part of this API: {shown}{}. Wrap the type in one the component owns, or, if the dependency is the point, allowlist it with the subject `{component}:third-party` and a reason. A third-party type in a public signature rather than a `pub use` is not seen by this rule.",
                            if totals.third_party.len() > 8 { ", ..." } else { "" }
                        ),
                        member: None,
                        positive_control: None,
                    });
                }
            }
        }
    }
    findings.sort_by(|a, b| a.subject.cmp(&b.subject));
    ranking.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if !ranking.is_empty() {
        notes.push(format!(
            "ARC-006 budgeted components by public items and members against budget: {}",
            ranking
                .iter()
                .map(|(n, t, b)| format!("{n} {t}/{b}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    (findings, notes)
}

#[cfg(test)]
mod tests;
