// SPDX-License-Identifier: MIT OR Apache-2.0
//! COR-012: an `unsafe impl Send` or `unsafe impl Sync` is not contradicted by the type's own fields.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. Each `unsafe impl Send for T` and
//! `unsafe impl Sync for T` (a negative impl `!Send` is not one) is matched, by the type's name, to the one `struct`, `enum` or `union`
//! of that name in the same package, and reported once per impl when:
//!
//! - **`Rc`:** a field's type names `Rc` (or `rc::Weak`): a reference count that is not atomic is unsound to send or share, whatever
//!   else the type does (Medium confidence);
//! - **`Cell` or `RefCell` behind `Sync`:** `unsafe impl Sync` for a type with a `Cell` or `RefCell` field: both mutate through `&self`
//!   with no synchronisation (Medium confidence);
//! - **an unbounded type parameter:** `unsafe impl<T> Send for W<T>` (or `Sync`) where `T` has no trait bound at all, in the
//!   parameter list or a `where` clause, and the type has a field that mentions the matching parameter other than as `PhantomData`: the
//!   impl promises that `W<Rc<u8>>` is `Send` (Medium confidence).
//!
//! A raw pointer field is **not** reported: it is the usual reason for an `unsafe impl Send` and the reason this rule does not
//! second-guess it; `SEC-005` already asks that the impl carry a `SAFETY` comment.
//!
//! **What it cannot see:** a type declared in another package or behind a macro, two types of the same name in one package (skipped), a
//! `Cell` or `Rc` hidden behind a type alias or a field of a field's type, a bound that is too weak (`T: Send` for a `Sync` impl, or `T: Copy`) and a bound on a project trait that may or may not imply `Send` (`T: Element`: any bound but `Sized` counts as one),
//! whether the impl is sound for other reasons (a lock around the cell). A file `syn` cannot parse is named in a note, never read as
//! clean. Opt-in until a read of the portfolio gives a denominator.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

const ID: &str = crate::rule_id::COR_012;

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

/// The path-segment names a type mentions, outside a `PhantomData` argument when `skip_phantom` is set.
fn names_in(ty: &syn::Type, skip_phantom: bool) -> BTreeSet<String> {
    struct V {
        names: BTreeSet<String>,
        skip: bool,
    }
    impl<'ast> Visit<'ast> for V {
        fn visit_path(&mut self, p: &'ast syn::Path) {
            if self.skip && p.segments.last().is_some_and(|s| s.ident == "PhantomData") {
                return;
            }
            for s in &p.segments {
                self.names.insert(s.ident.to_string());
            }
            syn::visit::visit_path(self, p);
        }
    }
    let mut v = V {
        names: BTreeSet::new(),
        skip: skip_phantom,
    };
    v.visit_type(ty);
    v.names
}

/// What the file declares about one type: its generic parameter names in order and its field types.
struct Decl {
    params: Vec<String>,
    fields: Vec<syn::Type>,
    ambiguous: bool,
}

fn fields_of(f: &syn::Fields) -> Vec<syn::Type> {
    f.iter().map(|f| f.ty.clone()).collect()
}

fn type_params(g: &syn::Generics) -> Vec<String> {
    g.type_params().map(|p| p.ident.to_string()).collect()
}

#[derive(Default)]
struct Collect {
    decls: BTreeMap<String, Decl>,
    impls: Vec<ImplSite>,
}

struct ImplSite {
    line: u32,
    trait_name: String,
    self_name: String,
    self_args: Vec<String>,
    bounded: BTreeSet<String>,
    params: BTreeSet<String>,
}

impl Collect {
    fn declare(&mut self, name: String, d: Decl) {
        match self.decls.get_mut(&name) {
            Some(old) => old.ambiguous = true,
            None => {
                self.decls.insert(name, d);
            }
        }
    }
}

