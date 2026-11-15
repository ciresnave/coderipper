// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-002: strings standing in for enums (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories is parsed with `syn`; items under
//! `#[cfg(test)]` and functions marked `#[test]` are skipped. A site is one of:
//!
//! * a `match` whose arms carry three or more distinct string-literal patterns (`"low" => ..`, `"a" | "b" => ..`);
//! * an `if` / `else if` chain whose conditions compare something to three or more string literals (`x == "a"`, also through `||`).
//!
//! One finding per site, reported at the `match` or the first `if`.
//!
//! **Not reported:** the body of a function named `from_str`, `try_from` or `from` (turning outside text into a typed value is exactly
//! where a string match belongs), a `match` or chain with fewer than three literals, `matches!` and macro bodies (`syn` does not look
//! inside macro tokens), and other languages. A file `syn` cannot parse is counted in a note, never read as clean.
//!
//! **What it cannot see (and the record says so):** whether the strings really are a closed set (a lookup in a data table is
//! indistinguishable from a command dispatcher at this level), strings compared through a helper, and a `match` on `&str` whose arms are
//! constants.

use super::efx003::is_dev_path;
use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeSet, HashSet};
use syn::visit::Visit;
use syn::{BinOp, Expr, ExprIf, ExprMatch, ImplItemFn, ItemFn, ItemMod, Lit, Pat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub line: u32,
    /// `match` or `if`.
    pub shape: &'static str,
    /// How many string literals were compared.
    pub literals: usize,
}

#[derive(Default)]
struct Scan {
    out: Vec<Site>,
    /// Lines of `else if` links already counted as part of a chain headed earlier.
    chain_links: HashSet<u32>,
}

/// Function names whose job is to turn outside text into a typed value.
fn is_boundary_fn(name: &str) -> bool {
    matches!(name, "from_str" | "try_from" | "from")
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => strip(&p.expr),
        Expr::Group(g) => strip(&g.expr),
        _ => e,
    }
}

fn is_str_lit(e: &Expr) -> Option<String> {
    match strip(e) {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

/// The string literals a pattern matches against.
fn pat_literals(p: &Pat, out: &mut BTreeSet<String>) {
    match p {
        Pat::Lit(l) => {
            if let Lit::Str(s) = &l.lit {
                out.insert(s.value());
            }
        }
        Pat::Or(o) => {
            for c in &o.cases {
                pat_literals(c, out);
            }
        }
        Pat::Paren(p) => pat_literals(&p.pat, out),
        _ => {}
    }
}

/// The string literals a condition compares something to (`x == "a"`, joined by `||`).
fn cond_literals(e: &Expr, out: &mut BTreeSet<String>) {
    match strip(e) {
        Expr::Binary(b) if matches!(b.op, BinOp::Or(_)) => {
            cond_literals(&b.left, out);
            cond_literals(&b.right, out);
        }
        Expr::Binary(b) if matches!(b.op, BinOp::Eq(_)) => {
            if let Some(s) = is_str_lit(&b.left).or_else(|| is_str_lit(&b.right)) {
                out.insert(s);
            }
        }
        _ => {}
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if !f.attrs.iter().any(is_test_attr) && !is_boundary_fn(&f.sig.ident.to_string()) {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        if !f.attrs.iter().any(is_test_attr) && !is_boundary_fn(&f.sig.ident.to_string()) {
            syn::visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_expr_match(&mut self, m: &'ast ExprMatch) {
        let mut lits = BTreeSet::new();
        for arm in &m.arms {
            pat_literals(&arm.pat, &mut lits);
        }
        if lits.len() >= 3 {
            self.out.push(Site {
                line: line_of(m.match_token.span),
                shape: "match",
                literals: lits.len(),
            });
        }
        syn::visit::visit_expr_match(self, m);
    }

    fn visit_expr_if(&mut self, i: &'ast ExprIf) {
        let line = line_of(i.if_token.span);
        if !self.chain_links.contains(&line) {
            let mut lits = BTreeSet::new();
            let mut cur = i;
            loop {
                cond_literals(&cur.cond, &mut lits);
                match &cur.else_branch {
                    Some((_, e)) => match &**e {
                        Expr::If(next) => {
                            self.chain_links.insert(line_of(next.if_token.span));
                            cur = next;
                        }
                        _ => break,
                    },
                    None => break,
                }
            }
            if lits.len() >= 3 {
                self.out.push(Site {
                    line,
                    shape: "if",
                    literals: lits.len(),
                });
            }
        }
        syn::visit::visit_expr_if(self, i);
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
    let id = crate::rule_id::TYP_002;
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
                        "a `{}` on {} string literals stands in for an enum",
                        site.shape, site.literals
                    ),
                    format!(
                        "this `{}` in `{path}` chooses between {} fixed strings. A misspelt string is a silent wrong branch and nothing lists the \
                         cases. Parse the text once at the boundary into an enum (`FromStr`) and match on that, so the compiler checks every case. \
                         Read from the syntax only: whether the strings are a closed set, or a lookup of data, is not judged.",
                        site.shape, site.literals
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

    fn got(src: &str) -> Vec<(u32, &'static str, usize)> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| (s.line, s.shape, s.literals))
            .collect()
    }

    #[test]
    fn a_match_on_three_string_literals_is_reported() {
        let src = "fn f(k: &str) -> u8 {\n    match k {\n        \"a\" => 1,\n        \"b\" | \"c\" => 2,\n        _ => 0,\n    }\n}";
        assert_eq!(got(src), vec![(2, "match", 3)]);
    }

    #[test]
    fn two_literals_or_non_string_patterns_are_not_reported() {
        let src = "fn f(k: &str, n: u8) {\n    match k { \"a\" => 1, \"b\" => 2, _ => 0 };\n    match n { 1 => 1, 2 => 2, 3 => 3, _ => 0 };\n}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn an_if_chain_on_three_literals_is_reported_once_at_the_head() {
        let src = "fn f(k: &str) {\n    if k == \"a\" {\n    } else if k == \"b\" {\n    } else if \"c\" == k {\n    } else {\n    }\n}";
        assert_eq!(got(src), vec![(2, "if", 3)]);
    }

    #[test]
    fn an_or_condition_counts_each_literal() {
        let src = "fn f(k: &str) {\n    if k == \"a\" || k == \"b\" || k == \"c\" {}\n}";
        assert_eq!(got(src), vec![(2, "if", 3)]);
    }

    #[test]
    fn a_short_chain_and_a_non_equality_are_not_reported() {
        let src = "fn f(k: &str) {\n    if k == \"a\" {} else if k == \"b\" {}\n    if k != \"a\" {} else if k != \"b\" {} else if k != \"c\" {}\n}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn a_boundary_function_and_test_code_are_skipped() {
        let src = "impl FromStr for K { fn from_str(s: &str) -> Result<K, ()> { match s { \"a\" => Ok(K::A), \"b\" => Ok(K::B), \"c\" => Ok(K::C), _ => Err(()) } } }\n#[cfg(test)]\nmod tests { fn t(k: &str) { match k { \"a\" => 1, \"b\" => 2, \"c\" => 3, _ => 0 }; } }";
        assert!(got(src).is_empty());
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("fn (((").is_none());
    }
}
