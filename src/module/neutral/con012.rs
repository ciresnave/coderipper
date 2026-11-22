// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-012: a public `async fn` returns a future that is `Send`.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. A `pub` free `async fn`, and a `pub` `async fn`
//! of an inherent `impl`, is reported once when the text shows something that makes its future `!Send`:
//!
//! - **a parameter** whose type names `Rc`, is a raw pointer (`*const T`, `*mut T`) or is a shared reference to a `Cell` or `RefCell`
//!   (`&Cell<T>`, `&RefCell<T>`): an `async fn` keeps every parameter in its future, so one `!Send` parameter makes the whole future
//!   `!Send` (Medium confidence);
//! - **a local** (`let`) whose annotated type or initialiser names `Rc` (`Rc::new`, `Rc::clone`, `Rc<..>`), or casts to a raw pointer,
//!   declared before the last `.await` of the function: it is live across a suspension point (Low confidence, because a scope may end
//!   before the `.await` and `syn` cannot see that).
//!
//! One **Low**-severity finding per function, at its name. A runtime that cannot move futures between threads needs `Send` for
//! `tokio::spawn` and for every multi-threaded executor, and the error appears at the *caller's* spawn, far from the function that caused it.
//!
//! A file that says it is single-threaded on purpose is not read: its text contains `?Send` (`#[async_trait(?Send)]`), `spawn_local`,
//! `LocalSet` or `wasm_bindgen`.
//!
//! **What it cannot see:** a `!Send` type that is not `Rc` or a raw pointer (a `std` `MutexGuard`, a `RefCell` borrow guard
//! `Ref`/`RefMut`, a user type with a raw pointer inside, a `Rc` renamed by `use .. as ..`); a `!Send` value made by a called function or a
//! method; a local whose scope ends before the `.await`; `impl Future` returns and `async` blocks; whether the function is reachable from
//! outside the crate (`pub` inside a private module counts). `syn` has no types. A file it cannot parse is named in a note, never read as
//! clean. Opt-in until a read of the portfolio gives a denominator.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;

const ID: &str = crate::rule_id::CON_012;

/// Text that says the file is single-threaded on purpose.
const LOCAL: &[&str] = &["?Send", "spawn_local", "LocalSet", "wasm_bindgen"];

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

/// Why a type is not `Send`, in words, when its text says so.
fn not_send_type(ty: &syn::Type) -> Option<String> {
    struct V(Option<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_type_ptr(&mut self, _: &'ast syn::TypePtr) {
            self.0.get_or_insert_with(|| "a raw pointer".into());
        }
        fn visit_type_reference(&mut self, r: &'ast syn::TypeReference) {
            if r.mutability.is_none() {
                if let syn::Type::Path(p) = &*r.elem {
                    if let Some(last) = p.path.segments.last() {
                        if last.ident == "Cell" || last.ident == "RefCell" {
                            self.0
                                .get_or_insert_with(|| format!("a shared `&{}`", last.ident));
                        }
                    }
                }
            }
            syn::visit::visit_type_reference(self, r);
        }
        fn visit_path(&mut self, p: &'ast syn::Path) {
            if p.segments.iter().any(|s| s.ident == "Rc") {
                self.0.get_or_insert_with(|| "an `Rc`".into());
            }
            syn::visit::visit_path(self, p);
        }
    }
    let mut v = V(None);
    v.visit_type(ty);
    v.0
}

/// Why an expression makes its binding `!Send`: it builds or clones an `Rc`, or casts to a raw pointer.
fn not_send_expr(expr: &syn::Expr) -> Option<String> {
    struct V(Option<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
        fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
        fn visit_expr_cast(&mut self, c: &'ast syn::ExprCast) {
            if matches!(&*c.ty, syn::Type::Ptr(_)) {
                self.0.get_or_insert_with(|| "a raw pointer".into());
            }
            syn::visit::visit_expr_cast(self, c);
        }
        fn visit_path(&mut self, p: &'ast syn::Path) {
            if p.segments.iter().any(|s| s.ident == "Rc") {
                self.0.get_or_insert_with(|| "an `Rc`".into());
            }
            syn::visit::visit_path(self, p);
        }
    }
    let mut v = V(None);
    v.visit_expr(expr);
    v.0
}

/// The locals of a body (not those inside a nested closure or `async` block, which are other futures) with their lines, and the
/// line of the last `.await`.
#[derive(Default)]
struct Body {
    locals: Vec<(u32, String)>,
    last_await: u32,
}

impl<'ast> Visit<'ast> for Body {
    fn visit_expr_await(&mut self, a: &'ast syn::ExprAwait) {
        self.last_await = self.last_await.max(line_of(a.await_token.span));
        syn::visit::visit_expr_await(self, a);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        let from_type = match &l.pat {
            syn::Pat::Type(t) => not_send_type(&t.ty),
            _ => None,
        };
        let why = from_type.or_else(|| l.init.as_ref().and_then(|init| not_send_expr(&init.expr)));
        if let Some(why) = why {
            let line = syn::spanned::Spanned::span(&l.let_token).start().line;
            self.locals.push((u32::try_from(line).unwrap_or(0), why));
        }
        syn::visit::visit_local(self, l);
    }

    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}

struct Site {
    line: u32,
    function: String,
    why: String,
    confidence: Confidence,
}

