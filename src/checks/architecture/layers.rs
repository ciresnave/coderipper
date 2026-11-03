// SPDX-License-Identifier: MIT OR Apache-2.0
//! Indirection that earns nothing (project scope, Rust): which layers of a package are only a shape, a trait with one implementation or a
//! type that only hands every call to the type inside it?
//!
//! Rule record: `ARC-008` in `rules/arc.toml`. Background: `docs/rules/arc.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.10: **shapes only**; whether a layer "earns nothing" is not
//! judged here.
//!
//! **Two shapes.**
//! - **A trait with one implementation and no test double.** A trait defined in the package, with at least one method, implemented
//!   (`impl Trait for X`) exactly once in the package's non-test code, where no implementation is written in test code (`#[cfg(test)]`
//!   items, test files) and no implementing type is named like a double (`Mock`, `Fake`, `Stub`, `Dummy`, `Spy`, `InMemory`, `Noop`, `Null`,
//!   `Test`). A trait with a blanket implementation (`impl<T> Trait for T`), a `pub` trait of a library (consumers may implement it) and a
//!   trait that is a supertrait of another are not reported.
//! - **A chain of forwarding types.** A struct with one field, whose every method that takes `self` (at least two) is one call to that field
//!   with the same arguments, is a *forwarding type*; when the type of its field is a forwarding type of the package too (through `Arc`,
//!   `Rc`, `Box` or a reference), the two are a chain: `Service` handing every call to `Repository`, which hands every call to `Adapter`. A
//!   chain is reported once, at its first type, with its length of two forwarding types or more.
//!
//! **Not done.** The design's chains of near-identical data mappers and its long call paths before any I/O happens need data-flow and a
//! notion of I/O that a syntactic read does not have.
//!
//! **Weak evidence by construction (V1, low confidence).** A single implementation may be a real boundary (a port that a second adapter
//! will fill, a seam for tests written elsewhere), a forwarding type may be a deliberate facade or a place a policy will go. The check
//! reports the shape and says so; it does not rank, change or offer a rewrite (patent watch, rows 64, 65 and 72 of
//! `docs/PATENT-SWEEP.md`). Syntactic: a type is matched by name; an implementation made by a macro or a trait object built elsewhere is not
//! seen.

