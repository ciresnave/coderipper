// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-007: a function takes two or more `bool` parameters (Rust).
//!
//! **What this judges (Rust, from the source alone):** every function with a parameter list (a free function, a method of an inherent
//! `impl`, a method declared in a trait), in `.rs` files outside vendored directories, `tests/`, `examples/`, `benches/`, `build.rs`,
//! outside `#[cfg(test)]` items and `#[test]` functions, with **two or more parameters whose type is the bare `bool`**
//! (`&bool` and `Option<bool>` are other types and are not counted), is reported once, at its name. At a call site `f(true, false)`
//! says nothing about which flag is which, and two flags of one type can be swapped without the compiler noticing; an enum per
//! flag (or a small options struct) says it.
//!
//! **Not read:** the methods of `impl Trait for Type` (the trait dictates the signature); the record's second half, "a bare boolean
//! *return* whose meaning is unclear" (a function that returns `bool` is usually a predicate, and the name says what the answer
//! means: not judged); a `bool` behind a type alias; macro-generated functions; other languages. A file `syn` cannot parse is named
//! in a note, never read as clean. Low severity, Medium confidence. Related, not the same: `RDB-004` reads a *single* flag argument
//! that the body branches on.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{FnArg, ImplItemFn, ItemFn, ItemMod, Signature, TraitItemFn, Type};

const ID: &str = crate::rule_id::TYP_007;

/// How many `bool` parameters make a finding.
const MIN_BOOLS: usize = 2;

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "test" || s.ident == "bench")
    })
}

fn is_bool(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p.qself.is_none() && p.path.is_ident("bool"),
        Type::Paren(p) => is_bool(&p.elem),
        Type::Group(g) => is_bool(&g.elem),
        _ => false,
    }
}

struct Site {
    name: String,
    line: u32,
    bools: Vec<String>,
}

#[derive(Default)]
struct Collect {
    owner: Vec<String>,
    sites: Vec<Site>,
    in_test: bool,
}

impl Collect {
    fn signature(&mut self, sig: &Signature) {
        let bools: Vec<String> = sig
            .inputs
            .iter()
            .filter_map(|a| match a {
                FnArg::Typed(t) if is_bool(&t.ty) => Some(match &*t.pat {
                    syn::Pat::Ident(i) => i.ident.to_string(),
                    _ => "_".to_string(),
                }),
                _ => None,
            })
            .collect();
        if bools.len() >= MIN_BOOLS {
            let name = if self.owner.is_empty() {
                sig.ident.to_string()
            } else {
                format!("{}::{}", self.owner.join("::"), sig.ident)
            };
            self.sites.push(Site {
                name,
                line: line_of(sig.ident.span()),
                bools,
            });
        }
    }
}

impl<'ast> Visit<'ast> for Collect {
    fn visit_item_mod(&mut self, i: &'ast ItemMod) {
        let before = self.in_test;
        if is_cfg_test(&i.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, i);
        self.in_test = before;
    }

    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        if !self.in_test && !is_cfg_test(&i.attrs) && !has_test_attr(&i.attrs) {
            self.signature(&i.sig);
            syn::visit::visit_item_fn(self, i);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if self.in_test || is_cfg_test(&i.attrs) || i.trait_.is_some() {
            return;
        }
        let owner = match &*i.self_ty {
            Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.owner.push(owner);
        syn::visit::visit_item_impl(self, i);
        self.owner.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast ImplItemFn) {
        if !self.in_test && !is_cfg_test(&i.attrs) && !has_test_attr(&i.attrs) {
            self.signature(&i.sig);
        }
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if self.in_test || is_cfg_test(&i.attrs) {
            return;
        }
        self.owner.push(i.ident.to_string());
        syn::visit::visit_item_trait(self, i);
        self.owner.pop();
    }

    fn visit_trait_item_fn(&mut self, i: &'ast TraitItemFn) {
        if !self.in_test && !is_cfg_test(&i.attrs) {
            self.signature(&i.sig);
        }
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        for s in &p.sources {
            let mut c = Collect::default();
            c.visit_file(&s.file);
            for site in c.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Low,
                        Confidence::Medium,
                        view.name(),
                        format!(
                            "`{}` takes {} `bool` parameters ({}): at a call site the arguments do not say which flag is which",
                            site.name,
                            site.bools.len(),
                            site.bools
                                .iter()
                                .map(|b| format!("`{b}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        "Replace each flag with an enum whose variants name the choice, or group the flags in a small options struct; or split the function by the behaviour the flags select.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(site.name)
                    .positive_control(format!(
                        "{} Rust source file(s) of the package were read for the same signatures",
                        p.sources.len()
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            (
                "Cargo.toml",
                "[package]\nname = \"p\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn two_bool_parameters_are_reported_once_per_function() {
        let f = found("pub fn f(a: bool, b: bool, c: u8) {}\n");
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("2 `bool` parameters"));
        assert_eq!(found("pub fn f(a: bool, b: bool, c: bool) {}\n").len(), 1);
    }

    #[test]
    fn methods_and_trait_declarations_are_read() {
        assert_eq!(
            found("struct S;\nimpl S { fn m(&self, a: bool, b: bool) {} }\n").len(),
            1
        );
        assert_eq!(
            found("trait T { fn m(&self, a: bool, b: bool); }\n").len(),
            1
        );
    }

    #[test]
    fn one_bool_other_types_trait_impls_and_tests_are_not_reported() {
        assert!(found("fn f(a: bool, b: u8) {}\n").is_empty());
        assert!(found("fn f(a: &bool, b: Option<bool>) {}\n").is_empty());
        assert!(
            found("struct S;\ntrait T { fn m(&self); }\nimpl T for S { fn m(&self) {} }\n")
                .is_empty()
        );
        assert!(found("struct S;\ntrait T { fn m(&self, a: bool, b: bool); }\nimpl T for S { fn m(&self, a: bool, b: bool) {} }\n")
            .iter()
            .all(|f| f.subject.as_deref() == Some("T::m")));
        assert!(found("#[cfg(test)]\nmod t { fn f(a: bool, b: bool) {} }\n#[test]\nfn g(a: bool, b: bool) {}\n").is_empty());
    }
}
