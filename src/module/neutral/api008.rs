// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-008: a public signature names a type from another library (Rust).
//!
//! **What this judges (Rust, from the source and the manifest):** a package is read when it has a library root (`src/lib.rs`). Its
//! dependencies are the keys of `[dependencies]` (and of `[target.*.dependencies]`) in its `Cargo.toml`, with a hyphen read as an underscore,
//! minus the ones that are **path dependencies** (a sibling crate of the same project, which is not a third party), also through
//! `workspace = true` (the root `[workspace.dependencies]` entry is looked up for a `path`). Build and dev dependencies are not read.
//!
//! In every tracked `.rs` file of the package outside vendored directories, tests, examples, benches, `build.rs`, `src/main.rs` and
//! `src/bin/`, and outside `#[cfg(test)]` items, a **public** item (`pub`, not `pub(crate)` or narrower) is read for the types it names:
//!
//! - a free function's parameters and return type, and the same for the methods of an inherent `impl` and of a `pub trait`
//!   (the methods of `impl Trait for Type` are not read: their signature is the trait's);
//! - the fields of a `pub struct` and the fields of the variants of a `pub enum`;
//! - the aliased type of a `pub type`, the type of a `pub const` and `pub static`.
//!
//! A type *names a dependency* when a path in it (through generic arguments, references, slices, tuples, `dyn` and `impl` bounds,
//! where-clauses and function-pointer types) starts with the dependency's name (`serde_json::Value`), or starts with an identifier that a
//! `use` of the same file imports from it (`use serde_json::Value;` then `Value`, also under `as` and inside `{}` groups), when the file
//! defines no type of that name. One **Low**-severity, Medium-confidence finding per package and dependency, at the first site, with
//! the number of sites.
//!
//! **Not read:** `pub use` re-exports (a deliberate exposure, and the way to declare one); a glob import (`use dep::*;` names nothing
//! that can be resolved); types in macro arguments; an item that is `pub` but unreachable from the crate root (a `pub` item in a private
//! module: the rule has no module reachability here, so it reads it); a dependency under another name than its key (`package = ".."` is
//! read by its key). Other languages. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note, Package};
use super::{join, RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;
use syn::{
    FnArg, GenericParam, ImplItem, Item, ItemFn, ItemImpl, ItemMod, ItemTrait, ReturnType,
    TraitItem, Type, TypeParamBound, UseTree, Visibility,
};

const ID: &str = crate::rule_id::API_008;

/// One place where a public item names a dependency's type.
#[derive(Debug, Clone)]
struct Site {
    path: String,
    line: u32,
    item: String,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn is_binary_source(path: &str) -> bool {
    path == "src/main.rs"
        || path.ends_with("/src/main.rs")
        || path.starts_with("src/bin/")
        || path.contains("/src/bin/")
}

fn is_public(v: &Visibility) -> bool {
    matches!(v, Visibility::Public(_))
}

/// The third-party dependency names (underscored) of the manifest text, and whether the manifest parsed.
fn dependencies(manifest: &str, root_manifest: Option<&str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(doc) = manifest.parse::<toml::Table>() else {
        return out;
    };
    let ws: Option<toml::Table> = root_manifest
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|d| {
            d.get("workspace")
                .and_then(|w| w.get("dependencies"))
                .and_then(|d| d.as_table())
                .cloned()
        });
    let mut tables: Vec<&toml::Table> = Vec::new();
    if let Some(t) = doc.get("dependencies").and_then(|d| d.as_table()) {
        tables.push(t);
    }
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for (_, v) in targets {
            if let Some(t) = v.get("dependencies").and_then(|d| d.as_table()) {
                tables.push(t);
            }
        }
    }
    for t in tables {
        for (name, spec) in t {
            let mut is_path = spec.get("path").is_some();
            if spec.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                is_path = ws
                    .as_ref()
                    .and_then(|w| w.get(name))
                    .is_some_and(|e| e.get("path").is_some());
            }
            if !is_path {
                out.insert(name.replace('-', "_"));
            }
        }
    }
    out
}

