// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-002, CON-003 and CON-004 (REL-003 is an alias of CON-004: one finding, both names): Rust concurrency defects read from the source alone.
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories (`tests/`, `benches/`,
//! `examples/`, `build.rs`, `tests.rs`-style files) is parsed with `syn`; items under `#[cfg(test)]` and functions marked `#[test]`
//! (including `#[tokio::test]`) are skipped. One walk reads the file once and yields three kinds of site:
//!
//! * **CON-002, a guard held across `.await`.** A guard is `X.lock()`, `X.read()` or `X.write()` (no arguments, optionally followed by
//!   `.unwrap()`, `.expect(..)`, `.unwrap_or_else(..)` or `?`) or `X.borrow()` / `X.borrow_mut()`, in an `async fn`, an `async` block
//!   or an `async` closure. A guard bound by `let g = ..;` is held to the end of its block or to `drop(g)`; a temporary to the end of
//!   its statement (through the body for a `match`, `if let`, `while let` or `for`). An `.await` evaluated while a guard is held is
//!   the finding. An acquisition that is itself awaited (`m.lock().await`, an async lock built for suspension) is not a guard of this
//!   kind. Reported at **Medium** severity and confidence: a guard the compiler knows is moved or dropped (a `drop` in a branch, a
//!   guard moved into a call) is not always visible to `syn`.
//! * **CON-003, a thread-blocking call in async code.** In an `async fn`, `async` block or `async` closure (a closure that is not
//!   `async` is not read: it is what `spawn_blocking` and `block_in_place` take), a call whose path resolves, through the file's `use`
//!   items, to `std::thread::sleep`, a function of `std::fs`, `std::net::TcpStream::connect`, a `reqwest::blocking` item,
//!   `std::process::Command::new(..)...output()` / `.status()`, or a `.block_on(..)` method call.
//! * **CON-004 (also answered to as REL-003), an unbounded channel.** A call that resolves to `std::sync::mpsc::channel`, `tokio::sync::mpsc::unbounded_channel`,
//!   `futures::channel::mpsc::unbounded`, `crossbeam_channel::unbounded` (also `crossbeam::channel`), `flume::unbounded` or
//!   `async_channel::unbounded`. A channel with a capacity (`sync_channel(n)`, `mpsc::channel(n)` of tokio, `bounded(n)`) is not one.
//!
//! **What it cannot see (and the record says so):** a guard or a blocking call reached through a helper, a trait method or a macro
//! (`syn` does not look into macro tokens); the receiver's type (`x.lock()` may be a lock of any crate; the receiver is not resolved);
//! a `.read()` or `.borrow()` that is not a guard; a guard the compiler proves is dropped before the await; spawn sites with no
//! capacity (only channels are read for CON-004); other languages. A file `syn` cannot parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Block, Expr, ExprAsync, ExprAwait, ExprCall, ExprClosure, ExprForLoop, ExprIf,
    ExprMatch, ExprMethodCall, ExprWhile, ImplItemFn, Item, ItemFn, ItemMod, ItemUse, Pat, Stmt,
    TraitItemFn, UseTree,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// CON-002
    GuardAcrossAwait,
    /// CON-003
    Blocking,
    /// CON-004 (and its alias REL-003)
    Unbounded,
}

#[derive(Debug, Clone)]
pub(super) struct Site {
    pub kind: Kind,
    pub line: u32,
    /// The guard, the blocking path or the channel constructor.
    pub what: String,
    /// The function the site is in, or empty outside one.
    pub function: String,
    /// CON-002 only: the line of the `.await` that suspends with the guard held.
    pub await_line: u32,
}

/// A guard held while a statement runs.
struct Held {
    what: String,
    line: u32,
    name: Option<String>,
    reported: bool,
}

