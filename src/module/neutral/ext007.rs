// SPDX-License-Identifier: MIT OR Apache-2.0
//! EXT-007: speculative generality, first cut: a crate-private trait with exactly one implementation (Rust).
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package outside vendored directories, `tests/`,
//! `examples/`, `benches/` and `build.rs` is parsed with `syn`. A trait is reported when
//!
//! 1. it is **not `pub`** (private, `pub(crate)`, `pub(super)`, `pub(in ..)`): nobody outside the package can implement it, so every
//!    implementer is in the files read;
//! 2. it has at least one method, type or constant (a marker trait with no items is not an abstraction over behaviour);
//! 3. exactly **one** `impl Trait for Type` names it in the package, outside `#[cfg(test)]`, and **none** in test code (a second
//!    implementer in a `#[cfg(test)]` module is the fake or mock the seam exists for) and the trait is not `#[automock]`ed;
//! 4. that one implementation is for a concrete type, not a blanket `impl<T: ..> Trait for T`;
//! 5. the implementation is for a type the package defines (a trait implemented for a primitive or another crate's type is an
//!    extension trait, the way to add a method, not a seam);
//! 6. its name is unique among the package's traits, and no `macro_rules!` body of the package names it (a macro may generate the other
//!    implementations).
//!
//! One Low-severity, Low-confidence finding per trait, at the trait.
//!
//! **What it cannot see (and the record says so):** a trait kept for a reason the source does not show (a plugin point whose second
//! implementation is in another repository, a seam for tests in a `tests/` directory, a trait that is `pub` inside a private module);
//! unused type parameters and parameters that always receive the same argument (the record's other two halves); history that shows an
//! abstraction never extended (V3); other languages. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::visit::Visit;
use syn::{Item, ItemImpl, ItemMacro, ItemMod, ItemTrait, Type, Visibility};

const ID: &str = crate::rule_id::EXT_007;

struct TraitDecl {
    path: String,
    line: u32,
}

#[derive(Default)]
struct Scan {
    path: String,
    traits: BTreeMap<String, Vec<TraitDecl>>,
    /// Implementations of a trait name outside test code.
    impls: BTreeMap<String, usize>,
    /// Implementations of a trait name in test code, or blanket impls (either keeps the trait out of the report).
    other_impls: BTreeMap<String, usize>,
    macro_text: String,
    in_test: bool,
    /// Names of the types the package defines (struct, enum, union, alias).
    local_types: std::collections::BTreeSet<String>,
    /// For each trait name, the last path segment of the type of its counted implementations.
    impl_types: BTreeMap<String, Vec<String>>,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn has_automock(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "automock" || s.ident == "mockall")
    })
}

