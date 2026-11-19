// SPDX-License-Identifier: MIT OR Apache-2.0
//! ERR-007: a resource is released on the error path too.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. In one block, a `let x = ..` binds a value,
//! a later statement of the same block releases it by hand (`x.close()`, `x.unlock()`, `x.release()`, `x.free()`, `x.disconnect()`,
//! or a call of `close`, `fclose`, `free`, `dealloc`, `CloseHandle`, `unlock`, `release` with `x`, `&x` or `&mut x` as an argument, also as the
//! only statement of an `unsafe { .. }` block),
//! and a statement **between them** can leave the function early: it holds a `?` or a `return` (a nested closure is another function and
//! is not read). The early exit skips the release, so the resource lives until the process ends or is closed by a `Drop` the rule cannot
//! see. One **Medium**-severity, Low-confidence finding per release, at the release call.
//!
//! **What it cannot see:** whether the type of `x` already releases itself on drop (most Rust types do, and then the manual call is
//! redundant rather than missing, which is why the confidence is Low), a release through a guard, `defer`-style crates or a `finally`
//! of another language, a release in a different block or function, and a `break` or `continue` that skips it. A file `syn` cannot
//! parse is named in a note, never read as clean. Exception in 0.47.12: when it is the repository's only Rust file the run says `not applicable` and does not name it (fixed in 0.47.14).

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::ERR_007;

const RELEASE_METHODS: &[&str] = &["close", "unlock", "release", "free", "disconnect"];
const RELEASE_FNS: &[&str] = &[
    "close",
    "fclose",
    "free",
    "dealloc",
    "CloseHandle",
    "unlock",
    "release",
];

/// The single identifier an expression names directly: `x`, `&x`, `&mut x`.
fn ident_of(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Path(p) => p.path.get_ident().map(ToString::to_string),
        syn::Expr::Reference(r) => ident_of(&r.expr),
        syn::Expr::Paren(p) => ident_of(&p.expr),
        _ => None,
    }
}

/// The resource a statement releases by hand, and the line of the call.
fn release(stmt: &syn::Stmt) -> Option<(String, u32)> {
    let expr = match stmt {
        syn::Stmt::Expr(e, _) => e,
        syn::Stmt::Local(l) => &l.init.as_ref()?.expr,
        _ => return None,
    };
    // `x.close()`, optionally `.unwrap()`-ed, `?`-ed or awaited
    let mut e = expr;
    loop {
        match e {
            syn::Expr::Try(t) => e = &t.expr,
            syn::Expr::Await(a) => e = &a.base,
            syn::Expr::Paren(p) => e = &p.expr,
            // `unsafe { libc::close(h) }`: the usual shape of an FFI release
            syn::Expr::Unsafe(u) if u.block.stmts.len() == 1 => match &u.block.stmts[0] {
                syn::Stmt::Expr(inner, _) => e = inner,
                _ => break,
            },
            syn::Expr::MethodCall(m)
                if matches!(m.method.to_string().as_str(), "unwrap" | "expect" | "ok") =>
            {
                e = &m.receiver;
            }
            _ => break,
        }
    }
    match e {
        syn::Expr::MethodCall(m) if RELEASE_METHODS.contains(&m.method.to_string().as_str()) => {
            Some((
                ident_of(&m.receiver)?,
                u32::try_from(m.method.span().start().line).unwrap_or(0),
            ))
        }
        syn::Expr::Call(c) => {
            let syn::Expr::Path(p) = &*c.func else {
                return None;
            };
            let last = p.path.segments.last()?.ident.to_string();
            if !RELEASE_FNS.contains(&last.as_str()) {
                return None;
            }
            let name = c.args.iter().find_map(ident_of)?;
            Some((name, u32::try_from(c.func.span().start().line).unwrap_or(0)))
        }
        _ => None,
    }
}