#[derive(Default)]
struct Scan {
    /// Full path each name in scope stands for, from the file's `use` items (`sleep` -> `std::thread::sleep`).
    uses: BTreeMap<String, Vec<String>>,
    /// Whether the code being read is async (an `async fn`, `async` block or `async` closure).
    in_async: bool,
    held: Vec<Held>,
    dropped: Vec<String>,
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

fn flatten(tree: &UseTree, prefix: &mut Vec<String>, out: &mut BTreeMap<String, Vec<String>>) {
    match tree {
        UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten(&p.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(n) => {
            let mut v = prefix.clone();
            v.push(n.ident.to_string());
            out.insert(n.ident.to_string(), v);
        }
        UseTree::Rename(r) => {
            let mut v = prefix.clone();
            v.push(r.ident.to_string());
            out.insert(r.rename.to_string(), v);
        }
        UseTree::Glob(_) => {}
        UseTree::Group(g) => {
            for t in &g.items {
                flatten(t, prefix, out);
            }
        }
    }
}

impl Scan {
    /// The path of a call's function with its first segment replaced by what the file's `use` items say it stands for.
    fn resolve(&self, path: &syn::Path) -> Vec<String> {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        match segs.first().and_then(|first| self.uses.get(first)) {
            Some(full) => full
                .iter()
                .cloned()
                .chain(segs.into_iter().skip(1))
                .collect(),
            None => segs,
        }
    }

    fn function<F: FnOnce(&mut Self)>(
        &mut self,
        attrs: &[Attribute],
        name: &str,
        is_async: bool,
        walk: F,
    ) {
        if attrs.iter().any(is_test_attr) {
            return;
        }
        let saved = (
            std::mem::take(&mut self.held),
            std::mem::take(&mut self.dropped),
            self.in_async,
            std::mem::replace(&mut self.function, name.to_string()),
        );
        self.in_async = is_async;
        walk(self);
        self.held = saved.0;
        self.dropped = saved.1;
        self.in_async = saved.2;
        self.function = saved.3;
    }

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

    fn push_guard(&mut self, what: String, line: u32) {
        if self.in_async {
            self.held.push(Held {
                what,
                line,
                name: None,
                reported: false,
            });
        }
    }

    fn blocking(&mut self, what: String, line: u32) {
        if self.in_async {
            self.out.push(Site {
                kind: Kind::Blocking,
                line,
                what,
                function: self.function.clone(),
                await_line: 0,
            });
        }
    }
}

/// `Some(description)` when `expr` is exactly a guard acquisition: `X.lock()` (or `read`, `write`, `borrow`, `borrow_mut`) with no
/// arguments, then any of `.unwrap()`, `.expect(..)`, `.unwrap_or_else(..)`, `?`. An acquisition that is awaited is an async lock.
fn guard(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Try(t) => guard(&t.expr),
        Expr::Paren(p) => guard(&p.expr),
        Expr::MethodCall(m) => {
            let name = m.method.to_string();
            if matches!(name.as_str(), "unwrap" | "expect" | "unwrap_or_else") {
                return guard(&m.receiver);
            }
            (m.args.is_empty()
                && matches!(
                    name.as_str(),
                    "lock" | "read" | "write" | "borrow" | "borrow_mut"
                ))
            .then(|| format!("`.{name}()` guard"))
        }
        _ => None,
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
        self.function(&f.attrs, &name, f.sig.asyncness.is_some(), |s| {
            syn::visit::visit_item_fn(s, f);
        });
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        let name = f.sig.ident.to_string();
        self.function(&f.attrs, &name, f.sig.asyncness.is_some(), |s| {
            syn::visit::visit_impl_item_fn(s, f);
        });
    }

    fn visit_trait_item_fn(&mut self, f: &'ast TraitItemFn) {
        let name = f.sig.ident.to_string();
        self.function(&f.attrs, &name, f.sig.asyncness.is_some(), |s| {
            syn::visit::visit_trait_item_fn(s, f);
        });
    }

    fn visit_item(&mut self, item: &'ast Item) {
        // a `use` anywhere in the file names things for the whole file: collect it, then read the rest
        if let Item::Use(ItemUse { attrs, tree, .. }) = item {
            if !attrs.iter().any(is_test_attr) {
                flatten(tree, &mut Vec::new(), &mut self.uses);
            }
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_block(&mut self, block: &'ast Block) {
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
                    if let Some(what) = guard(&init.expr) {
                        // read the receiver and any arguments first (they may await), then the guard is held
                        self.visit_expr(&init.expr);
                        // the visit pushed it as a temporary; bind it to the `let` name, or release it for `let _ =`
                        let name = match &local.pat {
                            Pat::Ident(p) if p.ident != "_" => Some(p.ident.to_string()),
                            Pat::Type(t) => match &*t.pat {
                                Pat::Ident(p) => Some(p.ident.to_string()),
                                _ => None,
                            },
                            _ => None,
                        };
                        let _ = what;
                        match name {
                            Some(name) => {
                                if self.held.len() > mark {
                                    if let Some(h) = self.held.get_mut(mark) {
                                        h.name = Some(name);
                                    }
                                    // keep only the outermost guard bound; the rest were temporaries of the initializer
                                    self.held.truncate(mark + 1);
                                }
                            }
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
            Stmt::Item(item) => self.visit_item(item),
            Stmt::Macro(_) => {}
        }
    }

    fn visit_expr_async(&mut self, a: &'ast ExprAsync) {
        let saved = (
            std::mem::take(&mut self.held),
            std::mem::take(&mut self.dropped),
            self.in_async,
        );
        self.in_async = true;
        self.visit_block(&a.block);
        self.held = saved.0;
        self.dropped = saved.1;
        self.in_async = saved.2;
    }

    fn visit_expr_closure(&mut self, c: &'ast ExprClosure) {
        // a closure runs at another time, under other guards; it is async code only when it is an `async` closure
        let saved = (
            std::mem::take(&mut self.held),
            std::mem::take(&mut self.dropped),
            self.in_async,
        );
        self.in_async = c.asyncness.is_some();
        self.visit_expr(&c.body);
        self.held = saved.0;
        self.dropped = saved.1;
        self.in_async = saved.2;
    }

    fn visit_expr_await(&mut self, a: &'ast ExprAwait) {
        // the awaited expression runs first (an async lock acquisition is `m.lock().await`, not a guard of this kind)
        if let Expr::MethodCall(m) = &*a.base {
            if m.args.is_empty()
                && matches!(m.method.to_string().as_str(), "lock" | "read" | "write")
            {
                // visit the receiver only: the acquisition suspends by design and yields an async guard
                self.visit_expr(&m.receiver);
                return;
            }
        }
        syn::visit::visit_expr_await(self, a);
        let line = line_of(a.await_token.span());
        let function = self.function.clone();
        for h in &mut self.held {
            if !h.reported {
                h.reported = true;
                self.out.push(Site {
                    kind: Kind::GuardAcrossAwait,
                    line: h.line,
                    what: h.what.clone(),
                    function: function.clone(),
                    await_line: line,
                });
            }
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        syn::visit::visit_expr_method_call(self, call);
        let name = call.method.to_string();
        if call.args.is_empty()
            && matches!(
                name.as_str(),
                "lock" | "read" | "write" | "borrow" | "borrow_mut"
            )
        {
            self.push_guard(format!("`.{name}()` guard"), line_of(call.method.span()));
        }
        if name == "block_on" {
            self.blocking(".block_on(..)".into(), line_of(call.method.span()));
        }
        if matches!(name.as_str(), "output" | "status") && call.args.is_empty() {
            if let Some(path) = command_root(&call.receiver) {
                if self.resolve(&path).starts_with(&[
                    "std".to_string(),
                    "process".to_string(),
                    "Command".to_string(),
                ]) {
                    self.blocking(
                        format!("std::process::Command::{name}"),
                        line_of(call.method.span()),
                    );
                }
            }
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        syn::visit::visit_expr_call(self, call);
        let Expr::Path(p) = &*call.func else { return };
        let path = self.resolve(&p.path);
        let line = line_of(call.span());
        let refs: Vec<&str> = path.iter().map(String::as_str).collect();
        if refs.first() == Some(&"drop") || refs.ends_with(&["mem", "drop"]) {
            if let Some(Expr::Path(arg)) = call.args.first() {
                if let Some(id) = arg.path.get_ident() {
                    self.dropped.push(id.to_string());
                }
            }
        }
        match refs.as_slice() {
            ["std", "thread", "sleep"] => self.blocking("std::thread::sleep".into(), line),
            ["std", "fs", f] => self.blocking(format!("std::fs::{f}"), line),
            ["std", "net", "TcpStream", "connect"] => {
                self.blocking("std::net::TcpStream::connect".into(), line);
            }
            ["reqwest", "blocking", rest @ ..] => {
                self.blocking(format!("reqwest::blocking::{}", rest.join("::")), line);
            }
            _ => {}
        }
        let unbounded = match refs.as_slice() {
            ["std", "sync", "mpsc", "channel"] => Some("std::sync::mpsc::channel"),
            [.., "mpsc", "unbounded_channel"] | ["tokio", .., "unbounded_channel"] => {
                Some("unbounded_channel")
            }
            ["futures", "channel", "mpsc", "unbounded"] => {
                Some("futures::channel::mpsc::unbounded")
            }
            ["crossbeam_channel", "unbounded"] | ["crossbeam", "channel", "unbounded"] => {
                Some("crossbeam_channel::unbounded")
            }
            ["flume", "unbounded"] => Some("flume::unbounded"),
            ["async_channel", "unbounded"] => Some("async_channel::unbounded"),
            _ => None,
        };
        if let Some(what) = unbounded {
            self.out.push(Site {
                kind: Kind::Unbounded,
                line,
                what: what.to_string(),
                function: self.function.clone(),
                await_line: 0,
            });
        }
    }

    // temporaries of a plain `if` / `while` condition end before the body; the scrutinee of `match`, `if let`, `while let`
    // and `for` lives through it (the default walk does that)
    fn visit_expr_if(&mut self, e: &'ast ExprIf) {
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
        syn::visit::visit_expr_match(self, e);
    }

    fn visit_expr_for_loop(&mut self, e: &'ast ExprForLoop) {
        syn::visit::visit_expr_for_loop(self, e);
    }
}

/// The path of `Command::new` at the root of a method chain (`Command::new("x").arg("y")`), when the chain starts with it.
fn command_root(expr: &Expr) -> Option<syn::Path> {
    match expr {
        Expr::MethodCall(m) => command_root(&m.receiver),
        Expr::Call(c) => match &*c.func {
            Expr::Path(p)
                if p.path.segments.last().is_some_and(|s| s.ident == "new")
                    && p.path.segments.len() >= 2 =>
            {
                let mut path = p.path.clone();
                // drop the trailing `::new`, leaving the type path
                let last = path.segments.pop()?;
                drop(last);
                path.segments.pop_punct();
                Some(path)
            }
            _ => None,
        },
        Expr::Paren(p) => command_root(&p.expr),
        _ => None,
    }
}

/// The sites of one file, or `None` when `syn` cannot parse it.
pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan::default();
    // the `use` items first, wherever they are: a path is resolved against the whole file
    for item in &parsed.items {
        collect_uses(item, &mut scan.uses);
    }
    scan.visit_file(&parsed);
    let mut out = scan.out;
    out.sort_by_key(|s| (s.line, s.await_line));
    out.dedup_by_key(|s| (s.kind as u8, s.line, s.what.clone()));
    Some(out)
}

fn collect_uses(item: &Item, uses: &mut BTreeMap<String, Vec<String>>) {
    match item {
        Item::Use(u) if !u.attrs.iter().any(is_test_attr) => {
            flatten(&u.tree, &mut Vec::new(), uses)
        }
        Item::Mod(ItemMod {
            attrs,
            content: Some((_, inner)),
            ..
        }) if !attrs.iter().any(is_test_attr) => {
            for i in inner {
                collect_uses(i, uses);
            }
        }
        _ => {}
    }
}

fn vendored(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

/// Reads every first-party Rust file once per rule asked for, and turns the sites of `kind` into findings of `id`.
fn judge_kind(
    view: &RepoView,
    id: &'static str,
    kind: Kind,
    severity: Severity,
    confidence: Confidence,
    describe: fn(&Site) -> (String, String),
) -> Verdict {
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
        for site in found.into_iter().filter(|s| s.kind == kind) {
            let (title, why) = describe(&site);
            findings.push(
                Finding::new(id, severity, confidence, view.name(), title, why)
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
        "{id}: {} Rust file(s) read with syn",
        files.len() - unparsed.len()
    )];
    if !unparsed.is_empty() {
        notes.push(format!(
            "{id}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        ));
    }
    Verdict::Noted(findings, notes)
}

fn guard_text(s: &Site) -> (String, String) {
    (
        format!("a {} is held across the `.await` at line {}", s.what, s.await_line),
        format!(
            "In `{}` a guard taken here is still held when the task suspends at line {}: every other task that wants the lock waits for this one to be woken, and a std mutex guard makes the future `!Send`. Shrink the critical section so the guard is dropped before the await (a block, or `drop`), copy out what you need first, or use an async lock if it must be held across suspension. This is read from the syntax only; a guard the compiler sees dropped earlier can still be reported.",
            s.function, s.await_line
        ),
    )
}

fn blocking_text(s: &Site) -> (String, String) {
    (
        format!("`{}` blocks the thread in async code", s.what),
        format!(
            "`{}` runs on an executor thread and does not yield, so every other task scheduled on that thread stalls until it returns. Use the async equivalent (`tokio::time::sleep`, `tokio::fs`, an async client), or hand the call to `spawn_blocking`.",
            s.function
        ),
    )
}

fn unbounded_text(s: &Site) -> (String, String) {
    (
        format!("`{}` makes a channel with no capacity", s.what),
        "A slow consumer turns an unbounded channel into memory growth with no limit. Use a bounded channel (`sync_channel(n)`, `mpsc::channel(n)`, `bounded(n)`) and decide what a full queue does: make the producer wait, shed load or return an error, and write that down.".to_string(),
    )
}

pub(super) fn judge_con002(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::CON_002,
        Kind::GuardAcrossAwait,
        Severity::Medium,
        Confidence::Medium,
        guard_text,
    )
}

pub(super) fn judge_con003(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::CON_003,
        Kind::Blocking,
        Severity::Medium,
        Confidence::Medium,
        blocking_text,
    )
}

pub(super) fn judge_con004(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::CON_004,
        Kind::Unbounded,
        Severity::Medium,
        Confidence::Medium,
        unbounded_text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, kind: Kind) -> Vec<Site> {
        sites(src)
            .expect("parses")
            .into_iter()
            .filter(|s| s.kind == kind)
            .collect()
    }

    #[test]
    fn a_std_guard_bound_by_let_and_held_across_an_await_is_reported_at_the_acquisition() {
        let src = "use std::sync::Mutex;
struct S { m: Mutex<i32> }
impl S {
    async fn f(&self) {
        let g = self.m.lock().unwrap();
        other().await;
        let _ = *g;
    }
}
async fn other() {}
";
        let s = run(src, Kind::GuardAcrossAwait);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!((s[0].line, s[0].await_line), (5, 6));
        assert_eq!(s[0].function, "f");
    }

    #[test]
    fn a_guard_dropped_or_scoped_before_the_await_or_an_async_lock_is_clean() {
        let src = "use std::sync::Mutex;
struct S { m: Mutex<i32>, t: tokio::sync::Mutex<i32> }
impl S {
    async fn dropped(&self) {
        let g = self.m.lock().unwrap();
        let n = *g;
        drop(g);
        other().await;
        let _ = n;
    }
    async fn scoped(&self) {
        { let g = self.m.lock().unwrap(); let _ = *g; }
        other().await;
    }
    async fn copied(&self) {
        let n = *self.m.lock().unwrap();
        other().await;
        let _ = n;
    }
    async fn tokio_lock(&self) {
        let g = self.t.lock().await;
        other().await;
        let _ = *g;
    }
    async fn condition(&self) {
        if *self.m.lock().unwrap() > 0 { other().await; }
    }
}
async fn other() {}
";
        assert!(run(src, Kind::GuardAcrossAwait).is_empty());
    }

    #[test]
    fn a_temporary_held_through_a_match_or_a_call_argument_is_across_the_await() {
        let src = "use std::sync::Mutex;
struct S { m: Mutex<Vec<i32>> }
impl S {
    async fn matched(&self) {
        match self.m.lock().unwrap().len() {
            0 => other().await,
            _ => {}
        }
    }
    async fn argument(&self) {
        self.m.lock().unwrap().push(value().await);
    }
}
async fn other() {}
async fn value() -> i32 { 0 }
";
        let s = run(src, Kind::GuardAcrossAwait);
        assert_eq!(s.len(), 2, "{s:?}");
    }

    #[test]
    fn a_guard_in_an_async_block_is_read_and_a_sync_function_is_not() {
        let src = "use std::sync::Mutex;
static M: Mutex<i32> = Mutex::new(0);
fn spawn_it() {
    tokio::spawn(async move {
        let g = M.lock().unwrap();
        other().await;
        let _ = *g;
    });
}
fn sync_only() {
    let g = M.lock().unwrap();
    let _ = *g;
}
async fn other() {}
";
        let s = run(src, Kind::GuardAcrossAwait);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(s[0].line, 5);
    }

    #[test]
    fn blocking_calls_are_read_through_the_use_items_and_only_in_async_code() {
        let src = "use std::thread::sleep;
use std::time::Duration;
use std::fs;
use std::process::Command;
async fn a() {
    sleep(Duration::from_secs(1));
    let _ = fs::read_to_string(\"x\");
    let _ = std::thread::sleep(Duration::from_secs(1));
    let _ = Command::new(\"ls\").arg(\"-l\").output();
}
fn sync_fn() {
    sleep(Duration::from_secs(1));
    let _ = fs::read_to_string(\"x\");
}
";
        let s = run(src, Kind::Blocking);
        let lines: Vec<u32> = s.iter().map(|x| x.line).collect();
        assert_eq!(lines, vec![6, 7, 8, 9], "{s:?}");
    }

    #[test]
    fn the_async_equivalents_and_blocking_pool_closures_are_clean() {
        let src = "use std::time::Duration;
async fn a() {
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _ = tokio::fs::read_to_string(\"x\").await;
    let _ = tokio::task::spawn_blocking(|| std::fs::read_to_string(\"x\")).await;
    let _ = tokio::process::Command::new(\"ls\").output().await;
}
";
        assert!(run(src, Kind::Blocking).is_empty());
    }

    #[test]
    fn unbounded_channels_are_found_and_bounded_ones_are_not() {
        let src = "use std::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;
fn f() {
    let (_a, _b) = mpsc::channel::<i32>();
    let (_c, _d) = unbounded_channel::<i32>();
    let (_e, _f) = crossbeam_channel::unbounded::<i32>();
    let (_g, _h) = mpsc::sync_channel::<i32>(8);
    let (_i, _j) = tokio::sync::mpsc::channel::<i32>(8);
    let (_k, _l) = crossbeam_channel::bounded::<i32>(8);
}
";
        let s = run(src, Kind::Unbounded);
        let lines: Vec<u32> = s.iter().map(|x| x.line).collect();
        assert_eq!(lines, vec![4, 5, 6], "{s:?}");
    }

    #[test]
    fn tests_are_not_read_and_an_unparsable_file_is_counted() {
        let src = "use std::sync::Mutex;
static M: Mutex<i32> = Mutex::new(0);
#[cfg(test)]
mod tests {
    use super::*;
    async fn t() { let g = M.lock().unwrap(); other().await; let _ = *g; }
    async fn other() {}
}
#[tokio::test]
async fn u() { let g = M.lock().unwrap(); tokio::task::yield_now().await; let _ = *g; }
";
        assert!(run(src, Kind::GuardAcrossAwait).is_empty());
        match judge_con002(&RepoView::from_files(&[("src/lib.rs", "fn (")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(notes.iter().any(|n| n.contains("could not be parsed")));
            }
            _ => panic!("expected a Noted verdict"),
        }
    }

    #[test]
    fn findings_carry_the_rule_id_and_the_location() {
        let src = "use std::sync::mpsc;\nfn f() { let (_a, _b) = mpsc::channel::<i32>(); }\n";
        let files = [("src/lib.rs", src)];
        let get = |v: Verdict| match v {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(w) => panic!("{w}"),
        };
        let a = get(judge_con004(&RepoView::from_files(&files)));
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].check_id.as_str(), "CON-004");
        assert_eq!(a[0].location.as_ref().unwrap().line, Some(2));
    }
}
