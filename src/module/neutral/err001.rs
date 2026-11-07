// SPDX-License-Identifier: MIT OR Apache-2.0
//! ERR-001: errors are not swallowed (the Rust, syntactic half).
//!
//! **What this judges:** every tracked `.rs` file that is production source (outside vendored trees, development tooling, test
//! directories and `build.rs`; items under `#[cfg(test)]` or `#[test]` and `impl Drop` blocks are skipped) is parsed with `syn`.
//! Five ways of dropping a failure are found, each at **Medium** severity and confidence, because `syn` has no types:
//!
//! 1. `let _ = call();` where the value is a call or a method call (a `?`, a macro and a plain path are not);
//! 2. `_ = call();`;
//! 3. `call().ok();` as a statement (the `Result` becomes an `Option` that is thrown away);
//! 4. a match arm `Err(_) => {}` (or `=> ()`);
//! 5. `if let Ok(..) = call` with **no `else`**, when the call is one of a short named list whose failure matters: file-system,
//!    file, socket and `Command` calls and the read, write and flush methods.
//!
//! **A comment is the stated reason the rule asks for.** A comment on the same line, on the lines up to the end of the statement, or
//! alone on the line above silences the site. This is the opposite of `lint-suppression`, where a comment is deliberately not a
//! reason: here the rule's own remedy is to record why dropping the failure is safe at that exact spot.
//!
//! **Already covered, so not repeated:** a discarded `Result` from a call as a bare statement (`call();`) is `unused-return-values`
//! and the compiler's `unused_must_use`; related rule `ERR-011`.
//!
//! **What it cannot see:** another language (the catalog rule also names TypeScript and Python); whether the dropped value is a
//! `Result` at all (`let _ = lock.lock();` and `let _ = tx.send(x);` read the same); a drop through a macro (`let _ = write!(..)`);
//! a function that swallows internally; a file `syn` cannot parse (counted in a note, never read as clean).

use super::rust_src::{
    comment_beside, is_test_item, line_of, production_files, statement_end, MAX_BYTES,
};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Arm, Expr, ImplItemFn, ItemFn, ItemImpl, ItemMod, Local, Pat, Stmt};

const ID: &str = crate::rule_id::ERR_001;

/// A file without any of these cannot hold a site, so it is not parsed.
const PREFILTER: &[&str] = &["_ =", ".ok()", "Err(_)", "if let Ok"];

/// Names whose failure matters: a call or method with one of these among its path segments or method names, under `if let Ok(..)`.
const FALLIBLE: &[&str] = &[
    "fs",
    "File",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "Command",
    "read_to_string",
    "read_to_end",
    "write_all",
    "flush",
    "create_dir_all",
    "remove_file",
    "remove_dir_all",
    "rename",
    "output",
    "status",
    "spawn",
    "connect",
];

/// Methods whose result is a `bool`, an `Option`, a count or a new value, never a `Result`: `let _ = v.iter().any(..)` drops no failure.
const INFALLIBLE: &[&str] = &[
    "any",
    "all",
    "contains",
    "contains_key",
    "starts_with",
    "ends_with",
    "is_empty",
    "len",
    "insert",
    "remove",
    "push",
    "get",
    "clone",
    "to_string",
    "to_owned",
    "count",
];

#[derive(Clone, Copy)]
enum Kind {
    LetUnderscore,
    AssignUnderscore,
    OkStatement,
    EmptyErrArm,
    IfLetOkNoElse,
}

struct Hit {
    kind: Kind,
    line: u32,
    what: String,
}

struct Scan<'a> {
    lines: &'a [&'a str],
    hits: Vec<Hit>,
}

/// The call an expression is, if it is one: `path::to::f(..)` or `.method(..)`, awaited or not. A `?`, a macro and a path are not.
fn call_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Await(a) => call_name(&a.base),
        Expr::Paren(p) => call_name(&p.expr),
        Expr::Call(c) => match &*c.func {
            Expr::Path(p) => {
                let segs: Vec<String> = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let n = segs.len();
                let name = segs[n.saturating_sub(2)..].join("::");
                (name != "drop").then(|| format!("{name}(..)"))
            }
            _ => Some("a call".into()),
        },
        Expr::MethodCall(m) => {
            let method = m.method.to_string();
            (!INFALLIBLE.contains(&method.as_str())).then(|| format!(".{method}(..)"))
        }
        _ => None,
    }
}