/// `use` imports of a file: the name an import introduces, and the first segment it comes from.
fn collect_uses(tree: &UseTree, root: Option<String>, out: &mut BTreeMap<String, String>) {
    match tree {
        UseTree::Path(p) => {
            let r = root.unwrap_or_else(|| p.ident.to_string());
            collect_uses(&p.tree, Some(r), out);
        }
        UseTree::Name(n) => {
            let name = n.ident.to_string();
            match root {
                Some(r) => {
                    let key = if name == "self" { r.clone() } else { name };
                    out.insert(key, r);
                }
                None => {
                    out.insert(name.clone(), name);
                }
            }
        }
        UseTree::Rename(rn) => {
            let r = root.unwrap_or_else(|| rn.ident.to_string());
            out.insert(rn.rename.to_string(), r);
        }
        UseTree::Group(g) => {
            for t in &g.items {
                collect_uses(t, root.clone(), out);
            }
        }
        UseTree::Glob(_) => {}
    }
}

struct Scan<'a> {
    deps: &'a BTreeSet<String>,
    uses: BTreeMap<String, String>,
    local: BTreeSet<String>,
    path: String,
    in_test: bool,
    /// dependency -> sites
    found: BTreeMap<String, Vec<Site>>,
}

impl Scan<'_> {
    fn resolve(&self, first: &str, multi: bool, generics: &BTreeSet<String>) -> Option<String> {
        if generics.contains(first) {
            return None;
        }
        if multi && self.deps.contains(first) && !self.local.contains(first) {
            return Some(first.to_string());
        }
        if self.local.contains(first) {
            return None;
        }
        match self.uses.get(first) {
            Some(root) if self.deps.contains(root) => Some(root.clone()),
            _ => None,
        }
    }

    fn type_roots(&self, ty: &Type, generics: &BTreeSet<String>, out: &mut Vec<String>) {
        match ty {
            Type::Path(p) => {
                if let Some(q) = &p.qself {
                    self.type_roots(&q.ty, generics, out);
                }
                self.path_roots(&p.path, generics, out);
            }
            Type::Reference(r) => self.type_roots(&r.elem, generics, out),
            Type::Ptr(r) => self.type_roots(&r.elem, generics, out),
            Type::Slice(s) => self.type_roots(&s.elem, generics, out),
            Type::Array(a) => self.type_roots(&a.elem, generics, out),
            Type::Paren(p) => self.type_roots(&p.elem, generics, out),
            Type::Group(g) => self.type_roots(&g.elem, generics, out),
            Type::Tuple(t) => {
                for e in &t.elems {
                    self.type_roots(e, generics, out);
                }
            }
            Type::TraitObject(t) => self.bounds(&t.bounds, generics, out),
            Type::ImplTrait(t) => self.bounds(&t.bounds, generics, out),
            Type::FnPtr(f) => {
                for a in &f.inputs {
                    self.type_roots(&a.ty, generics, out);
                }
                if let ReturnType::Type(_, t) = &f.output {
                    self.type_roots(t, generics, out);
                }
            }
            _ => {}
        }
    }

    fn bounds(
        &self,
        bounds: &syn::punctuated::Punctuated<TypeParamBound, syn::Token![+]>,
        generics: &BTreeSet<String>,
        out: &mut Vec<String>,
    ) {
        for b in bounds {
            if let TypeParamBound::Trait(t) = b {
                self.path_roots(&t.path, generics, out);
            }
        }
    }

    fn path_roots(&self, path: &syn::Path, generics: &BTreeSet<String>, out: &mut Vec<String>) {
        if let Some(first) = path.segments.first() {
            if let Some(dep) =
                self.resolve(&first.ident.to_string(), path.segments.len() > 1, generics)
            {
                out.push(dep);
            }
        }
        for seg in &path.segments {
            if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                for arg in &a.args {
                    if let syn::GenericArgument::Type(t) = arg {
                        self.type_roots(t, generics, out);
                    }
                }
            }
            if let syn::PathArguments::Parenthesized(p) = &seg.arguments {
                for t in &p.inputs {
                    self.type_roots(&t.ty, generics, out);
                }
                if let ReturnType::Type(_, t) = &p.output {
                    self.type_roots(t, generics, out);
                }
            }
        }
    }

    fn generic_names(g: &syn::Generics, into: &mut BTreeSet<String>) {
        for p in &g.params {
            match p {
                GenericParam::Type(t) => {
                    into.insert(t.ident.to_string());
                }
                GenericParam::Const(c) => {
                    into.insert(c.ident.to_string());
                }
                GenericParam::Lifetime(_) => {}
            }
        }
    }

    /// Bounds on the generics of an item count as part of its public signature.
    fn generics_roots(&self, g: &syn::Generics, names: &BTreeSet<String>, out: &mut Vec<String>) {
        for p in &g.params {
            if let GenericParam::Type(t) = p {
                self.bounds(&t.bounds, names, out);
            }
        }
        if let Some(w) = &g.where_clause {
            for pred in &w.predicates {
                if let syn::WherePredicate::Type(t) = pred {
                    self.type_roots(&t.bounded_ty, names, out);
                    self.bounds(&t.bounds, names, out);
                }
            }
        }
    }

    fn record(&mut self, roots: Vec<String>, line: u32, item: String) {
        let mut seen = BTreeSet::new();
        for dep in roots {
            if seen.insert(dep.clone()) {
                self.found.entry(dep).or_default().push(Site {
                    path: self.path.clone(),
                    line,
                    item: item.clone(),
                });
            }
        }
    }

    fn signature(&mut self, sig: &syn::Signature, outer: &BTreeSet<String>, name: &str) {
        let mut names = outer.clone();
        Self::generic_names(&sig.generics, &mut names);
        let mut roots = Vec::new();
        for a in &sig.inputs {
            if let FnArg::Typed(t) = a {
                self.type_roots(&t.ty, &names, &mut roots);
            }
        }
        if let ReturnType::Type(_, t) = &sig.output {
            self.type_roots(t, &names, &mut roots);
        }
        self.generics_roots(&sig.generics, &names, &mut roots);
        let line = line_of(sig.ident.span());
        self.record(roots, line, format!("fn {name}"));
    }
}

