// SPDX-License-Identifier: MIT OR Apache-2.0
//! Long reach-through chains (project scope, Rust): which expressions dig through several of the package's own structs to reach something
//! far from the value they started with (`order.customer().address().city()`, `app.config.net.proxy.host`)?
//!
//! Rule record: `MOD-009` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`. The idea is the Law of Demeter: a caller should talk to its neighbour, not
//! to its neighbour's neighbours.
//!
//! **What it counts.** The package's own structs are read first: for each struct declared in `src/` (test items skipped), the fields whose
//! type is *another* struct of the package (through `Arc`, `Rc`, `Box` and references), and the inherent methods that take `&self` or `self`
//! and nothing else and return another struct of the package. Each such field or accessor is a **hop** from its owner to its target. In
//! every method-call and field-access chain, a link is counted when its name is a hop from the type the previous link reached (the first
//! link may start from any owner: the receiver's type is not known to a syntactic read), and the number of consecutive counted links is the
//! chain's depth. A link that is not a hop ends the run. A chain whose depth reaches `[mod009] min_hops` (default 4) is one Info finding at
//! the first counted link.
//!
//! **Why this is not a builder or an iterator pipeline.** A builder method returns its own type (`Self`), and an iterator adaptor returns a
//! type of the standard library: neither moves from one struct of the package to a *different* one, so neither is a hop. The type-change
//! test is the whole rule.
//!
//! **Weak evidence by construction (Info, low confidence).** Types are matched by name: a field or method called like a hop on a value of
//! another type (a `config` field on a type outside the package) is counted when its name is a hop from the previous link's possible owners,
//! and a hop reached through a trait method, a macro, a closure parameter or a type alias is not seen. A facade that exists to be traversed
//! (a configuration tree, an AST) reads as a chain on purpose. The finding names the chain; it makes no change.
//!
//! **Known limitations / deliberate scope:**
//! - Syntactic (`syn`): `Option` and `Result` between links end a run (`a.b().unwrap().c()` is two short runs), as does any method that
//!   takes an argument.
//! - Only `src/` is read; `tests/`, `benches/` and `examples/` are not.

use crate::check::{Check, CheckContext, Network, Scope};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_009;

/// The depth reported when the repository sets no `[mod009] min_hops`.
pub(crate) const DEFAULT_MIN_HOPS: usize = 4;

/// Reports chains of field and accessor links that each move to a different struct of the package (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ReachThroughCheck;

impl ReachThroughCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for ReachThroughCheck {
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
        crate::package::fan_out(ctx, |ctx, package| {
            let min_hops =
                match std::fs::read_to_string(ctx.project_root.join(crate::config::FILE_NAME)) {
                    Ok(text) => crate::config::mod009_min_hops(&text)?.unwrap_or(DEFAULT_MIN_HOPS),
                    Err(_) => DEFAULT_MIN_HOPS,
                };
            let files = read_sources(&package.dir)?;
            let project = package.dir.file_name().map_or_else(
                || package.name.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
            judge(&files, min_hops, &project)
        })
    }
}

/// `(owner, target)` pairs of the package's own structs, by the name of the field or accessor that moves between them.
#[derive(Debug, Default)]
pub(crate) struct Hops {
    fields: BTreeMap<String, BTreeSet<(String, String)>>,
    methods: BTreeMap<String, BTreeSet<(String, String)>>,
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

#[derive(Default)]
struct Raw {
    structs: BTreeSet<String>,
    /// `(owner, field name, field type name)`
    fields: Vec<(String, String, String)>,
    /// `(owner, method name, return type name)`
    methods: Vec<(String, String, String)>,
}

fn collect_items(items: &[syn::Item], raw: &mut Raw) {
    for item in items {
        match item {
            syn::Item::Struct(s) if !is_cfg_test(&s.attrs) => {
                let owner = s.ident.to_string();
                raw.structs.insert(owner.clone());
                for (i, f) in s.fields.iter().enumerate() {
                    let name = f
                        .ident
                        .as_ref()
                        .map_or_else(|| i.to_string(), ToString::to_string);
                    if let Some(t) = inner_type_name(&f.ty) {
                        raw.fields.push((owner.clone(), name, t));
                    }
                }
            }
            syn::Item::Impl(im) if im.trait_.is_none() && !is_cfg_test(&im.attrs) => {
                let Some(owner) = last_ident(&im.self_ty) else {
                    continue;
                };
                for it in &im.items {
                    let syn::ImplItem::Fn(m) = it else { continue };
                    if is_cfg_test(&m.attrs) {
                        continue;
                    }
                    let takes_only_self = m.sig.receiver().is_some() && m.sig.inputs.len() == 1;
                    if !takes_only_self {
                        continue;
                    }
                    if let syn::ReturnType::Type(_, ty) = &m.sig.output {
                        if let Some(t) = inner_type_name(ty) {
                            raw.methods
                                .push((owner.clone(), m.sig.ident.to_string(), t));
                        }
                    }
                }
            }
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    collect_items(inner, raw);
                }
            }
            _ => {}
        }
    }
}

