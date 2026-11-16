// SPDX-License-Identifier: MIT OR Apache-2.0
//! EFX-005: a method that both changes state and answers a query (Rust).
//!
//! **What this judges (Rust, from the source alone):** every method of an inherent `impl` block, in `.rs` files outside vendored
//! directories, `tests/`, `examples/`, `benches/`, `build.rs`, outside `#[cfg(test)]` items and `#[test]` functions, whose receiver is
//! `&mut self` **and** whose return type carries information: it is written, and it is not `()`, `Self`, `&mut Self`, `Result<(), E>`,
//! or the never type. Such a method is reported once, at its name, so a person can decide whether the mix is meant (command-query
//! separation: a call that changes state should not also be how a caller learns something).
//!
//! **Accepted idioms, not reported:** a method whose name says it hands back what it just took or swapped (`pop`, `take`, `remove`,
//! `swap`, `replace`, `insert`, `next`, `drain`, `split_off`, `truncate`, `get_or_insert`, `entry`, `compare_exchange`, `fetch_*`,
//! and any name starting with one of those words followed by `_`), a method whose name ends in `_mut` (an accessor that returns a
//! mutable borrow), and a method that returns a mutable reference (`&mut T`: a builder or an accessor).
//!
//! **Not read:** the methods of `impl Trait for Type` and the methods a trait declares (the trait dictates the signature); `&self`
//! methods that mutate through interior mutability (that is `EFX-007`'s ground); whether the body really changes anything (a
//! `&mut self` receiver is the proxy); a return type hidden behind an alias; macro-generated methods; other languages. A file `syn`
//! cannot parse is named in a note, never read as clean. Info severity, Low confidence, opt-in (`check EFX-005`).

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{FnArg, ImplItemFn, ItemFn, ItemMod, ReturnType, Signature, Type};

const ID: &str = crate::rule_id::EFX_005;

/// Name stems of methods that are command and query by design (the record's "pop or compare-and-swap").
const IDIOMS: &[&str] = &[
    "pop",
    "take",
    "remove",
    "swap",
    "replace",
    "insert",
    "next",
    "drain",
    "split_off",
    "truncate",
    "get_or_insert",
    "entry",
    "compare_exchange",
    "fetch",
    "push",
    "poll",
    "read",
    "recv",
    "send",
    "write",
    "advance",
    "consume",
];

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

fn is_idiom(name: &str) -> bool {
    name.ends_with("_mut")
        || IDIOMS
            .iter()
            .any(|w| name == *w || name.strip_prefix(w).is_some_and(|r| r.starts_with('_')))
}

fn is_unit(ty: &Type) -> bool {
    match ty {
        Type::Tuple(t) => t.elems.is_empty(),
        Type::Never(_) => true,
        Type::Paren(p) => is_unit(&p.elem),
        Type::Group(g) => is_unit(&g.elem),
        _ => false,
    }
}

fn is_self(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p.qself.is_none() && p.path.is_ident("Self"),
        Type::Paren(p) => is_self(&p.elem),
        Type::Group(g) => is_self(&g.elem),
        _ => false,
    }
}

/// `Result<(), E>` (or an alias spelled `Result<()>`): it reports failure, which carries no answer.
fn is_unit_result(ty: &Type) -> bool {
    let Type::Path(p) = ty else {
        return false;
    };
    let Some(last) = p.path.segments.last() else {
        return false;
    };
    if last.ident != "Result" {
        return false;
    }
    match &last.arguments {
        syn::PathArguments::AngleBracketed(a) => match a.args.first() {
            Some(syn::GenericArgument::Type(t)) => is_unit(t),
            _ => false,
        },
        _ => false,
    }
}

/// The return type is information a caller reads (not unit, not `Self`, not a mutable borrow, not a bare error signal).
fn answers_a_query(ret: &ReturnType) -> bool {
    match ret {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => {
            let mut_ref = matches!(&**ty, Type::Reference(r) if r.mutability.is_some());
            !(is_unit(ty) || is_self(ty) || mut_ref || is_unit_result(ty))
        }
    }
}

fn takes_mut_self(sig: &Signature) -> bool {
    matches!(
        sig.inputs.first(),
        Some(FnArg::Receiver(r)) if matches!(r.kind, syn::ReceiverKind::Reference(_, _, Some(_)))
    )
}

struct Site {
    name: String,
    line: u32,
}

#[derive(Default)]
struct Collect {
    owner: Vec<String>,
    sites: Vec<Site>,
    in_test: bool,
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
        // free functions have no receiver, but an impl can sit inside one
        if !self.in_test && !is_cfg_test(&i.attrs) && !has_test_attr(&i.attrs) {
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
        if self.in_test || is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        let sig = &i.sig;
        let name = sig.ident.to_string();
        if takes_mut_self(sig) && answers_a_query(&sig.output) && !is_idiom(&name) {
            self.sites.push(Site {
                name: format!("{}::{name}", self.owner.join("::")),
                line: line_of(sig.ident.span()),
            });
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
                        Severity::Info,
                        Confidence::Low,
                        view.name(),
                        format!(
                            "`{}` takes `&mut self` and returns a value: it changes state and answers a query in one call",
                            site.name
                        ),
                        "Split it into a command that returns nothing and a query that takes `&self`, unless the mix is an accepted idiom (pop, take, compare-and-swap) or is meant.",
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
    fn a_mut_self_method_that_returns_a_value_is_reported() {
        let f =
            found("struct S(u8);\nimpl S { fn bump(&mut self) -> u8 { self.0 += 1; self.0 } }\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject.as_deref(), Some("S::bump"));
    }

    #[test]
    fn commands_queries_builders_and_error_signals_are_not_reported() {
        for m in [
            "fn set(&mut self, v: u8) { self.0 = v; }",
            "fn get(&self) -> u8 { self.0 }",
            "fn with(&mut self, v: u8) -> &mut Self { self.0 = v; self }",
            "fn chain(&mut self) -> Self where Self: Sized { todo!() }",
            "fn try_set(&mut self, v: u8) -> Result<(), String> { self.0 = v; Ok(()) }",
            "fn go(&mut self) -> ! { loop {} }",
            "fn by_value(self) -> u8 { self.0 }",
        ] {
            let src = format!("struct S(u8);\nimpl S {{ {m} }}\n");
            assert!(found(&src).is_empty(), "{m}");
        }
    }

    #[test]
    fn accepted_idioms_trait_impls_and_tests_are_not_reported() {
        for m in [
            "fn pop(&mut self) -> Option<u8> { None }",
            "fn take_all(&mut self) -> Vec<u8> { vec![] }",
            "fn compare_exchange(&mut self, a: u8, b: u8) -> bool { a == b }",
            "fn slot_mut(&mut self) -> &mut u8 { &mut self.0 }",
        ] {
            let src = format!("struct S(u8);\nimpl S {{ {m} }}\n");
            assert!(found(&src).is_empty(), "{m}");
        }
        assert!(found("struct S(u8);\ntrait T { fn m(&mut self) -> u8; }\nimpl T for S { fn m(&mut self) -> u8 { 0 } }\n").is_empty());
        assert!(found("struct S(u8);\n#[cfg(test)]\nmod t { use super::S; impl S { fn m(&mut self) -> u8 { 0 } } }\n").is_empty());
    }
}