/// Whether a statement can leave the function: a `?` or a `return` outside nested closures and items.
fn exits_early(stmt: &syn::Stmt) -> bool {
    struct Exit(bool);
    impl<'ast> Visit<'ast> for Exit {
        fn visit_expr_try(&mut self, _: &'ast syn::ExprTry) {
            self.0 = true;
        }
        fn visit_expr_return(&mut self, _: &'ast syn::ExprReturn) {
            self.0 = true;
        }
        fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
        fn visit_item(&mut self, _: &'ast syn::Item) {}
    }
    let mut v = Exit(false);
    v.visit_stmt(stmt);
    v.0
}

fn binds(stmt: &syn::Stmt, name: &str) -> bool {
    let syn::Stmt::Local(l) = stmt else {
        return false;
    };
    fn pat_binds(p: &syn::Pat, name: &str) -> bool {
        match p {
            syn::Pat::Ident(i) => i.ident == name,
            syn::Pat::Type(t) => pat_binds(&t.pat, name),
            _ => false,
        }
    }
    pat_binds(&l.pat, name)
}

struct Site {
    line: u32,
    function: String,
    resource: String,
}

struct Walk {
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
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
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        for (j, stmt) in b.stmts.iter().enumerate() {
            let Some((name, line)) = release(stmt) else {
                continue;
            };
            let Some(i) = b.stmts[..j].iter().rposition(|s| binds(s, &name)) else {
                continue;
            };
            if b.stmts[i + 1..j].iter().any(exits_early) {
                self.sites.push(Site {
                    line,
                    function: self
                        .functions
                        .last()
                        .cloned()
                        .unwrap_or_else(|| "<module>".into()),
                    resource: name,
                });
            }
        }
        syn::visit::visit_block(self, b);
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
            let mut walk = Walk {
                functions: Vec::new(),
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Low,
                        view.name(),
                        format!(
                            "`{}` in `{}` is released by hand after a `?` or `return` that can skip the release",
                            site.resource, site.function
                        ),
                        "A resource freed only on the success path leaks when the code between acquiring and releasing it fails. Tie the release to scope exit: hold it in a type that releases on drop, or a guard, instead of calling the release at the end.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.resource))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that pairs a binding with its manual release and the early exits between them"
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
    fn a_question_mark_or_return_between_the_binding_and_the_release_is_reported() {
        let src = "pub fn f() -> Result<(), E> {
    let conn = connect()?;
    conn.send(1)?;
    conn.close();
    Ok(())
}
pub fn g(fd: i32) -> i32 {
    let h = open();
    if bad() { return -1; }
    unsafe { libc::close(h) };
    unsafe { close(&h) };
    0
}
";
        let f = found(src);
        // `f`: the `?` on line 3 skips `conn.close()`. `g`: the `return` skips `close(&h)` (line 11); both releases sit in single-statement `unsafe` blocks, which are read through.
        let lines: Vec<_> = f
            .iter()
            .map(|x| x.location.as_ref().unwrap().line.unwrap())
            .collect();
        assert!(lines.contains(&4), "{lines:?}");
        assert!(lines.contains(&10) && lines.contains(&11), "{lines:?}");
    }

    #[test]
    fn a_release_with_no_early_exit_before_it_is_not_reported() {
        let src = "pub fn f() {
    let conn = connect();
    conn.send(1);
    conn.close();
}
pub fn g() -> Result<(), E> {
    let conn = connect()?;
    let r = conn.send(1);
    conn.close();
    r
}
pub fn h() -> Result<(), E> {
    let work = || -> Result<(), E> { step()?; Ok(()) };
    let conn = connect2();
    let _ = work();
    conn.close();
    Ok(())
}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn f() -> Result<(), E> { let c = open(); step()?; c.close(); Ok(()) } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        let src = "pub fn f() -> Result<(), E> { let c = open(); step()?; c.close(); Ok(()) }\n";
        for f in found(src) {
            f.validate().unwrap();
        }
    }
}

/// Claims audit (2026-11-17): one test per claim of the module documentation, the catalog `checked_today` and the changelog entry.
#[cfg(test)]
mod claims {
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

    fn body(release: &str) -> String {
        format!("pub fn f() -> Result<(), E> {{\n    let x = open();\n    step()?;\n    {release}\n    Ok(())\n}}\n")
    }

    #[test]
    fn every_documented_release_method_and_function_form_is_read() {
        for m in ["close", "unlock", "release", "free", "disconnect"] {
            assert_eq!(found(&body(&format!("x.{m}();"))).len(), 1, "method {m}");
        }
        for func in [
            "close",
            "fclose",
            "free",
            "dealloc",
            "CloseHandle",
            "unlock",
            "release",
        ] {
            for arg in ["x", "&x", "&mut x"] {
                assert_eq!(
                    found(&body(&format!("{func}({arg});"))).len(),
                    1,
                    "{func}({arg})"
                );
            }
        }
        assert!(
            found(&body("x.send();")).is_empty(),
            "positive control: another method is not a release"
        );
    }

    #[test]
    fn a_release_may_be_unwrapped_question_marked_awaited_or_in_an_unsafe_block() {
        for r in [
            "x.close().unwrap();",
            "x.close().expect(\"c\");",
            "x.close()?;",
            "x.close().await;",
            "let _ = x.close();",
            "unsafe { free(x) };",
            "x.close().ok();",
        ] {
            assert_eq!(found(&body(r)).len(), 1, "{r}");
        }
    }

    #[test]
    fn the_binding_may_be_mutable_or_typed_and_only_the_same_block_counts() {
        let typed =
            "pub fn f() -> Result<(), E> { let mut x: Conn = open(); step()?; x.close(); Ok(()) }";
        assert_eq!(found(typed).len(), 1);
        let inner =
            "pub fn f() -> Result<(), E> { let x = open(); step()?; { x.close(); } Ok(()) }";
        assert!(
            found(inner).is_empty(),
            "a release in an inner block is another block: {:?}",
            found(inner)
        );
        let other = "pub fn f() -> Result<(), E> { let y = open(); step()?; x.close(); Ok(()) }";
        assert!(
            found(other).is_empty(),
            "a release of something this block did not bind"
        );
    }

    #[test]
    fn a_return_or_question_mark_between_skips_and_a_closure_or_item_between_does_not() {
        assert_eq!(
            found("pub fn f() { let x = open(); if bad() { return; } x.close(); }").len(),
            1
        );
        assert!(found(
            "pub fn f() { let x = open(); let c = || -> R { step()?; Ok(()) }; x.close(); }"
        )
        .is_empty());
        assert!(
            found("pub fn f() { let x = open(); fn g() -> R { step()?; Ok(()) } x.close(); }")
                .is_empty()
        );
    }

    #[test]
    fn a_finding_is_medium_severity_low_confidence_at_the_release_call() {
        let f = found(&body("x.close();"));
        assert_eq!(
            (f[0].severity, f[0].confidence),
            (Severity::Medium, Confidence::Low)
        );
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
        assert!(f[0].subject.as_deref().unwrap().ends_with(":f:x"));
    }
}
