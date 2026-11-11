// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-008 and REL-001: an outbound call or an await on something outside the process carries a deadline.
//!
//! One scan of each package's Rust source (the same walk for both rules, so they cannot disagree about a site) finds three kinds of site.
//! Every tracked `.rs` file of a package (outside vendored directories, tests, examples, tools and `build.rs`, and outside `#[cfg(test)]`
//! and test functions) is parsed with `syn`:
//!
//! 1. **An HTTP client built without a timeout:** `reqwest::Client::new()`, `reqwest::get(..)` (the `blocking` forms too), and a
//!    `Client::builder()` / `ClientBuilder::new()` chain ending in `.build()` that has no `.timeout(..)`, `.connect_timeout(..)` or
//!    `.read_timeout(..)` in it. A bare `Client` counts when the file imports it from `reqwest`.
//! 2. **An awaited connect without a deadline:** `TcpStream::connect(..).await` (tokio, async-std, smol) that is not inside a call of
//!    `timeout(..)` or `timeout_at(..)`.
//! 3. **A blocking connect without a deadline:** `std::net::TcpStream::connect(..)` (`connect_timeout` is the call that has one).
//!
//! `REL-001` (outbound calls have deadlines) reports all three; `CON-008` (an await on something outside the process has a timeout)
//! reports kind 2 only, the sites that are awaits. One **Medium**-severity, Medium-confidence finding per site, at the call.
//!
//! **What it cannot see:** a request sent with `.send().await` on a client built elsewhere, a deadline applied through a wrapper or a
//! middleware layer, a `.timeout(..)` adapter on the future (`FutureExt::timeout`, treated as a deadline for kind 2 and so not reported), the
//! database and RPC clients, and a timeout set by an environment variable. A client that waits on purpose is a waiver with a reason (the
//! rule says exceptions are declared). A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::spanned::Spanned;
use syn::visit::Visit;

const TIMEOUT_METHODS: &[&str] = &["timeout", "connect_timeout", "read_timeout"];
const DEADLINE_CALLS: &[&str] = &["timeout", "timeout_at"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    ClientWithoutTimeout,
    AwaitedConnect,
    BlockingConnect,
}

struct Site {
    line: u32,
    function: String,
    kind: Kind,
    what: String,
}

/// The full path each name of a `use` item stands for: `use reqwest::Client;` makes `Client` mean `reqwest::Client`.
#[derive(Default)]
struct Imports {
    names: BTreeMap<String, Vec<String>>,
}

