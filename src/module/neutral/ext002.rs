// SPDX-License-Identifier: MIT OR Apache-2.0
//! EXT-002: client code does not test the concrete type behind an abstraction or downcast it (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories (`tests/`, `benches/`,
//! `examples/`, `build.rs`, `tests.rs`-style files) is parsed with `syn`; items under `#[cfg(test)]` and functions marked `#[test]`
//! are skipped. A site is one of:
//!
//! * a method call `.downcast_ref::<T>()`, `.downcast_mut::<T>()` or `.downcast::<T>()` (the `Any` downcasts, on `&dyn Any`,
//!   `Box<dyn Any>` and `Arc<dyn Any + Send + Sync>`), also written without the turbofish when the call has no arguments;
//! * a method call `.is::<T>()` (a type test on `dyn Any`; read only with the turbofish, since `is` alone is a common name);
//! * `TypeId::of::<T>()`, and a method call `.type_id()` (the identity of the type behind a value).
//!
//! **Not reported:** a downcast to an error type (the turbofish names a type whose last segment ends in `Error`, or is `Elapsed`):
//! `anyhow::Error`, `Box<dyn Error>` and `io::Error` are inspected this way by design, and the error type is the library's, not an
//! abstraction of the project. Code in tests. A downcast through a helper, a trait method or a macro (`syn` does not look inside macro
//! tokens). Other languages (`instanceof`, `isinstance`, type switches are not read).
//!
//! **What it cannot see (and the record says so):** the receiver's type (`x.downcast_ref::<T>()` may be a method of any crate, not
//! `Any`), whether the abstraction is the project's own, and whether the downcast is the one place that must know (a plugin registry,
//! a type-keyed map, an `as_any` bridge); those are the reasons to allow a site, with the reason written down. A file `syn` cannot
//! parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{
    Attribute, Expr, ExprCall, ExprMethodCall, GenericArgument, ImplItemFn, ItemFn, ItemMod,
    PathArguments, TraitItemFn,
};

const ID: &str = crate::rule_id::EXT_002;

#[derive(Debug, Clone)]
pub(super) struct Site {
    pub line: u32,
    /// What was written: `downcast_ref::<Foo>`, `TypeId::of::<Foo>` and so on.
    pub what: String,
    /// The function the site is in, or empty outside one.
    pub function: String,
}

#[derive(Default)]
struct Scan {
    function: String,
    out: Vec<Site>,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn is_test_attr(attr: &Attribute) -> bool {
    if attr
        .path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "test")
    {
        return true;
    }
    attr.path().is_ident("cfg")
        && attr
            .parse_args::<syn::Meta>()
            .is_ok_and(|m| m.path().is_ident("test"))
}

/// The first type argument of `<..>`, rendered as its path (`io::Error`); `_` for a type that is not a plain path.
fn first_type(a: &syn::AngleBracketedGenericArguments) -> Option<String> {
    a.args.iter().find_map(|g| match g {
        GenericArgument::Type(syn::Type::Path(t)) => Some(
            t.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        ),
        GenericArgument::Type(_) => Some("_".to_string()),
        _ => None,
    })
}

/// A downcast to an error type is how `anyhow::Error`, `Box<dyn Error>` and `io::Error` are inspected: not an abstraction of the project.
fn is_error_type(ty: &str) -> bool {
    let last = ty.rsplit("::").next().unwrap_or(ty);
    last.ends_with("Error") || last == "Elapsed"
}

impl Scan {
    fn function(&mut self, attrs: &[Attribute], name: &str, read: impl FnOnce(&mut Self)) {
        if attrs.iter().any(is_test_attr) {
            return;
        }
        let outer = std::mem::replace(&mut self.function, name.to_string());
        read(self);
        self.function = outer;
    }

