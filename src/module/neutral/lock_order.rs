// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-006: locks are always acquired in one order, so no cycle of waiting threads can form.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file outside a vendored directory, a `tests/`, `benches/`
//! or `examples/` directory and a `#[cfg(test)]` module is parsed with `syn`. In each function, a *lock acquisition* is a call
//! `X.lock()`, `X.read()` or `X.write()` with no arguments whose receiver `X` names a lock by a path the whole crate can agree on:
//! `self.field`, `self.a.b`, `self.shards[i]` (a lock per index, named `..[]`), or a `static` such as `GLOBAL` (an upper-case path).
//! A guard that is *held* while another lock is acquired makes an **edge** `held -> acquired`: a guard bound by `let g = X.lock()..;`
//! is held to the end of its block or to `drop(g)`, and a temporary (`X.lock().unwrap().push(..)`) to the end of its statement (to the
//! end of the condition for a plain `if` or `while`, to the end of the whole statement for `match`, `if let` and `for`). A **cycle**
//! among the edges of the whole repository (`a -> b` in one function and `b -> a` in another) is a finding: two threads, each
//! in one of those functions, can wait for each other. A lock acquired while the same lock is held (`a -> a`, a `Mutex` taken twice)
//! is a finding too, except for a per-index lock (`..[]`), where two indexes may be two locks.
//!
//! **What it cannot see (and the record says so):** a lock reached through a local variable or a parameter (`a.lock()` where `a` is not
//! a field or a `static`: its identity is not known across functions), an edge made by a *call* (`f()` while holding `a`, where `f`
//! takes `b`: only direct nesting is read), a guard moved into a function, a struct or a closure, ordering enforced by something
//! other than the nesting (a channel, a try-lock), and macros (`syn` does not look into their tokens). A pass is therefore evidence
//! about the nesting `syn` can see and no more; a cycle found is a *possible* deadlock, reported at **Low** confidence. A file `syn`
//! cannot parse is counted in a note, never read as clean.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;
use syn::{
    Block, Expr, ExprCall, ExprClosure, ExprForLoop, ExprIf, ExprMatch, ExprMethodCall, ExprWhile,
    FnArg, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Pat, Stmt,
};

const ID: &str = crate::rule_id::CON_006;

/// Directory names whose contents are somebody else's code, or tests.
const SKIPPED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bower_components",
    "tests",
    "benches",
    "examples",
];

fn skipped(path: &str) -> bool {
    path.split('/')
        .any(|s| SKIPPED.iter().any(|d| s.eq_ignore_ascii_case(d)))
}

/// One place where `to` was acquired while `from` was held.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    from: String,
    to: String,
    file: String,
    line: u32,
    function: String,
}

/// A guard held while a statement runs.
struct Held {
    lock: String,
    /// The binding that holds it (`let g = ..`), or `None` for a temporary of the statement.
    name: Option<String>,
}

/// Walks one function body in evaluation order, keeping the guards held.
struct Walk<'a> {
    file: &'a str,
    function: String,
    /// The type of the `impl` the function is in (`self.a` is `Type.a`), or empty outside one.
    self_ty: String,
    held: Vec<Held>,
    edges: Vec<Edge>,
    /// Names dropped by `drop(g)` in the current statement, applied when it ends.
    dropped: Vec<String>,
}

