// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-005: a spawned task is not left detached without a written reason.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. A call of a free spawn function
//! (`tokio::spawn`, `tokio::task::spawn`, `task::spawn_local`, `thread::spawn`, `std::thread::spawn`, `async_std::task::spawn`,
//! `smol::spawn`, `spawn_blocking` of those, or a bare `spawn(..)` that the file imports from one of them) is reported when its handle
//! is dropped on the spot: the call is a statement of its own (`spawn(..);`) or is bound to `_` (`let _ = spawn(..);`). A handle kept in a
//! variable (`let handle = ..`, `let _task = ..`), returned, pushed, awaited or joined is not reported, and neither is a spawn through a
//! method (`set.spawn(..)` of a `JoinSet`, `scope.spawn(..)`), where a set or a scope owns the task.
//!
//! A detached spawn with a justification is not reported: a `//` comment on the line or in the three lines above that says why
//! (`detach`, `fire-and-forget`, `fire and forget`, `background`, `on purpose`, `deliberate`, `intentional`, `supervis`, `owned by`).
//! One **Medium**-severity, Medium-confidence finding per detached spawn, at the call.
//!
//! **What it cannot see:** a handle that is kept and never joined (`let _h = ..` then forgotten), a spawn through a wrapper function of
//! the project, a runtime handle's method (`rt.spawn(..)` is a method and is left alone), and whether the task forwards its own errors.
//! A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeSet;
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::CON_005;

const SPAWNS: &[&str] = &["spawn", "spawn_local", "spawn_blocking"];

/// The path segments that make a `spawn` a task or thread spawn and not another function of the same name.
const RUNTIMES: &[&str] = &[
    "tokio",
    "task",
    "thread",
    "async_std",
    "smol",
    "std",
    "glommio",
    "monoio",
];

const JUSTIFICATIONS: &[&str] = &[
    "detach",
    "fire-and-forget",
    "fire and forget",
    "background",
    "on purpose",
    "deliberate",
    "intentional",
    "supervis",
    "owned by",
];

/// The names a file imports from a runtime or the thread module: `use tokio::spawn;` makes a bare `spawn(..)` one.
#[derive(Default)]
struct Imports {
    spawns: BTreeSet<String>,
}