    fn push(&mut self, line: u32, what: String) {
        self.out.push(Site {
            line,
            what,
            function: self.function.clone(),
        });
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        let name = f.sig.ident.to_string();
        self.function(&f.attrs, &name, |s| syn::visit::visit_item_fn(s, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        let name = f.sig.ident.to_string();
        self.function(&f.attrs, &name, |s| syn::visit::visit_impl_item_fn(s, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast TraitItemFn) {
        let name = f.sig.ident.to_string();
        self.function(&f.attrs, &name, |s| syn::visit::visit_trait_item_fn(s, f));
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let name = call.method.to_string();
        let ty = call.turbofish.as_ref().and_then(first_type);
        let no_args = call.args.is_empty();
        let hit = match name.as_str() {
            "downcast_ref" | "downcast_mut" | "downcast" if no_args => true,
            "is" if no_args && ty.is_some() => true,
            "type_id" if no_args => true,
            _ => false,
        };
        let skip = ty.as_deref().is_some_and(is_error_type);
        if hit && !skip {
            let what = match &ty {
                Some(t) => format!("{name}::<{t}>"),
                None => name,
            };
            self.push(line_of(call.method.span()), what);
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(p) = &*call.func {
            let segs = &p.path.segments;
            if segs.len() >= 2 {
                let last = &segs[segs.len() - 1];
                let owner = &segs[segs.len() - 2];
                if owner.ident == "TypeId" && last.ident == "of" {
                    let ty = match &last.arguments {
                        PathArguments::AngleBracketed(a) => first_type(a).unwrap_or_default(),
                        _ => String::new(),
                    };
                    if !is_error_type(&ty) {
                        self.push(line_of(last.ident.span()), format!("TypeId::of::<{ty}>"));
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan::default();
    scan.visit_file(&parsed);
    let mut out = scan.out;
    out.sort_by_key(|s| (s.line, s.what.clone()));
    out.dedup_by_key(|s| (s.line, s.what.clone()));
    Some(out)
}

fn vendored(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p) && !is_dev_path(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source file outside tests".into(),
        );
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        let Some(found) = view.read(path).and_then(|t| sites(&t)) else {
            unparsed.push(path);
            continue;
        };
        for site in found {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::Medium,
                    view.name(),
                    format!("`{}` tests the concrete type behind a value", site.what),
                    format!(
                        "In `{}` the code asks what type is really behind a value (`{}`). Needing to know the real type means the abstraction is not \
                         substitutable. Move the varying behaviour into the abstraction (a trait method), or model the cases as a closed enum matched \
                         exhaustively. If this is the one place that must know (a plugin registry, a type-keyed map), allow the site with that reason. \
                         Read from the syntax only: the receiver's type is not resolved, so a method named like a downcast on another type is reported too.",
                        site.function, site.what
                    ),
                )
                .location(Location::new(path.as_str(), Some(site.line)))
                .subject(format!("{}:{}:{}", path, site.function, site.what))
                .positive_control(format!(
                    "{} Rust file(s) parsed with syn; the site was read at {path}:{}",
                    files.len() - unparsed.len(),
                    site.line
                )),
            );
        }
    }
    let mut notes = vec![format!(
        "{ID}: {} Rust file(s) read with syn",
        files.len() - unparsed.len()
    )];
    if !unparsed.is_empty() {
        notes.push(format!(
            "{ID}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        ));
    }
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whats(src: &str) -> Vec<String> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| s.what)
            .collect()
    }

    #[test]
    fn the_any_downcasts_are_reported_with_their_target_type() {
        let src = "use std::any::Any;
fn f(a: &dyn Any, b: Box<dyn Any>) {
    let _ = a.downcast_ref::<Foo>();
    let _ = b.downcast::<Bar>();
}";
        assert_eq!(whats(src), vec!["downcast_ref::<Foo>", "downcast::<Bar>"]);
    }

    #[test]
    fn a_type_test_and_a_type_id_are_reported() {
        let src = "use std::any::{Any, TypeId};
fn f(a: &dyn Any) -> bool {
    a.is::<Foo>() || a.type_id() == TypeId::of::<Foo>()
}";
        let got = whats(src);
        assert!(got.contains(&"is::<Foo>".to_string()), "{got:?}");
        assert!(got.contains(&"type_id".to_string()), "{got:?}");
        assert!(got.contains(&"TypeId::of::<Foo>".to_string()), "{got:?}");
    }

    #[test]
    fn is_without_a_turbofish_and_calls_with_arguments_are_not_type_tests() {
        let src = "fn f(x: Opt) -> bool { x.is() || x.is_some() || x.downcast_ref(1) }";
        assert!(whats(src).is_empty());
    }

    #[test]
    fn a_downcast_to_an_error_type_is_how_errors_are_inspected_and_is_not_reported() {
        let src = "fn f(e: anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>().is_some() || e.downcast_ref::<MyError>().is_some()
}";
        assert!(whats(src).is_empty());
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "fn f(a: &dyn std::any::Any) {}
#[cfg(test)]
mod tests {
    fn t(a: &dyn std::any::Any) { a.downcast_ref::<Foo>(); }
}
#[test]
fn u() { Box::new(1).downcast::<Foo>(); }";
        assert!(whats(src).is_empty());
    }

    #[test]
    fn the_site_names_its_function_and_line() {
        let src = "fn outer(a: &dyn std::any::Any) {\n    a.downcast_ref::<Foo>();\n}";
        let s = sites(src).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].line, s[0].function.as_str()), (2, "outer"));
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none_not_an_empty_list() {
        assert!(sites("fn (((").is_none());
    }
}