fn simple_self_name(ty: &Type) -> String {
    match ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, i: &'ast ItemMod) {
        let before = self.in_test;
        if is_cfg_test(&i.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, i);
        self.in_test = before;
    }

    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        if !self.in_test && !is_cfg_test(&i.attrs) && is_public(&i.vis) {
            let name = i.sig.ident.to_string();
            self.signature(&i.sig, &BTreeSet::new(), &name);
        }
        // nested items are private
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if self.in_test || is_cfg_test(&i.attrs) || i.trait_.is_some() {
            return;
        }
        let mut outer = BTreeSet::new();
        Self::generic_names(&i.generics, &mut outer);
        let owner = simple_self_name(&i.self_ty);
        for it in &i.items {
            if let ImplItem::Fn(f) = it {
                if is_public(&f.vis) && !is_cfg_test(&f.attrs) {
                    let name = format!("{owner}::{}", f.sig.ident);
                    self.signature(&f.sig, &outer, &name);
                }
            }
        }
    }

    fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut outer = BTreeSet::new();
        Self::generic_names(&i.generics, &mut outer);
        for it in &i.items {
            if let TraitItem::Fn(f) = it {
                let name = format!("{}::{}", i.ident, f.sig.ident);
                self.signature(&f.sig, &outer, &name);
            }
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut names = BTreeSet::new();
        Self::generic_names(&i.generics, &mut names);
        let mut roots = Vec::new();
        for f in &i.fields {
            if is_public(&f.vis) {
                self.type_roots(&f.ty, &names, &mut roots);
            }
        }
        self.record(
            roots,
            line_of(i.ident.span()),
            format!("struct {}", i.ident),
        );
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut names = BTreeSet::new();
        Self::generic_names(&i.generics, &mut names);
        let mut roots = Vec::new();
        for v in &i.variants {
            for f in &v.fields {
                self.type_roots(&f.ty, &names, &mut roots);
            }
        }
        self.record(roots, line_of(i.ident.span()), format!("enum {}", i.ident));
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut names = BTreeSet::new();
        Self::generic_names(&i.generics, &mut names);
        let mut roots = Vec::new();
        self.type_roots(&i.ty, &names, &mut roots);
        self.record(roots, line_of(i.ident.span()), format!("type {}", i.ident));
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut roots = Vec::new();
        self.type_roots(&i.ty, &BTreeSet::new(), &mut roots);
        self.record(roots, line_of(i.ident.span()), format!("const {}", i.ident));
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        if self.in_test || is_cfg_test(&i.attrs) || !is_public(&i.vis) {
            return;
        }
        let mut roots = Vec::new();
        self.type_roots(&i.ty, &BTreeSet::new(), &mut roots);
        self.record(
            roots,
            line_of(i.ident.span()),
            format!("static {}", i.ident),
        );
    }
}

