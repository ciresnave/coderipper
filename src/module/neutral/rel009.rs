// SPDX-License-Identifier: MIT OR Apache-2.0
//! REL-009: monotonic clocks for durations (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories is parsed with `syn`; items under
//! `#[cfg(test)]` and functions marked `#[test]` are skipped. Inside each function body a local is *wall-clock* when it is bound by
//! `let [mut] name = SystemTime::now()` (also `std::time::SystemTime::now()`). A site is a method call that measures a duration on the
//! wall clock:
//!
//! * `name.elapsed()` on a wall-clock local, or `SystemTime::now().elapsed()`;
//! * `x.duration_since(y)` where `x` is a wall-clock local or a `SystemTime::now()` call, unless `y` is `UNIX_EPOCH`;
//! * `x.duration_since(y)` where `y` is a wall-clock local and `x` is not known.
//!
//! `SystemTime::now().duration_since(UNIX_EPOCH)` is a timestamp, not a measured interval, and is never a site. A start time that is
//! stored in a field or passed in is not followed: no type inference, so recall is low and the rule says so.
//!
//! **Not judged:** `chrono` or `time` crate wall clocks (`Utc::now() - start`), a `SystemTime` that reaches the call through a field,
//! a parameter or a function return, other languages. A file `syn` cannot parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashSet;
use syn::visit::Visit;
use syn::{Expr, ExprMethodCall, ImplItemFn, ItemFn, ItemMod, Local, Pat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub line: u32,
    /// `elapsed` or `duration_since`.
    pub method: String,
}

#[derive(Default)]
struct Scan {
    out: Vec<Site>,
    /// Names bound from `SystemTime::now()` in the function being read.
    wall: HashSet<String>,
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => strip(&p.expr),
        Expr::Group(g) => strip(&g.expr),
        _ => e,
    }
}

/// `SystemTime::now()`, with or without a leading path.
fn is_now_call(e: &Expr) -> bool {
    let Expr::Call(c) = strip(e) else {
        return false;
    };
    let Expr::Path(p) = strip(&c.func) else {
        return false;
    };
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    segs.len() >= 2 && segs[segs.len() - 2] == "SystemTime" && segs[segs.len() - 1] == "now"
}

fn path_ident(e: &Expr) -> Option<String> {
    match strip(e) {
        Expr::Path(p) if p.path.segments.len() == 1 => Some(p.path.segments[0].ident.to_string()),
        _ => None,
    }
}

fn is_epoch(e: &Expr) -> bool {
    matches!(strip(e), Expr::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "UNIX_EPOCH"))
}

impl Scan {
    fn is_wall(&self, e: &Expr) -> bool {
        is_now_call(e) || path_ident(e).is_some_and(|n| self.wall.contains(&n))
    }

    fn in_fn(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = std::mem::take(&mut self.wall);
        f(self);
        self.wall = saved;
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !f.attrs.iter().any(is_test_attr) {
            self.in_fn(|s| syn::visit::visit_item_fn(s, f));
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        if !f.attrs.iter().any(is_test_attr) {
            self.in_fn(|s| syn::visit::visit_impl_item_fn(s, f));
        }
    }

    fn visit_local(&mut self, l: &'ast Local) {
        // Visit the initialiser first so `let a = f(a.elapsed())` reads the old binding, then record the new one.
        syn::visit::visit_local(self, l);
        let name = match &l.pat {
            Pat::Ident(p) => Some(p.ident.to_string()),
            Pat::Type(t) => match &*t.pat {
                Pat::Ident(p) => Some(p.ident.to_string()),
                _ => None,
            },
            _ => None,
        };
        if let (Some(name), Some(init)) = (name, &l.init) {
            if is_now_call(&init.expr) {
                self.wall.insert(name);
            } else {
                self.wall.remove(&name);
            }
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast ExprMethodCall) {
        let method = m.method.to_string();
        let hit = match method.as_str() {
            "elapsed" => m.args.is_empty() && self.is_wall(&m.receiver),
            "duration_since" => m.args.first().is_some_and(|arg| {
                (self.is_wall(&m.receiver) && !is_epoch(arg))
                    || (path_ident(arg).is_some_and(|n| self.wall.contains(&n))
                        && !self.is_wall(&m.receiver))
            }),
            _ => false,
        };
        if hit {
            self.out.push(Site {
                line: line_of(m.method.span()),
                method,
            });
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan::default();
    scan.visit_file(&parsed);
    let mut out = scan.out;
    out.sort_by_key(|s| s.line);
    out.dedup();
    Some(out)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let id = crate::rule_id::REL_009;
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
                    id,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!(
                        "a duration is measured on the wall clock (`.{}` on a `SystemTime`)",
                        site.method
                    ),
                    format!(
                        "`.{}` in `{path}` measures an interval from `SystemTime::now()`. The wall clock jumps when it is adjusted, so the interval can be \
                         negative, an error, or far off. Take the start with `Instant::now()` and read `.elapsed()` on that. Keep `SystemTime` for \
                         timestamps shown to people or stored. Read from the syntax only: a start time held in a field or passed in is not followed.",
                        site.method
                    ),
                )
                .location(Location::new(path.as_str(), Some(site.line)))
                .subject(format!("{}:{}", path, site.line))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn got(src: &str) -> Vec<(u32, String)> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| (s.line, s.method))
            .collect()
    }

    #[test]
    fn elapsed_on_a_system_time_local_is_reported() {
        let src = "fn f() {\n    let start = SystemTime::now();\n    work();\n    let d = start.elapsed();\n}";
        assert_eq!(got(src), vec![(4, "elapsed".to_string())]);
    }

    #[test]
    fn a_full_path_and_a_mut_binding_are_read() {
        let src =
            "fn f() {\n    let mut t = std::time::SystemTime::now();\n    t.elapsed().ok();\n}";
        assert_eq!(got(src), vec![(3, "elapsed".to_string())]);
    }

    #[test]
    fn duration_since_between_wall_readings_is_reported_but_the_epoch_is_not() {
        let src = "fn f(stored: SystemTime) {\n    let a = SystemTime::now();\n    let b = SystemTime::now();\n    b.duration_since(a);\n    let ts = SystemTime::now().duration_since(UNIX_EPOCH);\n    stored.duration_since(a);\n}";
        assert_eq!(
            got(src),
            vec![
                (4, "duration_since".to_string()),
                (6, "duration_since".to_string())
            ]
        );
    }

    #[test]
    fn instant_and_an_unknown_start_are_not_reported() {
        let src = "fn f(start: SystemTime) {\n    let i = Instant::now();\n    i.elapsed();\n    start.elapsed();\n}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn a_rebinding_to_instant_clears_the_wall_clock_name() {
        let src = "fn f() {\n    let t = SystemTime::now();\n    let t = Instant::now();\n    t.elapsed();\n}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn test_code_is_skipped_and_a_function_does_not_leak_into_the_next() {
        let src = "fn a() { let t = SystemTime::now(); }\nfn b() { t.elapsed(); }\n#[cfg(test)]\nmod tests { fn c() { let t = SystemTime::now(); t.elapsed(); } }";
        assert!(got(src).is_empty());
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("fn (((").is_none());
    }
}
