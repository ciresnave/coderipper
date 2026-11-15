// SPDX-License-Identifier: MIT OR Apache-2.0
//! EXT-004: fat interfaces, the implementer half (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories is parsed with `syn`; items under
//! `#[cfg(test)]` and functions marked `#[test]` are skipped. The package is read as a whole, in two passes:
//!
//! 1. each `trait` declared in the package, with the number of methods it declares (`MIN_METHODS`, six, makes it large);
//! 2. each `impl Trait for Type` in the package, with the number of its methods that are *stubs*: an empty body, or a body that is only
//!    `unimplemented!()` or `todo!()` (`MIN_STUBS`, three, makes an implementer a complaint).
//!
//! A trait is reported once, at its declaration, when it is large and at least one implementer in the package stubs `MIN_STUBS` or more
//! of it: that implementer was forced to accept members it has no use for.
//!
//! **Not counted:** a default method an implementer does not override. A default is the trait author's own answer to "most implementers
//! will not care", so leaving it alone is the design working, not a fat interface. A trait name declared twice in the package is skipped
//! (the impl cannot be matched to one of them without name resolution), as is a trait from another crate.
//!
//! **Deliberately not built: the per-client usage half.** The rule's statement also reports an interface whose *clients* each use disjoint
//! subsets of its members. That needs a call graph resolved to the trait's methods through generics and `dyn` values, per client, which
//! this reader (syntax only, no name or type resolution) cannot do honestly: a count of method names would match unrelated methods that
//! share a name and miss every call made through a generic bound. The catalog already rates it not worth building for Rust yet; this
//! check reports only what the syntax shows, the stubbed members.
//!
//! A file `syn` cannot parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, HashMap};
use syn::visit::Visit;
use syn::{Block, Expr, ImplItem, ItemImpl, ItemMod, ItemTrait, Stmt, TraitItem};