fn collect_use(tree: &syn::UseTree, path: &mut Vec<String>, out: &mut Imports) {
    match tree {
        syn::UseTree::Path(p) => {
            path.push(p.ident.to_string());
            collect_use(&p.tree, path, out);
            path.pop();
        }
        syn::UseTree::Name(n) => {
            let name = n.ident.to_string();
            if SPAWNS.contains(&name.as_str())
                && path.iter().any(|s| RUNTIMES.contains(&s.as_str()))
            {
                out.spawns.insert(name);
            }
        }
        syn::UseTree::Rename(r) => {
            let name = r.ident.to_string();
            if SPAWNS.contains(&name.as_str())
                && path.iter().any(|s| RUNTIMES.contains(&s.as_str()))
            {
                out.spawns.insert(r.rename.to_string());
            }
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                collect_use(t, path, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

struct Site {
    line: u32,
    function: String,
    callee: String,
}

struct Walk<'a> {
    imports: Imports,
    lines: Vec<&'a str>,
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

/// The spawn this call is, as a path text, when it is a free spawn function.
fn spawn_callee(call: &syn::ExprCall, imports: &Imports) -> Option<String> {
    let syn::Expr::Path(p) = &*call.func else {
        return None;
    };
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let last = segs.last()?;
    if segs.len() == 1 {
        return imports.spawns.contains(last).then(|| last.clone());
    }
    if !SPAWNS.contains(&last.as_str()) {
        return None;
    }
    let parents = &segs[..segs.len() - 1];
    parents
        .iter()
        .any(|s| RUNTIMES.contains(&s.as_str()))
        .then(|| segs.join("::"))
}

impl Walk<'_> {
    /// A justifying comment: on the call's own line, or in the comment block directly above it (comment lines and blank lines up to
    /// the first line of code, three lines at most). A comment above an earlier statement is that statement's.
    fn justified(&self, line: u32) -> bool {
        let says = |l: &str| {
            l.find("//").is_some_and(|i| {
                let comment = l[i..].to_lowercase();
                JUSTIFICATIONS.iter().any(|j| comment.contains(j))
            })
        };
        let line = usize::try_from(line).unwrap_or(0);
        if self
            .lines
            .get(line.wrapping_sub(1))
            .is_some_and(|l| says(l))
        {
            return true;
        }
        for n in (line.saturating_sub(3).max(1)..line).rev() {
            let Some(l) = self.lines.get(n - 1) else {
                break;
            };
            let t = l.trim_start();
            if t.is_empty() {
                continue;
            }
            if !t.starts_with("//") {
                break;
            }
            if says(l) {
                return true;
            }
        }
        false
    }

    fn detached(&mut self, expr: &syn::Expr) {
        let syn::Expr::Call(call) = expr else {
            return;
        };
        let Some(callee) = spawn_callee(call, &self.imports) else {
            return;
        };
        let line = u32::try_from(call.func.span().start().line).unwrap_or(0);
        if self.justified(line) {
            return;
        }
        self.sites.push(Site {
            line,
            function: self
                .functions
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".into()),
            callee,
        });
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
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

    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        match s {
            syn::Stmt::Expr(e, Some(_)) => self.detached(e),
            syn::Stmt::Local(l) if matches!(l.pat, syn::Pat::Wild(_)) => {
                if let Some(init) = &l.init {
                    self.detached(&init.expr);
                }
            }
            _ => {}
        }
        syn::visit::visit_stmt(self, s);
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
            let mut imports = Imports::default();
            for item in &s.file.items {
                if let syn::Item::Use(u) = item {
                    collect_use(&u.tree, &mut Vec::new(), &mut imports);
                }
            }
            let mut walk = Walk {
                imports,
                lines: s.text.lines().collect(),
                functions: Vec::new(),
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!(
                            "`{}` in `{}` detaches its task: the handle is dropped on the spot",
                            site.callee, site.function
                        ),
                        "A task nobody owns can fail silently, outlive the work that started it and leak resources at shutdown. Keep the handle and await it, spawn into a JoinSet or a scope, or say in a comment on the spawn why it may run on its own.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.callee))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads spawn statements and the comments above them"
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
    fn a_dropped_spawn_statement_and_a_let_underscore_are_reported() {
        let src = "pub async fn f() {
    tokio::spawn(async { work().await });
    let _ = std::thread::spawn(|| work_sync());
    tokio::task::spawn_blocking(|| 1);
}
";
        let f = found(src);
        assert_eq!(f.len(), 3, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(f[0]
            .subject
            .as_deref()
            .unwrap()
            .ends_with(":f:tokio::spawn"));
    }

    #[test]
    fn a_kept_awaited_returned_or_set_owned_handle_is_not_reported() {
        let src = "pub async fn f(set: &mut JoinSet<()>) -> JoinHandle<()> {
    let h = tokio::spawn(async {});
    let _task = tokio::spawn(async {});
    tokio::spawn(async {}).await.unwrap();
    set.spawn(async {});
    std::thread::scope(|s| { s.spawn(|| 1); });
    handles.push(tokio::spawn(async {}));
    tokio::spawn(async {})
}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_justifying_comment_above_or_on_the_line_clears_the_spawn() {
        let src = "pub async fn f() {
    // fire-and-forget: the metrics flush owns itself
    tokio::spawn(flush());
    tokio::spawn(tick()); // detached on purpose
    tokio::spawn(oops());
}
";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(5));
    }

    #[test]
    fn a_bare_spawn_counts_only_when_the_file_imports_it_from_a_runtime() {
        let imported = "use tokio::spawn;\npub async fn f() { spawn(async {}); }\n";
        assert_eq!(found(imported).len(), 1);
        let local = "fn spawn(_: u8) {}\npub fn f() { spawn(1); }\n";
        assert!(found(local).is_empty());
        let renamed = "use std::thread::spawn as go;\npub fn f() { go(|| 1); }\n";
        assert_eq!(found(renamed).len(), 1, "a renamed import is followed");
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub async fn f() { tokio::spawn(async {}); } }\n#[tokio::test]\nasync fn g() { tokio::spawn(async {}); }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f() { std::thread::spawn(|| 1); }\n") {
            f.validate().unwrap();
        }
    }
}