/// The type parameters of an impl that carry any trait bound other than `Sized`, in the parameter list or a `where` clause. A bound
/// the rule does not know (`T: Element`) may imply `Send` or `Sync`, so only a parameter with no bound at all is reported.
fn bounded_params(g: &syn::Generics) -> BTreeSet<String> {
    let says = |bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>| {
        bounds.iter().any(|b| match b {
            syn::TypeParamBound::Trait(t) => {
                t.path.segments.last().is_some_and(|s| s.ident != "Sized")
            }
            _ => false,
        })
    };
    let mut out = BTreeSet::new();
    for p in g.type_params() {
        if says(&p.bounds) {
            out.insert(p.ident.to_string());
        }
    }
    if let Some(w) = &g.where_clause {
        for pred in &w.predicates {
            if let syn::WherePredicate::Type(t) = pred {
                if says(&t.bounds) {
                    if let syn::Type::Path(p) = &t.bounded_ty {
                        if let Some(id) = p.path.get_ident() {
                            out.insert(id.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

impl<'ast> Visit<'ast> for Collect {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.declare(
            i.ident.to_string(),
            Decl {
                params: type_params(&i.generics),
                fields: fields_of(&i.fields),
                ambiguous: false,
            },
        );
    }

    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.declare(
            i.ident.to_string(),
            Decl {
                params: type_params(&i.generics),
                fields: i.fields.named.iter().map(|f| f.ty.clone()).collect(),
                ambiguous: false,
            },
        );
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let fields = i
            .variants
            .iter()
            .flat_map(|v| fields_of(&v.fields))
            .collect();
        self.declare(
            i.ident.to_string(),
            Decl {
                params: type_params(&i.generics),
                fields,
                ambiguous: false,
            },
        );
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if i.unsafety.is_some() {
            if let (Some((path, _)), None) = (&i.trait_, &i.modifiers.polarity) {
                if let Some(seg) = path.segments.last() {
                    let t = seg.ident.to_string();
                    if t == "Send" || t == "Sync" {
                        if let syn::Type::Path(p) = &*i.self_ty {
                            if let Some(last) = p.path.segments.last() {
                                let args = match &last.arguments {
                                    syn::PathArguments::AngleBracketed(a) => a
                                        .args
                                        .iter()
                                        .map(|a| match a {
                                            syn::GenericArgument::Type(syn::Type::Path(p)) => p
                                                .path
                                                .get_ident()
                                                .map(ToString::to_string)
                                                .unwrap_or_default(),
                                            _ => String::new(),
                                        })
                                        .collect(),
                                    _ => Vec::new(),
                                };
                                self.impls.push(ImplSite {
                                    line: line_of(
                                        i.unsafety
                                            .as_ref()
                                            .map_or_else(proc_macro2::Span::call_site, |u| u.span),
                                    ),
                                    trait_name: t,
                                    self_name: last.ident.to_string(),
                                    self_args: args,
                                    bounded: bounded_params(&i.generics),
                                    params: type_params(&i.generics).into_iter().collect(),
                                });
                            }
                        }
                    }
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

struct Hit {
    why: String,
}

fn judge_impl(site: &ImplSite, decl: &Decl) -> Option<Hit> {
    let names: BTreeSet<String> = decl
        .fields
        .iter()
        .flat_map(|f| names_in(f, false))
        .collect();
    if names.contains("Rc") {
        return Some(Hit {
            why: "a field is an `Rc`, whose count is not atomic".into(),
        });
    }
    if site.trait_name == "Sync" && (names.contains("Cell") || names.contains("RefCell")) {
        return Some(Hit {
            why: "a field is a `Cell` or `RefCell`, which mutate through `&self` with no synchronisation".into(),
        });
    }
    for (i, arg) in site.self_args.iter().enumerate() {
        if arg.is_empty() || !site.params.contains(arg) || site.bounded.contains(arg) {
            continue;
        }
        let Some(declared) = decl.params.get(i) else {
            continue;
        };
        let used = decl
            .fields
            .iter()
            .any(|f| names_in(f, true).contains(declared));
        if used {
            return Some(Hit {
                why: format!(
                    "the type parameter `{arg}` has no trait bound, so the impl also covers a `{}<Rc<u8>>`",
                    site.self_name
                ),
            });
        }
    }
    None
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        // The impl's own file is where it is reported: find it by re-scanning per file with the package's declarations.
        let mut all = Collect::default();
        for s in &p.sources {
            all.visit_file(&s.file);
        }
        for s in &p.sources {
            let mut here = Collect::default();
            here.visit_file(&s.file);
            for site in here.impls {
                let Some(decl) = all.decls.get(&site.self_name) else {
                    continue;
                };
                if decl.ambiguous {
                    continue;
                }
                let Some(hit) = judge_impl(&site, decl) else {
                    continue;
                };
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!(
                            "`unsafe impl {} for {}` in `{}` is contradicted by the type: {}",
                            site.trait_name, site.self_name, s.path, hit.why
                        ),
                        "An `unsafe impl Send` or `Sync` is a promise the compiler does not check. Remove the impl if a field cannot be sent or shared, replace `Rc` with `Arc` and `Cell` or `RefCell` with a `Mutex`, or add the bound (`T: Send`, `T: Sync`) the impl needs, and keep a `// SAFETY:` comment that says why the rest holds.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.self_name, site.trait_name))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed and every `unsafe impl Send` and `unsafe impl Sync` was matched to its type's fields"
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
    fn an_rc_field_behind_send_or_sync_is_reported() {
        for tr in ["Send", "Sync"] {
            let src = format!("struct S {{ a: std::rc::Rc<u8> }}\nunsafe impl {tr} for S {{}}\n");
            let f = found(&src);
            assert_eq!(f.len(), 1, "{tr}: {f:?}");
            assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        }
    }

    #[test]
    fn a_cell_is_reported_for_sync_only() {
        let sync = "struct S { a: RefCell<u8> }\nunsafe impl Sync for S {}\n";
        assert_eq!(found(sync).len(), 1);
        let cell = "struct S { a: Cell<u8> }\nunsafe impl Sync for S {}\n";
        assert_eq!(found(cell).len(), 1);
        let send = "struct S { a: Cell<u8> }\nunsafe impl Send for S {}\n";
        assert!(found(send).is_empty());
    }

    #[test]
    fn an_unbounded_parameter_is_reported_and_a_bounded_one_is_not() {
        let bad = "struct W<T> { v: T }\nunsafe impl<T> Send for W<T> {}\n";
        assert_eq!(found(bad).len(), 1);
        let good = "struct W<T> { v: T }\nunsafe impl<T: Send> Send for W<T> {}\n";
        assert!(found(good).is_empty());
        let wh = "struct W<T> { v: T }\nunsafe impl<T> Sync for W<T> where T: Sync + 'static {}\n";
        assert!(found(wh).is_empty());
    }

    #[test]
    fn a_phantom_only_parameter_is_not_reported() {
        let src = "struct W<T> { p: std::marker::PhantomData<T>, n: u8 }\nunsafe impl<T> Send for W<T> {}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_raw_pointer_field_is_the_usual_reason_and_is_not_reported() {
        let src = "struct S { p: *mut u8 }\n// SAFETY: owned\nunsafe impl Send for S {}\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn a_safe_negative_or_unknown_impl_is_not_read() {
        let src = "struct S { a: Rc<u8> }\nimpl !Send for S {}\nstruct T { a: Rc<u8> }\nunsafe impl Send for Other {}\nunsafe impl Foo for T {}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn two_types_of_one_name_are_skipped() {
        let src = "mod a { pub struct S { x: Rc<u8> } }\nmod b { pub struct S { x: u8 } }\nunsafe impl Send for S {}\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn an_enum_variant_field_and_a_cfg_test_impl_are_read_correctly() {
        let en = "enum E { A(Rc<u8>), B }\nunsafe impl Send for E {}\n";
        assert_eq!(found(en).len(), 1);
        let test =
            "struct S { a: Rc<u8> }\n#[cfg(test)]\nmod t { unsafe impl Send for super::S {} }\n";
        assert!(found(test).is_empty());
    }

    #[test]
    fn every_finding_validates() {
        for f in found("struct S { a: Rc<u8> }\nunsafe impl Send for S {}\n") {
            f.validate().unwrap();
        }
    }
}