/// The names a file defines itself (so an imported-looking name that is also defined here is not read as the import).
fn local_names(file: &syn::File) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    fn walk(items: &[Item], out: &mut BTreeSet<String>) {
        for it in items {
            match it {
                Item::Struct(x) => {
                    out.insert(x.ident.to_string());
                }
                Item::Enum(x) => {
                    out.insert(x.ident.to_string());
                }
                Item::Union(x) => {
                    out.insert(x.ident.to_string());
                }
                Item::Type(x) => {
                    out.insert(x.ident.to_string());
                }
                Item::Trait(x) => {
                    out.insert(x.ident.to_string());
                }
                Item::Mod(m) => {
                    out.insert(m.ident.to_string());
                    if let Some((_, items)) = &m.content {
                        walk(items, out);
                    }
                }
                _ => {}
            }
        }
    }
    walk(&file.items, &mut out);
    out
}

fn uses_of(file: &syn::File) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    fn walk(items: &[Item], out: &mut BTreeMap<String, String>) {
        for it in items {
            match it {
                Item::Use(u) => {
                    collect_uses(&u.tree, None, out);
                }
                Item::Mod(m) => {
                    if let Some((_, items)) = &m.content {
                        walk(items, out);
                    }
                }
                _ => {}
            }
        }
    }
    walk(&file.items, &mut out);
    out
}