fn parse(f: &SourceFile) -> anyhow::Result<syn::File> {
    syn::parse_file(&f.text).map_err(|e| {
        anyhow::anyhow!(
            "could not parse {} with syn ({e}); refusing to guess",
            f.path
        )
    })
}

pub(crate) fn hops_of(parsed: &[syn::File]) -> Hops {
    let mut raw = Raw::default();
    for p in parsed {
        collect_items(&p.items, &mut raw);
    }
    let mut hops = Hops::default();
    // a hop moves to a different struct of the package: `Self` and non-package types are not hops
    for (owner, name, target) in &raw.fields {
        if raw.structs.contains(target) && target != owner {
            hops.fields
                .entry(name.clone())
                .or_default()
                .insert((owner.clone(), target.clone()));
        }
    }
    for (owner, name, target) in &raw.methods {
        if raw.structs.contains(target) && target != owner {
            hops.methods
                .entry(name.clone())
                .or_default()
                .insert((owner.clone(), target.clone()));
        }
    }
    hops
}

/// One link of a chain.
struct Link {
    name: String,
    method: bool,
    argc: usize,
    line: usize,
}

/// A chain found: where it starts, its counted links, and the function it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chain {
    pub file: String,
    pub line: usize,
    pub depth: usize,
    pub text: String,
    pub function: String,
}

/// The longest run of consecutive hops in `links`: `(depth, start index)`.
fn longest_run(links: &[Link], hops: &Hops) -> (usize, usize) {
    let mut current: Option<BTreeSet<String>> = None;
    let (mut run, mut best, mut best_start) = (0usize, 0usize, 0usize);
    for (i, l) in links.iter().enumerate() {
        let table = if l.method {
            &hops.methods
        } else {
            &hops.fields
        };
        let reached: BTreeSet<String> = table
            .get(&l.name)
            .into_iter()
            .flatten()
            .filter(|(owner, _)| current.as_ref().is_none_or(|c| c.contains(owner)))
            .map(|(_, target)| target.clone())
            .collect();
        if reached.is_empty() || (l.method && l.argc != 0) {
            run = 0;
            current = None;
            continue;
        }
        run += 1;
        current = Some(reached);
        if run > best {
            best = run;
            best_start = i + 1 - run;
        }
    }
    (best, best_start)
}

struct Walker<'a> {
    hops: &'a Hops,
    file: &'a str,
    functions: Vec<String>,
    chains: Vec<Chain>,
    /// Chains of each depth, to say what the threshold left out.
    histogram: BTreeMap<usize, usize>,
}