fn is_blanket(self_ty: &Type, generics: &syn::Generics) -> bool {
    match self_ty {
        Type::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => generics
            .type_params()
            .any(|t| p.path.segments[0].ident == t.ident),
        Type::Reference(r) => is_blanket(&r.elem, generics),
        _ => false,
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, i: &'ast ItemMod) {
        let before = self.in_test;
        if is_cfg_test(&i.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, i);
        self.in_test = before;
    }

    fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
        if !self.in_test
            && !is_cfg_test(&i.attrs)
            && !has_automock(&i.attrs)
            && !matches!(i.vis, Visibility::Public(_))
            && !i.items.is_empty()
        {
            self.traits
                .entry(i.ident.to_string())
                .or_default()
                .push(TraitDecl {
                    path: self.path.clone(),
                    line: line_of(i.ident.span()),
                });
        }
        syn::visit::visit_item_trait(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if let Some((path, _)) = &i.trait_ {
            if let Some(last) = path.segments.last() {
                let name = last.ident.to_string();
                let counted =
                    !self.in_test && !is_cfg_test(&i.attrs) && !is_blanket(&i.self_ty, &i.generics);
                if counted {
                    let ty = match &*i.self_ty {
                        Type::Path(p) => p
                            .path
                            .segments
                            .last()
                            .map(|s| s.ident.to_string())
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    self.impl_types.entry(name.clone()).or_default().push(ty);
                }
                let map = if counted {
                    &mut self.impls
                } else {
                    &mut self.other_impls
                };
                *map.entry(name).or_default() += 1;
            }
        }
        syn::visit::visit_item_impl(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast ItemMacro) {
        if i.mac.path.is_ident("macro_rules") {
            self.macro_text.push_str(&i.mac.tokens.to_string());
            self.macro_text.push(' ');
        }
    }

    fn visit_item(&mut self, i: &'ast Item) {
        match i {
            Item::Struct(x) => {
                self.local_types.insert(x.ident.to_string());
            }
            Item::Enum(x) => {
                self.local_types.insert(x.ident.to_string());
            }
            Item::Union(x) => {
                self.local_types.insert(x.ident.to_string());
            }
            Item::Type(x) => {
                self.local_types.insert(x.ident.to_string());
            }
            _ => {}
        }
        syn::visit::visit_item(self, i);
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
        let mut scan = Scan::default();
        for s in &p.sources {
            scan.path = s.path.clone();
            scan.visit_file(&s.file);
        }
        let files = p.sources.len();
        for (name, decls) in &scan.traits {
            if decls.len() != 1 {
                continue;
            }
            if scan.impls.get(name).copied().unwrap_or(0) != 1
                || scan.other_impls.get(name).copied().unwrap_or(0) != 0
                || scan.macro_text.contains(name.as_str())
            {
                continue;
            }
            // An extension trait (a method added to a type the package does not define, a primitive or another crate's type) is an
            // idiom, not a seam: the trait is the only way to add the method.
            let implemented_for_local = scan
                .impl_types
                .get(name)
                .is_some_and(|v| v.iter().all(|t| scan.local_types.contains(t)));
            if !implemented_for_local {
                continue;
            }
            let d = &decls[0];
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Low,
                    view.name(),
                    format!("the crate-private trait `{name}` has exactly one implementation and no test double"),
                    "Nothing outside the crate can implement it and nothing inside varies it. Inline the abstraction (use the concrete type) until a second real implementer appears, or write the second one (a test double counts) so the seam is exercised.",
                )
                .location(Location::new(d.path.as_str(), Some(d.line)))
                .subject(name.clone())
                .positive_control(format!(
                    "{files} Rust source file(s) of the package were parsed and every `impl .. for ..` counted by trait name"
                )),
            );
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

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

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
    fn a_private_trait_with_one_impl_is_reported() {
        let f = found("trait Store { fn get(&self) -> u8; }\nstruct Mem;\nimpl Store for Mem { fn get(&self) -> u8 { 1 } }\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject.as_deref(), Some("Store"));
    }

    #[test]
    fn two_impls_a_test_double_or_a_public_trait_are_not_reported() {
        assert!(found("trait S { fn g(&self); }\nstruct A;\nstruct B;\nimpl S for A { fn g(&self) {} }\nimpl S for B { fn g(&self) {} }\n").is_empty());
        assert!(found("trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\n#[cfg(test)]\nmod t { use super::*; struct F; impl S for F { fn g(&self) {} } }\n").is_empty());
        assert!(found(
            "pub trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\n"
        )
        .is_empty());
    }

    #[test]
    fn a_marker_trait_a_blanket_impl_and_a_macro_named_trait_are_not_reported() {
        assert!(found("trait M {}\nstruct A;\nimpl M for A {}\n").is_empty());
        assert!(found("trait S { fn g(&self); }\nimpl<T> S for T { fn g(&self) {} }\n").is_empty());
        assert!(found("trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\nmacro_rules! m { ($t:ty) => { impl S for $t { fn g(&self) {} } } }\n").is_empty());
    }

    #[test]
    fn an_extension_trait_on_a_foreign_type_is_not_reported() {
        assert!(found(
            "trait ToF32 { fn to_f32(self) -> f32; }
impl ToF32 for f64 { fn to_f32(self) -> f32 { self as f32 } }
"
        )
        .is_empty());
    }

    #[test]
    fn a_trait_with_no_impl_is_not_reported() {
        assert!(found("trait S { fn g(&self); }\n").is_empty());
    }
}
