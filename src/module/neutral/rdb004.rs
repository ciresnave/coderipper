// SPDX-License-Identifier: MIT OR Apache-2.0
//! RDB-004: a function keeps its nesting, its parameter list and its flag arguments small.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` and `#[test]` items) is parsed with `syn`. Per function, one **Low**
//! finding at Medium confidence for each of three measures, at the function:
//!
//! 1. **Parameters:** more than [`MAX_PARAMS`] (7, clippy's `too_many_arguments` default), `self` not counted. Not read in the methods
//!    of a trait impl, whose signature the trait dictates.
//! 2. **Nesting:** control flow nested [`MAX_NESTING`] (5) or more deep. `if`, `match`, `loop`, `while` and `for` each add one level;
//!    an `else if` is the same level as the `if` it continues, a closure adds none, a function or item inside the body is its own
//!    function and is measured on its own. Read in every function, trait impls included.
//! 3. **Flag argument:** a parameter of type `bool` that the body uses directly as a condition: `if flag`, `if !flag`, `while flag`
//!    or `match flag`. A flag combined with something else (`if flag && x`) is not read. Not read in the methods of a trait impl.
//!
//! The numbers are the conservative end of the ones in common use (clippy's parameter limit; a nesting figure above the three or
//! four that reading guides name), chosen so the rule reports the functions that are plainly over. They are signals, not verdicts.
//! The subject is `name` for a free function and `Type::name` for a method; an allowlist entry for it silences all three measures.
//!
//! **What it cannot see:** the body of a macro (`vec![..]`, `matches!`), generated code, a `bool` hidden behind a type alias, a flag
//! read only through a variable it was copied into, a name shadowed inside the body, and any cost that is not shape (a long but flat
//! function). A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::RDB_004;

/// More parameters than this (not counting `self`) is reported.
const MAX_PARAMS: usize = 7;

/// Control flow nested this deep or deeper is reported.
const MAX_NESTING: usize = 5;

/// The deepest control-flow nesting of a function body.
#[derive(Default)]
struct Nest {
    depth: usize,
    max: usize,
}

impl Nest {
    fn enter(&mut self, f: impl FnOnce(&mut Self)) {
        self.depth += 1;
        self.max = self.max.max(self.depth);
        f(self);
        self.depth -= 1;
    }

    /// An `if` and the `else if`s after it share one level.
    fn if_chain(&mut self, e: &syn::ExprIf) {
        self.visit_expr(&e.cond);
        self.visit_block(&e.then_branch);
        match e.else_branch.as_ref().map(|(_, b)| &**b) {
            Some(syn::Expr::If(next)) => self.if_chain(next),
            Some(other) => self.visit_expr(other),
            None => {}
        }
    }
}

impl<'ast> Visit<'ast> for Nest {
    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        self.enter(|s| s.if_chain(e));
    }

    fn visit_expr_match(&mut self, e: &'ast syn::ExprMatch) {
        self.enter(|s| syn::visit::visit_expr_match(s, e));
    }

    fn visit_expr_loop(&mut self, e: &'ast syn::ExprLoop) {
        self.enter(|s| syn::visit::visit_expr_loop(s, e));
    }

    fn visit_expr_while(&mut self, e: &'ast syn::ExprWhile) {
        self.enter(|s| syn::visit::visit_expr_while(s, e));
    }

    fn visit_expr_for_loop(&mut self, e: &'ast syn::ExprForLoop) {
        self.enter(|s| syn::visit::visit_expr_for_loop(s, e));
    }

    /// A function inside a body is measured on its own.
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

/// The parameter names (`bool` ones) the body uses directly as a condition.
struct Flags<'a> {
    names: &'a [String],
    used: Vec<String>,
}