/// Every path segment and method name in a call chain.
fn names_in(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Await(a) => names_in(&a.base, out),
        Expr::Try(t) => names_in(&t.expr, out),
        Expr::Paren(p) => names_in(&p.expr, out),
        Expr::Reference(r) => names_in(&r.expr, out),
        Expr::Call(c) => {
            names_in(&c.func, out);
            for a in &c.args {
                names_in(a, out);
            }
        }
        Expr::MethodCall(m) => {
            out.push(m.method.to_string());
            names_in(&m.receiver, out);
        }
        Expr::Path(p) => out.extend(p.path.segments.iter().map(|s| s.ident.to_string())),
        _ => {}
    }
}

fn is_ok_pattern(pat: &Pat) -> bool {
    matches!(pat, Pat::TupleStruct(t) if t.path.segments.last().is_some_and(|s| s.ident == "Ok"))
}

impl Scan<'_> {
    fn add(&mut self, kind: Kind, first: u32, last: u32, what: String) {
        if !comment_beside(self.lines, first, last) {
            self.hits.push(Hit {
                kind,
                line: first,
                what,
            });
        }
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_test_item(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !is_test_item(&f.attrs) {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        if !is_test_item(&f.attrs) {
            syn::visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        let drop_impl = i
            .trait_
            .as_ref()
            .is_some_and(|(p, _)| p.segments.last().is_some_and(|s| s.ident == "Drop"));
        if !drop_impl && !is_test_item(&i.attrs) {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_local(&mut self, l: &'ast Local) {
        if matches!(l.pat, Pat::Wild(_)) {
            if let Some(name) = l.init.as_ref().and_then(|i| call_name(&i.expr)) {
                let first = line_of(l.let_token.span);
                let last = statement_end(self.lines, first);
                self.add(Kind::LetUnderscore, first, last, name);
            }
        }
        syn::visit::visit_local(self, l);
    }

    fn visit_expr_assign(&mut self, a: &'ast syn::ExprAssign) {
        if matches!(*a.left, Expr::Infer(_)) {
            if let Some(name) = call_name(&a.right) {
                let first = line_of(a.left.span());
                let last = statement_end(self.lines, first);
                self.add(Kind::AssignUnderscore, first, last, name);
            }
        }
        syn::visit::visit_expr_assign(self, a);
    }

    fn visit_stmt(&mut self, s: &'ast Stmt) {
        if let Stmt::Expr(Expr::MethodCall(m), Some(_)) = s {
            if m.method == "ok" && m.args.is_empty() {
                let first = line_of(m.method.span());
                let last = statement_end(self.lines, first);
                self.add(Kind::OkStatement, first, last, ".ok();".into());
            }
        }
        syn::visit::visit_stmt(self, s);
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        let err_wild = matches!(&arm.pat, Pat::TupleStruct(t)
            if t.path.segments.last().is_some_and(|s| s.ident == "Err")
                && t.elems.len() == 1
                && matches!(t.elems[0], Pat::Wild(_)));
        if err_wild {
            let first = line_of(arm.pat.span());
            let empty_end = match &*arm.body {
                Expr::Block(b) if b.block.stmts.is_empty() => {
                    Some(line_of(b.block.brace_token.span.close()))
                }
                Expr::Tuple(t) if t.elems.is_empty() => Some(first),
                _ => None,
            };
            if let Some(last) = empty_end {
                self.add(Kind::EmptyErrArm, first, last, "Err(_) => {}".into());
            }
        }
        syn::visit::visit_arm(self, arm);
    }

    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        if let Expr::Let(l) = &*e.cond {
            if e.else_branch.is_none() && is_ok_pattern(&l.pat) {
                let mut names = Vec::new();
                names_in(&l.expr, &mut names);
                if let Some(n) = names.iter().find(|n| FALLIBLE.contains(&n.as_str())) {
                    let first = line_of(e.if_token.span);
                    self.add(
                        Kind::IfLetOkNoElse,
                        first,
                        first,
                        format!("if let Ok(..) = ..{n}.. with no else"),
                    );
                }
            }
        }
        syn::visit::visit_expr_if(self, e);
    }
}

fn describe(kind: Kind, what: &str) -> String {
    match kind {
        Kind::LetUnderscore => format!("`let _ = {what}` drops a possible failure with no stated reason"),
        Kind::AssignUnderscore => format!("`_ = {what}` drops a possible failure with no stated reason"),
        Kind::OkStatement => "`.ok();` as a statement turns a failure into nothing with no stated reason".into(),
        Kind::EmptyErrArm => "an `Err(_) => {}` arm ignores the failure with no stated reason".into(),
        Kind::IfLetOkNoElse => format!("`{what}`: the failure of a file, process or network call is ignored with no stated reason"),
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = production_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source outside tests, tooling and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let mut parsed = 0usize;
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        if text.len() > MAX_BYTES || !PREFILTER.iter().any(|p| text.contains(p)) {
            continue;
        }
        let Ok(file) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        parsed += 1;
        let lines: Vec<&str> = text.lines().collect();
        let mut scan = Scan {
            lines: &lines,
            hits: Vec::new(),
        };
        scan.visit_file(&file);
        for h in scan.hits {
            findings.push((path.clone(), h));
        }
    }
    findings.sort_by(|a, b| (&a.0, a.1.line).cmp(&(&b.0, b.1.line)));
    let total = files.len();
    let out: Vec<Finding> = findings
        .into_iter()
        .map(|(path, h)| {
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::Medium,
                view.name(),
                format!("{path}:{}: {}", h.line, describe(h.kind, &h.what)),
                "Handle the failure, return it to the caller, or write a comment on that line or the line above that says why dropping it is safe (for this rule a comment is the stated reason, unlike `lint-suppression`).",
            )
            .location(Location::new(path.as_str(), Some(h.line)))
            .subject(view.name())
            .positive_control(format!(
                "{total} production Rust file(s) tracked, {parsed} of them parsed for the patterns; this site is at {path}:{}",
                h.line
            ))
        })
        .collect();
    if unparsed.is_empty() {
        Verdict::Findings(out)
    } else {
        let note = format!(
            "{} Rust file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed
                .iter()
                .take(5)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        );
        Verdict::Noted(out, vec![note])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[("src/lib.rs", src)])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    #[test]
    fn each_way_of_dropping_a_failure_is_found() {
        let src = "\
fn f() {
    let _ = std::fs::remove_file(\"a\");
    _ = std::fs::remove_file(\"b\");
    std::fs::remove_file(\"c\").ok();
    match g() { Ok(_) => {}, Err(_) => {} }
    if let Ok(s) = std::fs::read_to_string(\"d\") { use_it(s); }
}
";
        let f = run(src);
        let lines: Vec<u32> = f
            .iter()
            .map(|x| x.location.as_ref().and_then(|l| l.line).unwrap())
            .collect();
        assert_eq!(lines, vec![2, 3, 4, 5, 6], "{f:?}");
    }

    #[test]
    fn a_method_that_returns_a_bool_or_an_option_drops_no_failure() {
        let src = "fn f(v: &[u8]) { let _ = v.iter().any(|x| *x == 1); let _ = set.insert(3); }
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_comment_beside_the_site_is_the_stated_reason() {
        let src = "\
fn f() {
    // the file may already be gone
    let _ = std::fs::remove_file(\"a\");
    let _ = std::fs::remove_file(\"b\"); // best effort cleanup
    match g() { Ok(_) => {}, Err(_) => { /* the peer hung up */ } }
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn not_a_drop_of_a_call_and_not_a_try_and_not_an_else_and_not_the_unlisted() {
        let src = "\
fn f() {
    let _ = x;
    let _ = g()?;
    let _ = vec![1];
    let _ = drop(h());
    let (a, _) = pair();
    if let Ok(v) = parse(\"1\") { use_it(v); }
    if let Ok(s) = std::fs::read_to_string(\"d\") { use_it(s); } else { warn(); }
    match g() { Err(e) => log(e), _ => {} }
    match g() { Err(_) => fallback(), _ => {} }
}
";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn test_code_and_drop_impls_are_skipped() {
        let src = "\
struct S;
impl Drop for S { fn drop(&mut self) { let _ = std::fs::remove_file(\"a\"); } }
#[test]
fn t() { let _ = std::fs::remove_file(\"a\"); }
#[cfg(test)]
mod tests { fn u() { let _ = std::fs::remove_file(\"a\"); } }
";
        assert!(run(src).is_empty());
        let in_tests_dir = judge(&RepoView::from_files(&[(
            "tests/a.rs",
            "fn f() { let _ = std::fs::remove_file(\"a\"); }\n",
        )]));
        assert!(matches!(in_tests_dir, Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_file_that_does_not_parse_is_noted_never_clean() {
        match judge(&RepoView::from_files(&[(
            "src/lib.rs",
            "fn ( let _ = f();",
        )])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(notes[0].contains("could not be parsed"), "{notes:?}");
            }
            _ => panic!("expected a Noted verdict"),
        }
    }
}