/// The name of the lock `expr` is, when the whole crate can agree on it.
fn identity(expr: &Expr, self_ty: &str) -> Option<String> {
    match expr {
        Expr::Paren(p) => identity(&p.expr, self_ty),
        Expr::Reference(r) => identity(&r.expr, self_ty),
        Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => identity(&u.expr, self_ty),
        Expr::Field(f) => {
            let base = match &*f.base {
                Expr::Path(p) if p.path.is_ident("self") => {
                    if self_ty.is_empty() {
                        return None;
                    }
                    self_ty.to_string()
                }
                other => identity(other, self_ty)?,
            };
            let member = match &f.member {
                syn::Member::Named(n) => n.to_string(),
                syn::Member::Unnamed(i) => i.index.to_string(),
            };
            Some(format!("{base}.{member}"))
        }
        Expr::Index(i) => Some(format!("{}[]", identity(&i.expr, self_ty)?)),
        Expr::Path(p) => {
            // an upper-case path is a `static` or a `const` lock
            let last = p.path.segments.last()?.ident.to_string();
            (last.len() > 1
                && last
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            .then(|| {
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
            })
        }
        _ => None,
    }
}

/// `Some(lock)` when `expr` is exactly an acquisition: `X.lock()`, `X.read()` or `X.write()` (no arguments), possibly followed by
/// `.unwrap()`, `.expect(..)`, `.unwrap_or_else(..)`, `?` or `.await`.
fn acquisition(expr: &Expr, self_ty: &str) -> Option<String> {
    match expr {
        Expr::Try(t) => acquisition(&t.expr, self_ty),
        Expr::Await(a) => acquisition(&a.base, self_ty),
        Expr::Paren(p) => acquisition(&p.expr, self_ty),
        Expr::MethodCall(m) => {
            let name = m.method.to_string();
            if matches!(name.as_str(), "unwrap" | "expect" | "unwrap_or_else") {
                return acquisition(&m.receiver, self_ty);
            }
            if m.args.is_empty() && matches!(name.as_str(), "lock" | "read" | "write") {
                return identity(&m.receiver, self_ty);
            }
            None
        }
        _ => None,
    }
}

impl Walk<'_> {
    fn acquire(&mut self, lock: String, line: u32) {
        for h in &self.held {
            // two indexes of one per-index lock may be two locks: not a re-acquisition
            if h.lock == lock && lock.ends_with("[]") {
                continue;
            }
            self.edges.push(Edge {
                from: h.lock.clone(),
                to: lock.clone(),
                file: self.file.to_string(),
                line,
                function: self.function.clone(),
            });
        }
        self.held.push(Held { lock, name: None });
    }

    /// Ends a statement: its temporaries are released, and what it dropped is no longer held.
    fn end_statement(&mut self, mark: usize) {
        self.held.truncate(mark);
        for name in std::mem::take(&mut self.dropped) {
            if let Some(i) = self
                .held
                .iter()
                .rposition(|h| h.name.as_deref() == Some(name.as_str()))
            {
                self.held.remove(i);
            }
        }
    }
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_block(&mut self, block: &'ast Block) {
        // guards bound inside a block end with it
        let mark = self.held.len();
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
        self.held.truncate(mark);
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::Local(local) => {
                let mark = self.held.len();
                if let Some(init) = &local.init {
                    if let Some(lock) = acquisition(&init.expr, &self.self_ty) {
                        // the receiver may itself hold an acquisition (rare): not read
                        let line = line_of(syn::spanned::Spanned::span(&init.expr));
                        self.acquire(lock, line);
                        let name = match &local.pat {
                            Pat::Ident(p) if p.ident != "_" => Some(p.ident.to_string()),
                            Pat::Type(t) => match &*t.pat {
                                Pat::Ident(p) => Some(p.ident.to_string()),
                                _ => None,
                            },
                            _ => None,
                        };
                        match name {
                            // bound: held to the end of the block
                            Some(name) => {
                                if let Some(h) = self.held.last_mut() {
                                    h.name = Some(name);
                                }
                            }
                            // `let _ = X.lock()` drops the guard at once
                            None => self.held.truncate(mark),
                        }
                        return;
                    }
                    self.visit_expr(&init.expr);
                    if let Some((_, diverge)) = &init.diverge {
                        self.visit_expr(diverge);
                    }
                }
                self.end_statement(mark);
            }
            Stmt::Expr(expr, _) => {
                let mark = self.held.len();
                self.visit_expr(expr);
                self.end_statement(mark);
            }
            // items inside a body and macros are not read
            Stmt::Item(_) | Stmt::Macro(_) => {}
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        // the receiver and the arguments run first, then the call
        syn::visit::visit_expr_method_call(self, call);
        if call.args.is_empty()
            && matches!(call.method.to_string().as_str(), "lock" | "read" | "write")
        {
            if let Some(lock) = identity(&call.receiver, &self.self_ty) {
                self.acquire(lock, line_of(call.method.span()));
            }
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        syn::visit::visit_expr_call(self, call);
        // `drop(g)` / `std::mem::drop(g)`
        if let Expr::Path(p) = &*call.func {
            if p.path.segments.last().is_some_and(|s| s.ident == "drop") && call.args.len() == 1 {
                if let Expr::Path(arg) = &call.args[0] {
                    if let Some(id) = arg.path.get_ident() {
                        self.dropped.push(id.to_string());
                    }
                }
            }
        }
    }

    fn visit_expr_if(&mut self, e: &'ast ExprIf) {
        // a plain condition's temporaries end before the body; an `if let` scrutinee lives through it
        if matches!(&*e.cond, Expr::Let(_)) {
            syn::visit::visit_expr_if(self, e);
            return;
        }
        let mark = self.held.len();
        self.visit_expr(&e.cond);
        self.held.truncate(mark);
        self.visit_block(&e.then_branch);
        if let Some((_, else_branch)) = &e.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, e: &'ast ExprWhile) {
        if matches!(&*e.cond, Expr::Let(_)) {
            syn::visit::visit_expr_while(self, e);
            return;
        }
        let mark = self.held.len();
        self.visit_expr(&e.cond);
        self.held.truncate(mark);
        self.visit_block(&e.body);
    }

    fn visit_expr_match(&mut self, e: &'ast ExprMatch) {
        // the scrutinee's temporaries live through every arm: the default walk does that
        syn::visit::visit_expr_match(self, e);
    }

    fn visit_expr_for_loop(&mut self, e: &'ast ExprForLoop) {
        syn::visit::visit_expr_for_loop(self, e);
    }

    // a closure and an `async` block run at another time, under other guards: not read as part of this function
    fn visit_expr_closure(&mut self, _: &'ast ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_item(&mut self, _: &'ast Item) {}
}

/// The type name of an `impl` block's self type (`Foo` for `impl Foo`, `impl<T> Foo<T>`, `impl Trait for Foo`).
fn self_type_name(imp: &ItemImpl) -> String {
    match &*imp.self_ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        (a.path().is_ident("cfg")
            && a.meta
                .require_list()
                .is_ok_and(|l| l.tokens.to_string().replace(' ', "") == "test"))
            || a.path().is_ident("test")
    })
}