impl Flags<'_> {
    /// The name when the expression is a bare identifier, a negation of one, or parenthesised.
    fn bare(e: &syn::Expr) -> Option<String> {
        match e {
            syn::Expr::Paren(p) => Self::bare(&p.expr),
            syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Not(_)) => Self::bare(&u.expr),
            syn::Expr::Path(p) if p.qself.is_none() => p.path.get_ident().map(ToString::to_string),
            _ => None,
        }
    }

    fn note(&mut self, e: &syn::Expr) {
        if let Some(n) = Self::bare(e) {
            if self.names.contains(&n) && !self.used.contains(&n) {
                self.used.push(n);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Flags<'_> {
    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        self.note(&e.cond);
        syn::visit::visit_expr_if(self, e);
    }

    fn visit_expr_while(&mut self, e: &'ast syn::ExprWhile) {
        self.note(&e.cond);
        syn::visit::visit_expr_while(self, e);
    }

    fn visit_expr_match(&mut self, e: &'ast syn::ExprMatch) {
        self.note(&e.expr);
        syn::visit::visit_expr_match(self, e);
    }

    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

fn is_bool(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.qself.is_none() && p.path.is_ident("bool"))
}

struct Hit {
    line: u32,
    summary: String,
    subject: String,
}

#[derive(Default)]
struct Walk {
    hits: Vec<Hit>,
    /// The `Type` of the impl block being read, for the subject of a method.
    owner: Option<String>,
}

impl Walk {
    fn function(
        &mut self,
        sig: &syn::Signature,
        block: &syn::Block,
        attrs: &[syn::Attribute],
        in_trait_impl: bool,
    ) {
        if is_cfg_test(attrs) || attrs.iter().any(|a| a.path().is_ident("test")) {
            return;
        }
        let line = u32::try_from(sig.span().start().line).unwrap_or(0);
        let name = sig.ident.to_string();
        let subject = match &self.owner {
            Some(t) => format!("{t}::{name}"),
            None => name,
        };
        let mut push = |summary: String| {
            self.hits.push(Hit {
                line,
                summary,
                subject: subject.clone(),
            });
        };
        let typed: Vec<(&syn::Pat, &syn::Type)> = sig
            .inputs
            .iter()
            .filter_map(|a| match a {
                syn::FnArg::Typed(t) => Some((&*t.pat, &*t.ty)),
                syn::FnArg::Receiver(_) => None,
            })
            .collect();
        if !in_trait_impl && typed.len() > MAX_PARAMS {
            push(format!(
                "`{subject}` takes {} parameters (more than {MAX_PARAMS}, `self` not counted)",
                typed.len(),
            ));
        }
        let mut nest = Nest::default();
        nest.visit_block(block);
        if nest.max >= MAX_NESTING {
            push(format!(
                "`{subject}` nests control flow {} levels deep ({MAX_NESTING} or more is reported)",
                nest.max,
            ));
        }
        if !in_trait_impl {
            let names: Vec<String> = typed
                .iter()
                .filter(|(_, ty)| is_bool(ty))
                .filter_map(|(p, _)| match p {
                    syn::Pat::Ident(i) => Some(i.ident.to_string()),
                    _ => None,
                })
                .collect();
            if !names.is_empty() {
                let mut flags = Flags {
                    names: &names,
                    used: Vec::new(),
                };
                flags.visit_block(block);
                if !flags.used.is_empty() {
                    push(format!(
                        "`{subject}` switches its behaviour on the `bool` argument{} {}",
                        if flags.used.len() == 1 { "" } else { "s" },
                        flags
                            .used
                            .iter()
                            .map(|n| format!("`{n}`"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ));
                }
            }
        }
    }
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map_or_else(|| "?".into(), |s| s.ident.to_string()),
        syn::Type::Reference(r) => type_name(&r.elem),
        _ => "?".into(),
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let owner = self.owner.take();
        self.function(&i.sig, &i.block, &i.attrs, false);
        syn::visit::visit_item_fn(self, i);
        self.owner = owner;
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let owner = self.owner.replace(type_name(&i.self_ty));
        let in_trait = i.trait_.is_some();
        for item in &i.items {
            if let syn::ImplItem::Fn(f) = item {
                self.function(&f.sig, &f.block, &f.attrs, in_trait);
            }
        }
        // a function or module declared inside a method body is reached through the default walk, owned by no type
        self.owner = None;
        syn::visit::visit_item_impl(self, i);
        self.owner = owner;
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let owner = self.owner.replace(i.ident.to_string());
        for item in &i.items {
            if let syn::TraitItem::Fn(f) = item {
                if let Some(block) = &f.default {
                    self.function(&f.sig, block, &f.attrs, false);
                }
            }
        }
        self.owner = owner;
    }
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
        for s in &p.sources {
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for hit in walk.hits {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Low,
                        Confidence::Medium,
                        view.name(),
                        hit.summary,
                        "Use early exits to cut nesting, group related parameters into a type, and replace a flag argument with two functions or an enum. A limit is a signal: allowlist the function with the reason when the shape is right.",
                    )
                    .location(Location::new(s.path.as_str(), Some(hit.line)))
                    .subject(hit.subject)
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads function signatures and bodies"
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

    fn deep(levels: usize) -> String {
        let mut s = String::from("fn f(x: u8) {\n");
        for _ in 0..levels {
            s.push_str("if x > 0 {\n");
        }
        s.push_str("let _ = x;\n");
        for _ in 0..levels {
            s.push_str("}\n");
        }
        s.push_str("}\n");
        s
    }

    #[test]
    fn eight_parameters_are_reported_and_seven_are_not() {
        let f = found("fn a(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) {}\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("8 parameters"), "{}", f[0].summary);
        assert_eq!(f[0].subject.as_deref(), Some("a"));
        assert!(found("fn a(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8) {}\n").is_empty());
    }

    #[test]
    fn self_is_not_counted_and_a_trait_impl_signature_is_not_judged() {
        let src = "struct S;\nimpl S { fn m(&self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8) {} }\ntrait T { fn t(&self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8); }\nimpl T for S { fn t(&self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) {} }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn nesting_of_five_is_reported_and_four_is_not() {
        let f = found(&deep(5));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("5 levels"), "{}", f[0].summary);
        assert!(found(&deep(4)).is_empty());
    }

    #[test]
    fn match_and_loops_count_and_else_if_does_not() {
        let src = "fn f(x: u8, v: Vec<u8>) {\n for a in &v {\n while *a > 0 {\n loop {\n match x {\n 0 => { if x > 1 { } }\n _ => {}\n }\n break;\n }\n }\n }\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("5 levels"), "{}", f[0].summary);
        let chain = "fn g(x: u8) { if x == 0 {} else if x == 1 {} else if x == 2 {} else if x == 3 {} else if x == 4 {} else if x == 5 {} else {} }\n";
        assert!(found(chain).is_empty());
    }

    #[test]
    fn a_closure_adds_no_level_and_an_inner_function_is_its_own() {
        let src = "fn f(x: u8) {\n if x > 0 { if x > 1 { if x > 2 {\n let c = || { if x > 3 { } };\n c();\n fn inner(y: u8) { if y > 0 { if y > 1 { if y > 2 { if y > 3 { if y > 4 {} } } } } }\n } } }\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("inner"));
    }

    #[test]
    fn a_flag_argument_switching_behaviour_is_reported() {
        let f = found("fn run(go: bool) -> u8 { if go { 1 } else { 2 } }\nfn neg(go: bool) -> u8 { if !go { 1 } else { 2 } }\n");
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f[0].summary.contains("`go`"), "{}", f[0].summary);
    }

    #[test]
    fn a_bool_that_is_only_passed_on_or_combined_is_not_a_flag() {
        let src = "fn pass(go: bool) { other(go); }\nfn both(go: bool, x: u8) -> u8 { if go && x > 0 { 1 } else { 2 } }\nfn store(go: bool) -> S { S { go } }\nfn other(_: bool) {}\nstruct S { go: bool }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn methods_are_named_with_their_type_and_test_code_is_skipped() {
        let src = "struct S;\nimpl S { fn m(&self, go: bool) { if go {} } }\n#[cfg(test)]\nmod t { fn x(go: bool) { if go {} } }\n#[test]\nfn y(go: bool) { if go {} }\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("S::m"));
    }

    #[test]
    fn a_trait_default_body_is_read_and_a_trait_impl_is_read_for_nesting_only() {
        let src = format!(
            "trait T {{ fn d(&self, go: bool) {{ if go {{}} }} }}\nstruct S;\nimpl T for S {{ fn d(&self, go: bool) {{ if go {{}} }} }}\nimpl S {{}}\n{}",
            deep(5).replace("fn f(", "fn deep(")
        );
        let f = found(&src);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("T::d"));
        assert_eq!(f[1].subject.as_deref(), Some("deep"));
    }

    #[test]
    fn every_finding_validates() {
        for f in found(&deep(5)) {
            f.validate().unwrap();
        }
    }
}