/// A trait of at least this many methods is large.
const MIN_METHODS: usize = 6;
/// An implementer that stubs at least this many members is the complaint.
const MIN_STUBS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TraitDecl {
    pub name: String,
    pub line: u32,
    pub methods: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImplDecl {
    /// The last path segment of the implemented trait.
    pub trait_name: String,
    pub stubs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hit {
    pub path: String,
    pub line: u32,
    pub name: String,
    pub methods: usize,
    /// The most members stubbed by one implementer.
    pub max_stubs: usize,
    /// How many implementers stub `MIN_STUBS` or more.
    pub implementers: usize,
}

#[derive(Default)]
struct Scan {
    traits: Vec<TraitDecl>,
    impls: Vec<ImplDecl>,
}

/// Whether a method body does nothing: empty, or only `unimplemented!()` / `todo!()`.
fn is_stub(b: &Block) -> bool {
    match b.stmts.as_slice() {
        [] => true,
        [one] => {
            let mac = match one {
                Stmt::Macro(m) => Some(&m.mac),
                Stmt::Expr(Expr::Macro(m), _) => Some(&m.mac),
                _ => None,
            };
            mac.and_then(|m| m.path.segments.last())
                .is_some_and(|s| s.ident == "unimplemented" || s.ident == "todo")
        }
        _ => false,
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        if t.attrs.iter().any(is_test_attr) {
            return;
        }
        let methods = t
            .items
            .iter()
            .filter(|i| matches!(i, TraitItem::Fn(_)))
            .count();
        self.traits.push(TraitDecl {
            name: t.ident.to_string(),
            line: line_of(t.ident.span()),
            methods,
        });
        syn::visit::visit_item_trait(self, t);
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if i.attrs.iter().any(is_test_attr) {
            return;
        }
        if let Some((path, _)) = &i.trait_ {
            if let Some(seg) = path.segments.last() {
                let stubs = i
                    .items
                    .iter()
                    .filter(|it| matches!(it, ImplItem::Fn(f) if is_stub(&f.block)))
                    .count();
                self.impls.push(ImplDecl {
                    trait_name: seg.ident.to_string(),
                    stubs,
                });
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

fn scan(text: &str) -> Option<Scan> {
    let parsed = syn::parse_file(text).ok()?;
    let mut s = Scan::default();
    s.visit_file(&parsed);
    Some(s)
}

/// The traits of one package that are large and stubbed by an implementer, over `(path, text)` pairs. The second value is the paths that
/// did not parse.
pub(super) fn analyse(files: &[(String, String)]) -> (Vec<Hit>, Vec<String>) {
    let mut decls: HashMap<String, Vec<(String, TraitDecl)>> = HashMap::new();
    let mut impls: Vec<ImplDecl> = Vec::new();
    let mut unparsed = Vec::new();
    for (path, text) in files {
        let Some(s) = scan(text) else {
            unparsed.push(path.clone());
            continue;
        };
        for t in s.traits {
            decls
                .entry(t.name.clone())
                .or_default()
                .push((path.clone(), t));
        }
        impls.extend(s.impls);
    }
    let mut by_trait: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for i in &impls {
        if i.stubs >= MIN_STUBS {
            let e = by_trait.entry(i.trait_name.as_str()).or_insert((0, 0));
            e.0 = e.0.max(i.stubs);
            e.1 += 1;
        }
    }
    let mut out = Vec::new();
    for (name, (max_stubs, implementers)) in by_trait {
        let Some(list) = decls.get(name) else {
            continue;
        };
        let [(path, t)] = list.as_slice() else {
            continue;
        };
        if t.methods >= MIN_METHODS {
            out.push(Hit {
                path: path.clone(),
                line: t.line,
                name: t.name.clone(),
                methods: t.methods,
                max_stubs,
                implementers,
            });
        }
    }
    out.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    (out, unparsed)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let id = crate::rule_id::EXT_004;
    let paths: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p) && !is_dev_path(p))
        .collect();
    if paths.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source file outside tests".into(),
        );
    }
    let mut files: Vec<(String, String)> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for p in &paths {
        match view.read(p) {
            Some(t) => files.push(((*p).clone(), t)),
            None => unread.push((*p).clone()),
        }
    }
    let (hits, mut unparsed) = analyse(&files);
    unparsed.extend(unread);
    let findings: Vec<Finding> = hits
        .iter()
        .map(|h| {
            Finding::new(
                id,
                Severity::Low,
                Confidence::Low,
                view.name(),
                format!(
                    "trait `{}` has {} methods and {} implementer(s) stub {} or more of them",
                    h.name, h.methods, h.implementers, MIN_STUBS
                ),
                format!(
                    "`{}` in `{}` declares {} methods, and {} `impl` block(s) in this package leave at least {} of them with an empty or \
                     `unimplemented!()` body (the most is {}): those implementers were forced to accept members they have no use for. Split the \
                     trait into small role traits that match what each implementer does, or give the members it rarely needs a default body. \
                     Read from the syntax only: how the trait's clients use it is not read, and a default method left alone is not counted.",
                    h.name, h.path, h.methods, h.implementers, MIN_STUBS, h.max_stubs
                ),
            )
            .location(Location::new(h.path.as_str(), Some(h.line)))
            .subject(format!("{}:{}", h.path, h.name))
            .positive_control(format!(
                "{} Rust file(s) parsed with syn; the trait was read at {}:{}",
                files.len() - unparsed.len().min(files.len()),
                h.path,
                h.line
            ))
        })
        .collect();
    let mut notes = vec![format!(
        "{id}: {} Rust file(s) read with syn",
        files.len().saturating_sub(unparsed.len())
    )];
    if !unparsed.is_empty() {
        notes.push(format!(
            "{id}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        ));
    }
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Vec<Hit> {
        analyse(&[("lib.rs".to_string(), src.to_string())]).0
    }

    const BIG: &str = "pub trait Store { fn a(&self); fn b(&self); fn c(&self); fn d(&self); fn e(&self); fn f(&self); }\n";

    #[test]
    fn an_implementer_that_stubs_three_of_six_is_reported_at_the_trait() {
        let src = format!(
            "{BIG}struct R;\nimpl Store for R {{ fn a(&self) {{}} fn b(&self) {{ unimplemented!() }} fn c(&self) {{ todo!(\"later\"); }} fn d(&self) {{ real(); }} fn e(&self) {{ real(); }} fn f(&self) {{ real(); }} }}"
        );
        let hits = one(&src);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (hits[0].line, hits[0].methods, hits[0].max_stubs),
            (1, 6, 3)
        );
    }

    #[test]
    fn two_stubs_or_a_small_trait_are_not_reported() {
        let two = format!(
            "{BIG}struct R;\nimpl Store for R {{ fn a(&self) {{}} fn b(&self) {{}} fn c(&self) {{ real(); }} fn d(&self) {{ real(); }} fn e(&self) {{ real(); }} fn f(&self) {{ real(); }} }}"
        );
        assert!(one(&two).is_empty());
        let small = "trait T { fn a(&self); fn b(&self); fn c(&self); }\nstruct R;\nimpl T for R { fn a(&self) {} fn b(&self) {} fn c(&self) {} }";
        assert!(one(small).is_empty());
    }

    #[test]
    fn a_default_method_left_alone_is_not_a_stub() {
        let src = "trait T { fn a(&self) {} fn b(&self) {} fn c(&self) {} fn d(&self) {} fn e(&self) {} fn f(&self) {} }\nstruct R;\nimpl T for R { }";
        assert!(one(src).is_empty());
    }

    #[test]
    fn the_impl_may_live_in_another_file() {
        let files = [
            ("a.rs".to_string(), BIG.to_string()),
            (
                "b.rs".to_string(),
                "impl Store for R { fn a(&self) {} fn b(&self) {} fn c(&self) {} }".to_string(),
            ),
        ];
        let (hits, _) = analyse(&files);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.rs");
    }

    #[test]
    fn a_trait_declared_twice_or_from_elsewhere_is_skipped() {
        let twice = format!("{BIG}mod m {{ {BIG} }}\nimpl Store for R {{ fn a(&self) {{}} fn b(&self) {{}} fn c(&self) {{}} }}");
        assert!(one(&twice).is_empty());
        let foreign = "impl Iterator for R { fn next(&mut self) {} fn a(&self) {} fn b(&self) {} }";
        assert!(one(foreign).is_empty());
    }

    #[test]
    fn test_code_is_skipped() {
        let src = format!(
            "#[cfg(test)]\nmod tests {{ {BIG} struct R; impl Store for R {{ fn a(&self) {{}} fn b(&self) {{}} fn c(&self) {{}} }} }}"
        );
        assert!(one(&src).is_empty());
    }

    #[test]
    fn a_file_syn_cannot_parse_is_listed() {
        let (hits, unparsed) = analyse(&[("x.rs".to_string(), "trait (((".to_string())]);
        assert!(hits.is_empty());
        assert_eq!(unparsed, vec!["x.rs".to_string()]);
    }
}