fn has_library_root(p: &Package) -> bool {
    p.sources
        .iter()
        .any(|s| s.path == "src/lib.rs" || s.path.ends_with("/src/lib.rs"))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    let mut findings = Vec::new();
    let mut libs = 0usize;
    let root_manifest = view.read("Cargo.toml");
    let members: BTreeSet<String> = view
        .paths()
        .iter()
        .filter(|p| p.as_str() == "Cargo.toml" || p.ends_with("/Cargo.toml"))
        .filter_map(|p| view.read(p))
        .filter_map(|t| t.parse::<toml::Table>().ok())
        .filter_map(|d| {
            d.get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .map(|n| n.replace('-', "_"))
        })
        .collect();
    for p in &loaded.packages {
        if !has_library_root(p) {
            continue;
        }
        libs += 1;
        let Some(manifest) = view.read(&join(&p.dir, "Cargo.toml")) else {
            continue;
        };
        let mut deps = dependencies(&manifest, root_manifest.as_deref());
        // A package of the same repository is the project's own, whether it is a path or a registry dependency.
        for m in &members {
            deps.remove(m);
        }
        if deps.is_empty() {
            continue;
        }
        let mut all: BTreeMap<String, Vec<Site>> = BTreeMap::new();
        for s in &p.sources {
            if is_binary_source(&s.path) {
                continue;
            }
            let mut scan = Scan {
                deps: &deps,
                uses: uses_of(&s.file),
                local: local_names(&s.file),
                path: s.path.clone(),
                in_test: false,
                found: BTreeMap::new(),
            };
            scan.visit_file(&s.file);
            for (dep, sites) in scan.found {
                all.entry(dep).or_default().extend(sites);
            }
        }
        let files = p.sources.len();
        let subject_pkg = if p.dir.is_empty() {
            view.name().to_string()
        } else {
            p.dir.clone()
        };
        for (dep, mut sites) in all {
            sites.sort_by(|a, b| (a.path.as_str(), a.line).cmp(&(b.path.as_str(), b.line)));
            let first = sites[0].clone();
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!(
                        "the public API of `{subject_pkg}` names types from the dependency `{dep}` in {} place(s); first: `{}`",
                        sites.len(),
                        first.item
                    ),
                    "A public signature that names another library's type makes that library's breaking change yours. Wrap the type in one of your own, or keep it as a declared exception (an allowlist entry for this finding, with the reason).",
                )
                .location(Location::new(first.path.as_str(), Some(first.line)))
                .subject(format!("{subject_pkg}:{dep}"))
                .positive_control(format!(
                    "{files} Rust source file(s) of the package were read for the same paths, against {} third-party dependency name(s) of its manifest",
                    deps.len()
                )),
            );
        }
    }
    if libs == 0 {
        return Verdict::NotApplicable(
            "the repository tracks no Rust library package (no src/lib.rs outside tests, examples and vendored trees)".into(),
        );
    }
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde_json = \"1\"\nanyhow = \"1\"\nsibling = { path = \"../sibling\" }\n";

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_path_and_an_imported_name_in_a_public_signature_are_reported() {
        let f = found("pub fn a() -> serde_json::Value { todo!() }\n");
        assert_eq!(f.len(), 1);
        let f = found("use serde_json::Value;\npub fn a(v: Value) {}\n");
        assert_eq!(f.len(), 1);
        let f = found("use serde_json::{Map, Value as V};\npub fn a(v: V) {}\n");
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn one_finding_per_dependency_counts_the_sites() {
        let f = found("pub fn a() -> serde_json::Value { todo!() }\npub fn b(v: &serde_json::Value) {}\npub fn c() -> anyhow::Result<()> { Ok(()) }\n");
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|x| x.summary.contains("2 place(s)")));
    }

    #[test]
    fn fields_variants_aliases_and_bounds_are_read() {
        assert_eq!(
            found("pub struct S { pub v: serde_json::Value }\n").len(),
            1
        );
        assert_eq!(found("pub enum E { A(anyhow::Error) }\n").len(), 1);
        assert_eq!(found("pub type T = Vec<serde_json::Value>;\n").len(), 1);
        assert_eq!(
            found("pub fn f<T: serde_json::de::Read>(t: T) {}\n").len(),
            1
        );
    }

    #[test]
    fn private_items_re_exports_trait_impls_path_deps_and_test_code_are_not_read() {
        assert!(found("fn a() -> serde_json::Value { todo!() }\n").is_empty());
        assert!(found("pub(crate) fn a() -> serde_json::Value { todo!() }\n").is_empty());
        assert!(found("pub use serde_json::Value;\n").is_empty());
        assert!(found("pub struct S { v: serde_json::Value }\n").is_empty());
        assert!(found("pub struct S;\nimpl std::fmt::Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }\nimpl From<serde_json::Value> for S { fn from(_: serde_json::Value) -> S { S } }\n").is_empty());
        assert!(found("pub fn a(s: sibling::Thing) {}\n").is_empty());
        assert!(
            found("#[cfg(test)]\nmod t { pub fn a() -> serde_json::Value { todo!() } }\n")
                .is_empty()
        );
    }

    #[test]
    fn a_locally_defined_name_and_a_generic_parameter_are_not_the_import() {
        assert!(
            found("use serde_json::Value;\npub struct Value;\npub fn a(v: Value) {}\n").is_empty()
        );
        assert!(found("pub fn a<Value>(v: Value) {}\n").is_empty());
    }

    #[test]
    fn a_package_of_the_same_repository_is_not_a_third_party() {
        let v = judge(&RepoView::from_files(&[
            (
                "Cargo.toml",
                "[package]
name = \"p\"
version = \"0.1.0\"

[dependencies]
sibling-wire = \"0.1\"
",
            ),
            (
                "src/lib.rs",
                "pub fn a(s: sibling_wire::Frame) {}
",
            ),
            (
                "crates/wire/Cargo.toml",
                "[package]
name = \"sibling-wire\"
version = \"0.1.0\"
",
            ),
            (
                "crates/wire/src/lib.rs",
                "pub struct Frame;
",
            ),
        ]));
        assert!(matches!(&v, Verdict::Findings(f) if f.is_empty()));
    }

    #[test]
    fn a_binary_only_package_is_not_applicable() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/main.rs", "fn main() {}"),
        ]));
        assert!(matches!(v, Verdict::NotApplicable(_)));
    }
}