fn collect_use(tree: &syn::UseTree, path: &mut Vec<String>, out: &mut Imports) {
    match tree {
        syn::UseTree::Path(p) => {
            path.push(p.ident.to_string());
            collect_use(&p.tree, path, out);
            path.pop();
        }
        syn::UseTree::Name(n) => {
            let mut full = path.clone();
            full.push(n.ident.to_string());
            out.names.insert(n.ident.to_string(), full);
        }
        syn::UseTree::Rename(r) => {
            let mut full = path.clone();
            full.push(r.ident.to_string());
            out.names.insert(r.rename.to_string(), full);
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                collect_use(t, path, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

impl Imports {
    /// The path with its first segment replaced by what that name was imported as.
    fn resolve(&self, segs: &[String]) -> Vec<String> {
        match segs.first().and_then(|s| self.names.get(s)) {
            Some(full) => full.iter().chain(segs.iter().skip(1)).cloned().collect(),
            None => segs.to_vec(),
        }
    }
}

fn path_segments(expr: &syn::Expr) -> Option<Vec<String>> {
    let syn::Expr::Path(p) = expr else {
        return None;
    };
    Some(
        p.path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect(),
    )
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

struct Walk {
    imports: Imports,
    functions: Vec<String>,
    deadline_depth: usize,
    sites: Vec<Site>,
}

fn in_crate(segs: &[String], name: &str) -> bool {
    segs.iter().any(|s| s == name)
}

impl Walk {
    fn push(&mut self, line: usize, kind: Kind, what: String) {
        self.sites.push(Site {
            line: u32::try_from(line).unwrap_or(0),
            function: self
                .functions
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".into()),
            kind,
            what,
        });
    }

    /// `Client::new`, `reqwest::get` and their `blocking` forms.
    fn reqwest_call(&self, segs: &[String]) -> Option<String> {
        let full = self.imports.resolve(segs);
        if !in_crate(&full, "reqwest") {
            return None;
        }
        let tail: Vec<&str> = full.iter().map(String::as_str).collect();
        match tail.as_slice() {
            [.., "Client", "new"] | [.., "get"] => Some(full.join("::")),
            _ => None,
        }
    }

    fn is_client_builder_root(&self, expr: &syn::Expr) -> bool {
        let syn::Expr::Call(c) = expr else {
            return false;
        };
        let Some(segs) = path_segments(&c.func) else {
            return false;
        };
        let full = self.imports.resolve(&segs);
        if !in_crate(&full, "reqwest") {
            return false;
        }
        let tail: Vec<&str> = full.iter().map(String::as_str).collect();
        matches!(
            tail.as_slice(),
            [.., "Client", "builder"] | [.., "ClientBuilder", "new"]
        )
    }

    /// The `connect` of a `TcpStream`, and whether the type is a standard-library (blocking) one.
    fn tcp_connect(&self, c: &syn::ExprCall) -> Option<bool> {
        let segs = path_segments(&c.func)?;
        let full = self.imports.resolve(&segs);
        let n = full.len();
        if n < 2 || full[n - 1] != "connect" || full[n - 2] != "TcpStream" {
            return None;
        }
        let known = ["std", "tokio", "async_std", "smol", "net"];
        if !full.iter().any(|s| known.contains(&s.as_str())) {
            return None;
        }
        Some(in_crate(&full, "std") || (in_crate(&full, "net") && !in_crate(&full, "tokio")))
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

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        let segs = path_segments(&c.func);
        let deadline = segs
            .as_ref()
            .and_then(|s| s.last())
            .is_some_and(|l| DEADLINE_CALLS.contains(&l.as_str()));
        if let Some(segs) = &segs {
            if let Some(what) = self.reqwest_call(segs) {
                self.push(
                    c.func.span().start().line,
                    Kind::ClientWithoutTimeout,
                    format!("`{what}(..)` makes an HTTP client with no timeout"),
                );
            }
        }
        if self.deadline_depth == 0 {
            if let Some(true) = self.tcp_connect(c) {
                self.push(
                    c.func.span().start().line,
                    Kind::BlockingConnect,
                    "`TcpStream::connect(..)` blocks with no deadline (`connect_timeout` has one)"
                        .into(),
                );
            }
        }
        if deadline {
            self.deadline_depth += 1;
        }
        syn::visit::visit_expr_call(self, c);
        if deadline {
            self.deadline_depth -= 1;
        }
    }

    fn visit_expr_await(&mut self, a: &'ast syn::ExprAwait) {
        if self.deadline_depth == 0 {
            if let syn::Expr::Call(c) = &*a.base {
                if let Some(false) = self.tcp_connect(c) {
                    self.push(
                        a.base.span().start().line,
                        Kind::AwaitedConnect,
                        "`TcpStream::connect(..).await` waits with no deadline".into(),
                    );
                }
            }
        }
        syn::visit::visit_expr_await(self, a);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        if m.method == "build" {
            let mut methods = Vec::new();
            let mut cur: &syn::Expr = &m.receiver;
            while let syn::Expr::MethodCall(inner) = cur {
                methods.push(inner.method.to_string());
                cur = &inner.receiver;
            }
            if self.is_client_builder_root(cur)
                && !methods
                    .iter()
                    .any(|n| TIMEOUT_METHODS.contains(&n.as_str()))
            {
                self.push(
                    m.method.span().start().line,
                    Kind::ClientWithoutTimeout,
                    "an HTTP client is built with no `.timeout(..)`, `.connect_timeout(..)` or `.read_timeout(..)`"
                        .into(),
                );
            }
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

fn sites_of(view: &RepoView, id: &str, wanted: &[Kind]) -> Verdict {
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
            let mut imports = Imports::default();
            for item in &s.file.items {
                if let syn::Item::Use(u) = item {
                    collect_use(&u.tree, &mut Vec::new(), &mut imports);
                }
            }
            let mut walk = Walk {
                imports,
                functions: Vec::new(),
                deadline_depth: 0,
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites.into_iter().filter(|x| wanted.contains(&x.kind)) {
                findings.push(
                    Finding::new(
                        id,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!("in `{}`: {}", site.function, site.what),
                        "A peer that stops answering holds the caller's task or thread, and its resources, indefinitely. Give the client a timeout when it is built (and pass the caller's remaining deadline down each hop), or wrap the call in `tokio::time::timeout`. A call that is meant to wait is a waiver with the reason.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.what))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads client builders, connects and the timeout calls around them"
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
    match unparsed_note(id, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

/// `REL-001`: every outbound call of the three kinds.
pub(super) fn judge_rel001(view: &RepoView) -> Verdict {
    sites_of(
        view,
        crate::rule_id::REL_001,
        &[
            Kind::ClientWithoutTimeout,
            Kind::AwaitedConnect,
            Kind::BlockingConnect,
        ],
    )
}

/// `CON-008`: the awaits, one kind of the same scan.
pub(super) fn judge_con008(view: &RepoView) -> Verdict {
    sites_of(view, crate::rule_id::CON_008, &[Kind::AwaitedConnect])
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn run(judge: fn(&RepoView) -> Verdict, src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_client_without_a_timeout_is_reported_in_each_form() {
        let src = "use reqwest::Client;
pub fn a() { let _ = reqwest::Client::new(); }
pub fn b() { let _ = Client::new(); }
pub async fn c() { let _ = reqwest::get(\"http://x\").await; }
pub fn d() { let _ = reqwest::Client::builder().user_agent(\"u\").build(); }
pub fn e() { let _ = reqwest::blocking::Client::new(); }
";
        let f = run(judge_rel001, src);
        assert_eq!(f.len(), 5, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(
            run(judge_con008, src).is_empty(),
            "no await of a connect here"
        );
    }

    #[test]
    fn a_builder_with_any_timeout_is_not_reported() {
        let src = "pub fn a() { let _ = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build(); }
pub fn b() { let _ = reqwest::Client::builder().connect_timeout(d()).user_agent(\"u\").build(); }
pub fn c() { let _ = reqwest::ClientBuilder::new().read_timeout(d()).build(); }
";
        assert!(
            run(judge_rel001, src).is_empty(),
            "{:?}",
            run(judge_rel001, src)
        );
    }

    #[test]
    fn an_awaited_connect_is_a_site_of_both_rules_unless_it_sits_in_a_timeout() {
        let src = "use tokio::net::TcpStream;
pub async fn a() { let _ = TcpStream::connect(\"h:1\").await; }
pub async fn b() { let _ = tokio::time::timeout(d(), TcpStream::connect(\"h:1\")).await; }
";
        assert_eq!(run(judge_rel001, src).len(), 1);
        let con = run(judge_con008, src);
        assert_eq!(con.len(), 1, "{con:?}");
        assert_eq!(con[0].location.as_ref().unwrap().line, Some(2));
        assert_eq!(con[0].check_id, "CON-008");
    }

    #[test]
    fn a_blocking_connect_is_reported_for_rel_001_only() {
        let src = "use std::net::TcpStream;
pub fn a() { let _ = TcpStream::connect(\"h:1\"); }
pub fn b() { let _ = std::net::TcpStream::connect_timeout(&addr(), d()); }
";
        let f = run(judge_rel001, src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(run(judge_con008, src).is_empty());
    }

    #[test]
    fn other_types_named_client_and_test_code_are_not_read() {
        let src = "pub struct Client; impl Client { pub fn new() -> Self { Client } }
pub fn a() { let _ = Client::new(); }
#[cfg(test)]
mod t { pub fn b() { let _ = reqwest::Client::new(); } }
#[tokio::test]
async fn c() { let _ = reqwest::Client::new(); }
";
        assert!(
            run(judge_rel001, src).is_empty(),
            "{:?}",
            run(judge_rel001, src)
        );
    }

    #[test]
    fn every_finding_validates() {
        for f in run(
            judge_rel001,
            "pub fn a() { let _ = reqwest::Client::new(); }\n",
        ) {
            f.validate().unwrap();
        }
    }
}