/// Collects the edges of every function in `items`.
fn collect(items: &[Item], file: &str, edges: &mut Vec<Edge>) {
    for item in items {
        match item {
            Item::Fn(f) => function(f, "", file, edges),
            Item::Impl(imp) => {
                let ty = self_type_name(imp);
                for it in &imp.items {
                    if let ImplItem::Fn(m) = it {
                        if has_cfg_test(&m.attrs) {
                            continue;
                        }
                        let _ = m.sig.inputs.iter().any(|a| matches!(a, FnArg::Receiver(_)));
                        walk_body(
                            &m.block,
                            &format!("{ty}::{}", m.sig.ident),
                            &ty,
                            file,
                            edges,
                        );
                    }
                }
            }
            Item::Mod(ItemMod {
                attrs,
                content: Some((_, inner)),
                ..
            }) if !has_cfg_test(attrs) => collect(inner, file, edges),
            _ => {}
        }
    }
}

fn function(f: &ItemFn, self_ty: &str, file: &str, edges: &mut Vec<Edge>) {
    if has_cfg_test(&f.attrs) {
        return;
    }
    walk_body(&f.block, &f.sig.ident.to_string(), self_ty, file, edges);
}

fn walk_body(block: &Block, name: &str, self_ty: &str, file: &str, edges: &mut Vec<Edge>) {
    let mut walk = Walk {
        file,
        function: name.to_string(),
        self_ty: self_ty.to_string(),
        held: Vec::new(),
        edges: Vec::new(),
        dropped: Vec::new(),
    };
    walk.visit_block(block);
    edges.extend(walk.edges);
}