use super::api_budget::surface;
use super::passthrough::{forwards_ident, strip};
use crate::check::{Check, CheckContext, Network, Scope};
use crate::checks::rust_sources::{read_sources, read_test_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_008;

/// Reports single-implementation traits and chains of forwarding types (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct LayersCheck;

impl LayersCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for LayersCheck {
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
        let tests = read_test_sources(&package.dir)?;
        let mut facts = collect(&files)?;
        facts.test_impls.extend(test_file_impls(&tests)?);
        facts.test_impls.extend(cfg_test_impls(&files)?);
        let has_lib = false && files.iter().any(|f| f.path == "src/lib.rs");
        let public = if has_lib {
            public_names(&files)?
        } else {
            BTreeSet::new()
        };
        let project = package.dir.file_name().map_or_else(
            || package.name.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let (traits, chains) = (
            single_impl_traits(&facts, &public),
            forwarding_chains(&facts),
        );
        let notes = vec![format!(
            "ARC-008 read {} trait(s) and {} struct(s); {} single-implementation trait(s), {} chain(s) of forwarding types",
            facts.traits.len(),
            facts.structs.len(),
            traits.len(),
            chains.len()
        )];
        Ok((judge(&traits, &chains, &project), notes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TraitFact {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub public: bool,
    pub methods: usize,
    pub supertraits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructFact {
    pub name: String,
    pub file: String,
    pub line: usize,
    /// The name of the single field's type, through `Arc`, `Rc`, `Box` and references, when the struct has exactly one field.
    pub inner: Option<String>,
    /// Methods taking `self` and how many of them only forward to the field.
    pub self_methods: usize,
    pub forwarding_methods: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Facts {
    pub traits: Vec<TraitFact>,
    pub structs: Vec<StructFact>,
    /// `(trait, implementing type, blanket)` for every `impl Trait for X` in non-test code.
    pub impls: Vec<(String, String, bool)>,
    /// Methods taking `self` of every inherent impl: `(type name, forwards its arguments to a field)`.
    pub methods: Vec<(String, bool)>,
    /// Traits implemented in test code.
    pub test_impls: BTreeSet<String>,
}

fn last_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => last_ident(&r.elem),
        syn::Type::Paren(p) => last_ident(&p.elem),
        _ => None,
    }
}

/// The type name behind `Arc<T>`, `Rc<T>`, `Box<T>` and `&T`, else the type's own last segment.
fn inner_type_name(ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(p) = ty {
        let last = p.path.segments.last()?;
        if matches!(last.ident.to_string().as_str(), "Arc" | "Rc" | "Box") {
            if let syn::PathArguments::AngleBracketed(a) = &last.arguments {
                if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                    return inner_type_name(t);
                }
            }
        }
    }
    last_ident(ty)
}

/// Whether a method body is one call on `self.<field>` with all the parameters in order, looking through `Ok`, `Some`, `?`, `.await`
/// and `.into()`.
fn forwards_to_field(sig: &syn::Signature, block: &syn::Block) -> bool {
    let mut params = Vec::new();
    for input in &sig.inputs {
        if let syn::FnArg::Typed(pt) = input {
            match &*pt.pat {
                syn::Pat::Ident(pi) if pi.subpat.is_none() && pi.by_ref.is_none() => {
                    params.push(pi.ident.to_string());
                }
                _ => return false,
            }
        }
    }
    let [syn::Stmt::Expr(e, _)] = block.stmts.as_slice() else {
        return false;
    };
    let syn::Expr::MethodCall(m) = strip(e) else {
        return false;
    };
    let on_field = match &*m.receiver {
        syn::Expr::Field(f) => matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")),
        syn::Expr::Reference(r) => {
            matches!(&*r.expr, syn::Expr::Field(f) if matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")))
        }
        _ => false,
    };
    on_field
        && m.args.len() == params.len()
        && m.args
            .iter()
            .zip(&params)
            .all(|(a, p)| forwards_ident(a, p))
}

pub(crate) fn collect(files: &[SourceFile]) -> anyhow::Result<Facts> {
    let mut facts = Facts::default();
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        walk(&parsed.items, &f.path, &mut facts);
    }
    // the methods of inherent impls are attached to their struct by name, in whatever file they are written
    for s in &mut facts.structs {
        s.self_methods = facts.methods.iter().filter(|(t, _)| *t == s.name).count();
        s.forwarding_methods = facts
            .methods
            .iter()
            .filter(|(t, fwd)| *t == s.name && *fwd && s.inner.is_some())
            .count();
    }
    Ok(facts)
}

fn walk(items: &[syn::Item], file: &str, facts: &mut Facts) {
    for item in items {
        match item {
            syn::Item::Trait(t) if !is_cfg_test(&t.attrs) => {
                facts.traits.push(TraitFact {
                    name: t.ident.to_string(),
                    file: file.to_string(),
                    line: t.ident.span().start().line,
                    public: matches!(t.vis, syn::Visibility::Public(_)),
                    methods: t
                        .items
                        .iter()
                        .filter(|i| matches!(i, syn::TraitItem::Fn(_)))
                        .count(),
                    supertraits: t
                        .supertraits
                        .iter()
                        .filter_map(|b| match b {
                            syn::TypeParamBound::Trait(tb) => {
                                tb.path.segments.last().map(|s| s.ident.to_string())
                            }
                            _ => None,
                        })
                        .collect(),
                });
            }
            syn::Item::Struct(s) if !is_cfg_test(&s.attrs) => {
                let fields: Vec<&syn::Field> = s.fields.iter().collect();
                facts.structs.push(StructFact {
                    name: s.ident.to_string(),
                    file: file.to_string(),
                    line: s.ident.span().start().line,
                    inner: (fields.len() == 1)
                        .then(|| inner_type_name(&fields[0].ty))
                        .flatten(),
                    self_methods: 0,
                    forwarding_methods: 0,
                });
            }
            syn::Item::Impl(im) if !is_cfg_test(&im.attrs) => {
                let self_name = last_ident(&im.self_ty);
                if let Some((path, _)) = &im.trait_ {
                    let blanket = matches!(&*im.self_ty, syn::Type::Path(p) if p.path.segments.len() == 1
                        && im.generics.params.iter().any(|g| matches!(g, syn::GenericParam::Type(t) if t.ident == p.path.segments[0].ident)));
                    if let (Some(tr), Some(ty)) = (path.segments.last(), &self_name) {
                        facts
                            .impls
                            .push((tr.ident.to_string(), ty.clone(), blanket));
                    }
                } else if let Some(name) = self_name {
                    // attach after all structs are known: remember by name on the struct entries
                    for it in &im.items {
                        if let syn::ImplItem::Fn(m) = it {
                            if m.sig.receiver().is_none() || is_cfg_test(&m.attrs) {
                                continue;
                            }
                            facts
                                .methods
                                .push((name.clone(), forwards_to_field(&m.sig, &m.block)));
                        }
                    }
                }
            }
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    walk(inner, file, facts);
                }
            }
            _ => {}
        }
    }
}

/// The traits implemented in test files (`tests.rs`, `*_test.rs`) and in `#[cfg(test)]` items of the files read.
pub(crate) fn test_file_impls(tests: &[SourceFile]) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for f in tests {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        impls_in(&parsed.items, &mut out);
    }
    Ok(out)
}

fn impls_in(items: &[syn::Item], out: &mut BTreeSet<String>) {
    for item in items {
        match item {
            syn::Item::Impl(im) => {
                if let Some((path, _)) = &im.trait_ {
                    if let Some(tr) = path.segments.last() {
                        out.insert(tr.ident.to_string());
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    impls_in(inner, out);
                }
            }
            _ => {}
        }
    }
}

/// The traits implemented inside `#[cfg(test)]` items of the non-test files.
pub(crate) fn cfg_test_impls(files: &[SourceFile]) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        cfg_test_in(&parsed.items, &mut out);
    }
    Ok(out)
}