impl<'ast> Visit<'ast> for Walker<'_> {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_cfg_test(&i.attrs) {
            syn::visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_cfg_test(&i.attrs) {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_expr(&mut self, e: &'ast syn::Expr) {
        if !matches!(
            e,
            syn::Expr::MethodCall(_)
                | syn::Expr::Field(_)
                | syn::Expr::Try(_)
                | syn::Expr::Await(_)
        ) {
            syn::visit::visit_expr(self, e);
            return;
        }
        // the whole chain from its outermost expression; its inner parts are not visited again as chains of their own
        let mut links: Vec<Link> = Vec::new();
        let mut arg_lists: Vec<&syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>> =
            Vec::new();
        let mut cur = e;
        loop {
            match cur {
                syn::Expr::MethodCall(m) => {
                    links.push(Link {
                        name: m.method.to_string(),
                        method: true,
                        argc: m.args.len(),
                        line: m.method.span().start().line,
                    });
                    arg_lists.push(&m.args);
                    cur = &m.receiver;
                }
                syn::Expr::Field(f) => {
                    let (name, line) = match &f.member {
                        syn::Member::Named(id) => (id.to_string(), id.span().start().line),
                        syn::Member::Unnamed(ix) => (ix.index.to_string(), ix.span.start().line),
                    };
                    links.push(Link {
                        name,
                        method: false,
                        argc: 0,
                        line,
                    });
                    cur = &f.base;
                }
                syn::Expr::Try(t) => cur = &t.expr,
                syn::Expr::Await(a) => cur = &a.base,
                syn::Expr::Paren(p) => cur = &p.expr,
                _ => break,
            }
        }
        links.reverse();
        let (depth, start) = longest_run(&links, self.hops);
        if depth >= 2 {
            *self.histogram.entry(depth).or_default() += 1;
            let text = links[start..start + depth]
                .iter()
                .map(|l| {
                    if l.method {
                        format!("{}()", l.name)
                    } else {
                        l.name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(".");
            self.chains.push(Chain {
                file: self.file.to_string(),
                line: links[start].line,
                depth,
                text,
                function: self.functions.last().cloned().unwrap_or_default(),
            });
        }
        self.visit_expr(cur);
        for args in arg_lists {
            for a in args {
                self.visit_expr(a);
            }
        }
    }
}

/// The chains of every file in `files`, for `hops` read from the same files.
pub(crate) fn find_chains(
    files: &[SourceFile],
) -> anyhow::Result<(Vec<Chain>, BTreeMap<usize, usize>, Hops)> {
    let parsed: Vec<syn::File> = files.iter().map(parse).collect::<anyhow::Result<_>>()?;
    let hops = hops_of(&parsed);
    let mut chains = Vec::new();
    let mut histogram = BTreeMap::new();
    for (f, p) in files.iter().zip(&parsed) {
        let mut w = Walker {
            hops: &hops,
            file: &f.path,
            functions: Vec::new(),
            chains: Vec::new(),
            histogram: BTreeMap::new(),
        };
        w.visit_file(p);
        chains.append(&mut w.chains);
        for (d, n) in w.histogram {
            *histogram.entry(d).or_default() += n;
        }
    }
    Ok((chains, histogram, hops))
}

pub(crate) fn judge(
    files: &[SourceFile],
    min_hops: usize,
    project: &str,
) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
    let (mut chains, histogram, hops) = find_chains(files)?;
    chains.sort_by(|a, b| (b.depth, &a.file, a.line).cmp(&(a.depth, &b.file, b.line)));
    let findings: Vec<Finding> = chains
        .iter()
        .filter(|c| c.depth >= min_hops)
        .map(|c| Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Info,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: c.file.clone(),
                line: u32::try_from(c.line).ok(),
            }),
            subject: Some(c.text.clone()),
            summary: format!(
                "`{}` reaches through {} of the package's own structs, one after another{}",
                c.text,
                c.depth,
                if c.function.is_empty() { String::new() } else { format!(" (in `{}`)", c.function) }
            ),
            detail: format!(
                "Each link of `{}` is a field or an accessor that moves from one struct of this package to a different one, {} in a row, so the caller depends on how every struct along the path is put together. Ask the nearest struct to do the job, or give it a method that hides the traversal. This is a syntactic reading by name, so Info at low confidence: a struct that exists to be traversed (a configuration tree, a syntax tree) reads this way on purpose; allowlist the chain text with the reason. Raise `[mod009] min_hops` (now {min_hops}) to report fewer.",
                c.text, c.depth
            ),
            member: None,
            positive_control: None,
        })
        .collect();
    let hist = if histogram.is_empty() {
        "no chain of 2 or more hops".to_string()
    } else {
        histogram
            .iter()
            .map(|(d, n)| format!("{d} hops: {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let notes = vec![format!(
        "MOD-009 read {} field hop(s) and {} accessor hop(s) between the package's structs; chains by depth ({hist}); {} reported at {min_hops} hops or more",
        hops.fields.values().map(BTreeSet::len).sum::<usize>(),
        hops.methods.values().map(BTreeSet::len).sum::<usize>(),
        findings.len()
    )];
    Ok((findings, notes))
}

#[cfg(test)]
mod tests;