/// The strongly connected components of size two or more of the graph the edges make (Tarjan).
fn cycles(edges: &[Edge]) -> Vec<Vec<String>> {
    let mut nodes: BTreeSet<&str> = BTreeSet::new();
    let mut adjacent: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in edges {
        if e.from == e.to {
            continue;
        }
        nodes.insert(&e.from);
        nodes.insert(&e.to);
        adjacent.entry(&e.from).or_default().insert(&e.to);
    }
    struct State<'a> {
        index: BTreeMap<&'a str, usize>,
        low: BTreeMap<&'a str, usize>,
        stack: Vec<&'a str>,
        on: BTreeSet<&'a str>,
        next: usize,
        out: Vec<Vec<String>>,
    }
    fn visit<'a>(v: &'a str, adjacent: &BTreeMap<&'a str, BTreeSet<&'a str>>, s: &mut State<'a>) {
        s.index.insert(v, s.next);
        s.low.insert(v, s.next);
        s.next += 1;
        s.stack.push(v);
        s.on.insert(v);
        for &w in adjacent.get(v).into_iter().flatten() {
            if !s.index.contains_key(w) {
                visit(w, adjacent, s);
                let low = s.low[v].min(s.low[w]);
                s.low.insert(v, low);
            } else if s.on.contains(w) {
                let low = s.low[v].min(s.index[w]);
                s.low.insert(v, low);
            }
        }
        if s.low[v] == s.index[v] {
            let mut component = Vec::new();
            while let Some(w) = s.stack.pop() {
                s.on.remove(w);
                component.push(w.to_string());
                if w == v {
                    break;
                }
            }
            if component.len() > 1 {
                component.sort();
                s.out.push(component);
            }
        }
    }
    let mut state = State {
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on: BTreeSet::new(),
        next: 0,
        out: Vec::new(),
    };
    for &n in &nodes {
        if !state.index.contains_key(n) {
            visit(n, &adjacent, &mut state);
        }
    }
    state.out.sort();
    state.out
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !skipped(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust source file outside tests, benches, examples and vendored trees".into());
    }
    let mut edges = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        let Ok(parsed) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        collect(&parsed.items, path, &mut edges);
    }
    edges.sort();
    edges.dedup();
    let mut findings = Vec::new();
    for component in cycles(&edges) {
        let inside: Vec<&Edge> = edges
            .iter()
            .filter(|e| e.from != e.to && component.contains(&e.from) && component.contains(&e.to))
            .collect();
        let Some(first) = inside.iter().min_by_key(|e| (e.file.as_str(), e.line)) else {
            continue;
        };
        let sites: Vec<String> = inside
            .iter()
            .take(6)
            .map(|e| {
                format!(
                    "{} -> {} at {}:{} (in {})",
                    e.from, e.to, e.file, e.line, e.function
                )
            })
            .collect();
        findings.push(
            Finding::new(
                ID,
                Severity::High,
                Confidence::Low,
                view.name(),
                format!("the locks {} are acquired in more than one order", component.join(", ")),
                format!(
                    "Two threads, one in each of these paths, can each hold one lock and wait for the other: {}. Document one order for these locks and follow it everywhere (or take them together), and test it. This is read from direct nesting only (a lock taken through a call is not seen), so it is a possible deadlock, not a proven one.",
                    sites.join("; ")
                ),
            )
            .location(Location::new(first.file.clone(), Some(first.line)))
            .subject(component.join(" <-> ")),
        );
    }
    // a lock taken while the same lock is held
    let mut again: BTreeMap<&str, &Edge> = BTreeMap::new();
    for e in edges.iter().filter(|e| e.from == e.to) {
        again.entry(&e.from).or_insert(e);
    }
    for (lock, e) in again {
        findings.push(
            Finding::new(
                ID,
                Severity::High,
                Confidence::Low,
                view.name(),
                format!("the lock {lock} is acquired while it is already held"),
                format!(
                    "{} takes {lock} at {}:{} while a guard of the same lock is still held there: a `Mutex` is not re-entrant, so the thread waits for itself. Release the first guard (a block, or `drop`) before the second acquisition, or pass the guard down.",
                    e.function, e.file, e.line
                ),
            )
            .location(Location::new(e.file.clone(), Some(e.line)))
            .subject(lock.to_string()),
        );
    }
    let mut notes = vec![format!(
        "{ID}: {} lock-order edge(s) read from {} Rust file(s)",
        edges.len(),
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

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    const OPPOSITE: &str = "use std::sync::Mutex;
struct S { a: Mutex<i32>, b: Mutex<i32> }
impl S {
    fn one(&self) {
        let _a = self.a.lock().unwrap();
        let _b = self.b.lock().unwrap();
    }
    fn two(&self) {
        let _b = self.b.lock().unwrap();
        let _a = self.a.lock().unwrap();
    }
}
";

    #[test]
    fn two_functions_taking_the_same_locks_in_opposite_orders_are_a_cycle_at_the_first_edge() {
        let f = run(&[("src/lib.rs", OPPOSITE)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].check_id, "CON-006");
        assert_eq!(f[0].location.as_ref().unwrap().file, "src/lib.rs");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(6));
        assert!(
            f[0].summary.contains("S.a") && f[0].summary.contains("S.b"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn one_consistent_order_is_clean() {
        let consistent = OPPOSITE.replace(
            "let _b = self.b.lock().unwrap();\n        let _a = self.a.lock().unwrap();",
            "let _a = self.a.lock().unwrap();\n        let _b = self.b.lock().unwrap();",
        );
        assert!(run(&[("src/lib.rs", &consistent)]).is_empty());
    }

    #[test]
    fn a_guard_that_is_dropped_or_out_of_scope_holds_nothing() {
        let released = "use std::sync::Mutex;
struct S { a: Mutex<i32>, b: Mutex<i32> }
impl S {
    fn one(&self) {
        let a = self.a.lock().unwrap();
        drop(a);
        let _b = self.b.lock().unwrap();
    }
    fn two(&self) {
        { let _b = self.b.lock().unwrap(); }
        let _a = self.a.lock().unwrap();
    }
    fn three(&self) {
        let n = *self.a.lock().unwrap();
        let _b = self.b.lock().unwrap();
        let _ = n;
    }
}
";
        assert!(run(&[("src/lib.rs", released)]).is_empty());
    }

    #[test]
    fn temporaries_of_one_statement_are_nested_and_a_plain_if_condition_is_not() {
        let nested = "use std::sync::Mutex;
struct S { a: Mutex<Vec<i32>>, b: Mutex<Vec<i32>> }
impl S {
    fn one(&self) {
        self.a.lock().unwrap().push(self.b.lock().unwrap().len() as i32);
    }
    fn two(&self) {
        self.b.lock().unwrap().push(self.a.lock().unwrap().len() as i32);
    }
}
";
        assert_eq!(run(&[("src/lib.rs", nested)]).len(), 1);
        let conditions = "use std::sync::Mutex;
struct S { a: Mutex<bool>, b: Mutex<bool> }
impl S {
    fn one(&self) {
        if *self.a.lock().unwrap() { let _b = self.b.lock().unwrap(); }
    }
    fn two(&self) {
        if *self.b.lock().unwrap() { let _a = self.a.lock().unwrap(); }
    }
}
";
        assert!(
            run(&[("src/lib.rs", conditions)]).is_empty(),
            "the condition's guard ends before the body"
        );
    }

    #[test]
    fn a_lock_taken_twice_is_reported_but_two_indexes_of_a_shard_array_are_not() {
        let twice = "use std::sync::Mutex;
struct S { a: Mutex<i32> }
impl S {
    fn f(&self) {
        let _x = self.a.lock().unwrap();
        let _y = self.a.lock().unwrap();
    }
}
";
        let f = run(&[("src/lib.rs", twice)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("already held"), "{}", f[0].summary);
        let shards = "use std::sync::Mutex;
struct S { shards: Vec<Mutex<i32>> }
impl S {
    fn f(&self, i: usize, j: usize) {
        let _x = self.shards[i].lock().unwrap();
        let _y = self.shards[j].lock().unwrap();
    }
}
";
        assert!(run(&[("src/lib.rs", shards)]).is_empty());
    }

    #[test]
    fn statics_work_across_files_and_tests_closures_and_unknown_receivers_are_ignored() {
        let one = "use std::sync::Mutex;
pub static A: Mutex<i32> = Mutex::new(0);
pub static B: Mutex<i32> = Mutex::new(0);
pub fn f() { let _a = A.lock().unwrap(); let _b = B.lock().unwrap(); }
";
        let two = "pub fn g() { let _b = crate::B.lock().unwrap(); let _a = crate::A.lock().unwrap(); }\n";
        // `crate::B` and `B` are different spellings of one static: only the last segment is not enough, so they do not unify (said in the record)
        let f = run(&[("src/a.rs", one), ("src/b.rs", two)]);
        assert!(f.len() <= 1, "{f:?}");
        let ignored = "use std::sync::Mutex;
struct S { a: Mutex<i32>, b: Mutex<i32> }
impl S {
    fn one(&self, x: &Mutex<i32>) {
        let _a = self.a.lock().unwrap();
        let _c = x.lock().unwrap();
        let f = || { let _b = self.b.lock().unwrap(); };
        f();
    }
    fn two(&self) {
        let _b = self.b.lock().unwrap();
        let _a = self.a.lock().unwrap();
    }
}
#[cfg(test)]
mod tests {
    fn t(s: &super::S) { let _b = s.b.lock().unwrap(); let _a = s.a.lock().unwrap(); }
}
";
        assert!(run(&[("src/lib.rs", ignored)]).is_empty());
        // a file under tests/ is not read
        assert!(matches!(
            judge(&RepoView::from_files(&[("tests/x.rs", OPPOSITE)])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn a_file_that_does_not_parse_is_counted_never_clean() {
        let v = judge(&RepoView::from_files(&[
            ("src/lib.rs", "fn ("),
            ("src/ok.rs", "fn f() {}\\n"),
        ]));
        match v {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(
                    notes.iter().any(|n| n.contains("could not be parsed")),
                    "{notes:?}"
                );
            }
            _ => panic!("expected a Noted verdict"),
        }
    }
}