fn check_fn(sig: &syn::Signature, block: &syn::Block, out: &mut Vec<Site>) {
    if sig.asyncness.is_none() {
        return;
    }
    let function = sig.ident.to_string();
    let line = line_of(sig.ident.span());
    for input in &sig.inputs {
        if let syn::FnArg::Typed(t) = input {
            if let Some(why) = not_send_type(&t.ty) {
                out.push(Site {
                    line,
                    function,
                    why: format!("a parameter holds {why}"),
                    confidence: Confidence::Medium,
                });
                return;
            }
        }
    }
    let mut body = Body::default();
    body.visit_block(block);
    if let Some((at, why)) = body
        .locals
        .iter()
        .find(|(at, _)| *at < body.last_await)
        .cloned()
    {
        out.push(Site {
            line,
            function,
            why: format!("a local at line {at} holds {why} and an `.await` follows it"),
            confidence: Confidence::Low,
        });
    }
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        if matches!(i.vis, syn::Visibility::Public(_)) {
            check_fn(&i.sig, &i.block, &mut self.sites);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) || i.trait_.is_some() {
            return;
        }
        for item in &i.items {
            if let syn::ImplItem::Fn(f) = item {
                if is_cfg_test(&f.attrs) || has_test_attr(&f.attrs) {
                    continue;
                }
                if matches!(f.vis, syn::Visibility::Public(_)) {
                    check_fn(&f.sig, &f.block, &mut self.sites);
                }
            }
        }
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return super::rust_package::nothing_to_read(ID, &loaded, "the repository tracks no Rust package source outside tests, examples, tools and vendored trees");
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            if LOCAL.iter().any(|m| s.text.contains(m)) {
                continue;
            }
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Low,
                        site.confidence,
                        view.name(),
                        format!(
                            "`pub async fn {}` in `{}` returns a future that is not `Send`: {}",
                            site.function, s.path, site.why
                        ),
                        "A future that is not `Send` cannot be passed to `tokio::spawn` or run on a multi-threaded executor, and the compiler reports it at the caller. Use `Arc` instead of `Rc`, drop the value before the `.await` (end its scope), or take an owned or `Sync` type; if the function is meant for one thread, say so with `#[async_trait(?Send)]` or `spawn_local`.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}", s.path, site.function))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads public async signatures and their locals"
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
    fn a_not_send_parameter_is_reported() {
        for src in [
            "pub async fn f(x: std::rc::Rc<u8>) {}",
            "pub async fn f(x: *const u8) {}",
            "pub async fn f(x: &RefCell<u8>) {}",
            "pub async fn f(x: &Cell<u8>) {}",
            "pub async fn f(x: Vec<Rc<u8>>) {}",
        ] {
            let f = found(src);
            assert_eq!(f.len(), 1, "{src}: {f:?}");
            assert_eq!(f[0].confidence, Confidence::Medium);
        }
    }

    #[test]
    fn a_send_signature_is_clean() {
        let src = "pub async fn f(a: Arc<u8>, b: &mut RefCell<u8>, c: &str, d: Box<dyn Fn() + Send>) {}\npub async fn g(x: &Mutex<u8>) {}";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_local_rc_before_an_await_is_reported_at_low_confidence() {
        let src =
            "pub async fn f() {\n    let r = Rc::new(1);\n    other().await;\n    drop(r);\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].confidence, Confidence::Low);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(1));
        let ptr = "pub async fn f() {\n    let p = &x as *const u8;\n    other().await;\n}\n";
        assert_eq!(found(ptr).len(), 1);
    }

    #[test]
    fn a_local_rc_with_no_later_await_is_not_reported() {
        let src =
            "pub async fn f() {\n    other().await;\n    let r = Rc::new(1);\n    drop(r);\n}\n";
        assert!(found(src).is_empty());
        assert!(found("pub async fn f() { let r = Rc::new(1); drop(r); }").is_empty());
    }

    #[test]
    fn private_sync_test_and_trait_impl_functions_are_not_read() {
        let src = "async fn a(x: Rc<u8>) {}\npub fn b(x: Rc<u8>) {}\npub(crate) async fn c(x: Rc<u8>) {}\nimpl T for S { async fn d(x: Rc<u8>) {} }\n#[cfg(test)]\nmod t { pub async fn e(x: Rc<u8>) {} }\n#[tokio::test]\npub async fn g(x: Rc<u8>) {}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_pub_method_of_an_inherent_impl_is_read() {
        let src = "impl S {\n    pub async fn m(&self, x: Rc<u8>) {}\n    async fn private(&self, x: Rc<u8>) {}\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].subject.as_deref().unwrap().ends_with(":m"));
    }

    #[test]
    fn a_file_that_is_single_threaded_on_purpose_is_not_read() {
        for marker in [
            "// ?Send",
            "use tokio::task::LocalSet;",
            "use wasm_bindgen::prelude::*;",
            "// spawn_local",
        ] {
            let src = format!("{marker}\npub async fn f(x: Rc<u8>) {{}}\n");
            assert!(found(&src).is_empty(), "{marker}");
        }
    }

    #[test]
    fn a_closure_or_async_block_local_is_another_future() {
        let src = "pub async fn f() {\n    let c = || { let r = Rc::new(1); r };\n    other().await;\n}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub async fn f(x: Rc<u8>) {}") {
            f.validate().unwrap();
        }
    }
}