fn cfg_test_in(items: &[syn::Item], out: &mut BTreeSet<String>) {
    for item in items {
        let attrs: &[syn::Attribute] = match item {
            syn::Item::Mod(m) => &m.attrs,
            syn::Item::Impl(i) => &i.attrs,
            _ => &[],
        };
        if is_cfg_test(attrs) {
            impls_in(std::slice::from_ref(item), out);
        } else if let syn::Item::Mod(m) = item {
            if let Some((_, inner)) = &m.content {
                cfg_test_in(inner, out);
            }
        }
    }
}

fn public_names(files: &[SourceFile]) -> anyhow::Result<BTreeSet<String>> {
    let (totals, _) = surface(files, "", &BTreeSet::new())?;
    Ok(totals.public_names)
}

/// A trait with one implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SingleImpl {
    pub name: String,
    pub implementor: String,
    pub file: String,
    pub line: usize,
}

const DOUBLE_NAMES: &[&str] = &[
    "Mock", "Fake", "Stub", "Dummy", "Spy", "InMemory", "Noop", "NoOp", "Null", "Test",
];

pub(crate) fn single_impl_traits(f: &Facts, public: &BTreeSet<String>) -> Vec<SingleImpl> {
    let supertraits: BTreeSet<&str> = f
        .traits
        .iter()
        .flat_map(|t| t.supertraits.iter().map(String::as_str))
        .collect();
    let mut out = Vec::new();
    for t in &f.traits {
        if t.methods == 0
            || public.contains(&t.name)
            || supertraits.contains(t.name.as_str())
            || f.test_impls.contains(&t.name)
        {
            continue;
        }
        let impls: Vec<&(String, String, bool)> =
            f.impls.iter().filter(|(tr, _, _)| *tr == t.name).collect();
        if impls.len() != 1 || impls[0].2 {
            continue;
        }
        let implementor = &impls[0].1;
        if DOUBLE_NAMES.iter().any(|d| implementor.contains(d)) {
            continue;
        }
        out.push(SingleImpl {
            name: t.name.clone(),
            implementor: implementor.clone(),
            file: t.file.clone(),
            line: t.line,
        });
    }
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
}

/// A chain of forwarding types, first type first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chain {
    pub types: Vec<String>,
    pub file: String,
    pub line: usize,
}

pub(crate) fn forwarding_chains(f: &Facts) -> Vec<Chain> {
    let forwarding: BTreeMap<&str, &StructFact> = f
        .structs
        .iter()
        .filter(|s| {
            s.inner.is_some() && s.self_methods >= 2 && s.forwarding_methods == s.self_methods
        })
        .map(|s| (s.name.as_str(), s))
        .collect();
    let pointed_to: BTreeSet<&str> = forwarding
        .values()
        .filter_map(|s| s.inner.as_deref())
        .filter(|i| forwarding.contains_key(i))
        .collect();
    let mut out = Vec::new();
    for (name, s) in &forwarding {
        if pointed_to.contains(name) {
            continue;
        }
        let mut types = vec![(*name).to_string()];
        let mut cur = *s;
        while let Some(next) = cur.inner.as_deref().and_then(|i| forwarding.get(i)) {
            if types.iter().any(|t| t == &next.name) {
                break;
            }
            types.push(next.name.clone());
            cur = next;
        }
        if types.len() >= 2 {
            out.push(Chain {
                types,
                file: s.file.clone(),
                line: s.line,
            });
        }
    }
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
}

pub(crate) fn judge(traits: &[SingleImpl], chains: &[Chain], project: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in traits {
        out.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Info,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: t.file.clone(),
                line: u32::try_from(t.line).ok(),
            }),
            subject: Some(t.name.clone()),
            summary: format!(
                "the trait `{}` has one implementation, `{}`, and is never implemented in test code",
                t.name, t.implementor
            ),
            detail: format!(
                "`{}` is implemented once in this package (by `{}`), no implementation is written in test code and the implementing type is not named like a double, and it is not public API, a supertrait or blanket-implemented. A layer like this earns its place by a second implementation, a test seam or a boundary somebody will fill; none shows in this package. This is the shape only, so Info at low confidence; allowlist the trait name with the reason if it is a deliberate seam.",
                t.name, t.implementor
            ),
            member: None,
            positive_control: None,
        });
    }
    for c in chains {
        out.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Info,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: c.file.clone(),
                line: u32::try_from(c.line).ok(),
            }),
            subject: Some(c.types[0].clone()),
            summary: format!(
                "{} types only forward their calls to the type inside them: {}",
                c.types.len(),
                c.types
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ),
            detail: format!(
                "Each of {} holds one field of the next and every method that takes `self` is one call to it with the same arguments. A reader hops through each layer for no change of meaning. It may be a deliberate facade or a place a policy will go. This is the shape only, so Info at low confidence; allowlist the first type's name with the reason if the layers are deliberate.",
                c.types.iter().map(|t| format!("`{t}`")).collect::<Vec<_>>().join(", ")
            ),
            member: None,
            positive_control: None,
        });
    }
    out
}

#[cfg(test)]
mod tests;
