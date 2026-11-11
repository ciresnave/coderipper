// SPDX-License-Identifier: MIT OR Apache-2.0
//! COR-007: floating-point values are not compared for exact equality.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` and `#[test]` items) is parsed with `syn`. A `==` or `!=` is reported when
//! one operand is visibly a float:
//!
//! 1. a float literal other than zero (`0.5`, `1e-9`, `2f64`, a leading `-` included),
//! 2. a cast to `f32` or `f64` (`n as f64`), or
//! 3. an associated constant of `f32`/`f64` or of a `consts` module that is not an exact sentinel (`f64::EPSILON`, `f64::NAN`,
//!    `std::f64::consts::PI`),
//!
//! and the same literals inside `assert_eq!`, `assert_ne!`, `debug_assert_eq!` and `debug_assert_ne!`. A comparison with a zero
//! literal, with `INFINITY`, `NEG_INFINITY`, `MAX`, `MIN` or `MIN_POSITIVE` is a test of an exact sentinel, which Clippy's `float_cmp`
//! also lets pass, and is not reported. One **Medium**-severity, Medium-confidence finding per comparison, at the operator.
//!
//! **What it cannot see:** a comparison of two values whose float type only a type checker knows (`a == b` with `a: f64`), a comparison
//! through `PartialEq::eq`, `total_cmp` or a helper, and whether a tolerance is declared elsewhere. The rule's second half ("the
//! tolerance used for approximate comparison is stated") is not judged. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use proc_macro2::{TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::COR_007;

/// Associated constants that are exact values: comparing with them is a test of a sentinel, not an approximate comparison.
const EXACT_CONSTS: &[&str] = &["INFINITY", "NEG_INFINITY", "MAX", "MIN", "MIN_POSITIVE"];

const ASSERTS: &[&str] = &[
    "assert_eq",
    "assert_ne",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// What an operand says about the comparison.
enum Operand {
    /// Not visibly a float.
    Other,
    /// An exact sentinel (zero, an infinity, a limit): the comparison is not reported.
    Sentinel,
    /// A float that makes the comparison approximate in practice; the text says which.
    Float(String),
}

fn is_float_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.path.is_ident("f32") || p.path.is_ident("f64"))
}

fn operand(e: &syn::Expr) -> Operand {
    match e {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Float(f) => match f.base10_parse::<f64>() {
                Ok(0.0) => Operand::Sentinel,
                _ => Operand::Float(format!("the literal `{f}`")),
            },
            syn::Lit::Int(i) if matches!(i.suffix(), "f32" | "f64") => {
                if i.base10_digits().chars().all(|c| c == '0') {
                    Operand::Sentinel
                } else {
                    Operand::Float(format!("the literal `{i}`"))
                }
            }
            _ => Operand::Other,
        },
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => operand(&u.expr),
        syn::Expr::Paren(p) => operand(&p.expr),
        syn::Expr::Group(g) => operand(&g.expr),
        syn::Expr::Cast(c) if is_float_type(&c.ty) => Operand::Float("a cast to a float".into()),
        syn::Expr::Path(p) => {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let Some(last) = segs.last() else {
                return Operand::Other;
            };
            let float_owner = segs.iter().rev().skip(1).any(|s| s == "f32" || s == "f64");
            let consts = segs.iter().any(|s| s == "consts");
            if float_owner && (consts || segs.len() >= 2) {
                if EXACT_CONSTS.contains(&last.as_str()) {
                    Operand::Sentinel
                } else {
                    Operand::Float(format!("the constant `{}`", segs.join("::")))
                }
            } else {
                Operand::Other
            }
        }
        _ => Operand::Other,
    }
}

/// A non-zero float literal inside the tokens of an `assert_eq!`-style macro.
fn float_token(ts: TokenStream) -> Option<String> {
    for t in ts {
        match t {
            TokenTree::Group(g) => {
                if let Some(f) = float_token(g.stream()) {
                    return Some(f);
                }
            }
            TokenTree::Literal(l) => {
                let text = l.to_string();
                let numeric = text.starts_with(|c: char| c.is_ascii_digit());
                let floaty = text.contains('.')
                    || text.ends_with("f32")
                    || text.ends_with("f64")
                    || (text.contains(['e', 'E']) && !text.starts_with("0x"));
                let zero = text
                    .trim_end_matches("f32")
                    .trim_end_matches("f64")
                    .chars()
                    .all(|c| matches!(c, '0' | '.' | '_'));
                if numeric && floaty && !zero {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

struct Site {
    line: u32,
    function: String,
    what: String,
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
    functions: Vec<String>,
}

impl Walk {
    fn function(&self) -> String {
        self.functions
            .last()
            .cloned()
            .unwrap_or_else(|| "<module>".into())
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
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

    fn visit_expr_binary(&mut self, e: &'ast syn::ExprBinary) {
        let op = match e.op {
            syn::BinOp::Eq(_) => Some("=="),
            syn::BinOp::Ne(_) => Some("!="),
            _ => None,
        };
        if let Some(op) = op {
            let (l, r) = (operand(&e.left), operand(&e.right));
            let what = match (l, r) {
                (Operand::Sentinel, _) | (_, Operand::Sentinel) => None,
                (Operand::Float(w), _) | (_, Operand::Float(w)) => Some(w),
                _ => None,
            };
            if let Some(w) = what {
                self.sites.push(Site {
                    line: u32::try_from(e.op.span().start().line).unwrap_or(0),
                    function: self.function(),
                    what: format!("`{op}` against {w}"),
                });
            }
        }
        syn::visit::visit_expr_binary(self, e);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if let Some(seg) = m.path.segments.last() {
            let name = seg.ident.to_string();
            if ASSERTS.contains(&name.as_str()) {
                if let Some(lit) = float_token(m.tokens.clone()) {
                    self.sites.push(Site {
                        line: u32::try_from(seg.ident.span().start().line).unwrap_or(0),
                        function: self.function(),
                        what: format!("`{name}!` against the literal `{lit}`"),
                    });
                }
            }
        }
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
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!("exact float comparison in `{}`: {}", site.function, site.what),
                        "Two floats that should be equal rarely are bit for bit. Compare with a stated tolerance (`(a - b).abs() <= EPS`, with the EPS named and its unit said), or compare integers (the smallest unit) instead.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.what))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that finds comparison operators and assert macros"
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
    fn a_nonzero_literal_a_cast_and_a_constant_are_reported() {
        let src = "pub fn f(x: f64, n: u32) -> bool {
    let a = x == 0.5;
    let b = x != -1e-9;
    let c = x == n as f64;
    let d = x == f64::EPSILON;
    let e = 2f64 != x;
    a || b || c || d || e
}
";
        let f = found(src);
        assert_eq!(f.len(), 5, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(f[0].subject.as_deref().unwrap().contains(":f:"));
    }

    #[test]
    fn sentinels_integers_and_tolerance_comparisons_are_not_reported() {
        let src = "pub fn f(x: f64, n: u32) -> bool {
    x == 0.0 || x != 0f64 || x == f64::INFINITY || x == f64::MAX || n == 3 || (x - 1.5).abs() < 1e-9 || x < 0.5
}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn assert_macros_with_a_float_literal_are_reported() {
        let f = found("pub fn f(x: f64) { assert_eq!(x, 0.1 + 0.2); debug_assert_ne!(x, 1.5f32); assert_eq!(x, 0.0); }\n");
        assert_eq!(f.len(), 2, "{f:?}");
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn f(x: f64) -> bool { x == 0.5 } }\n#[test]\nfn g() { assert_eq!(1.0f64, 1.0); let _ = 2.0 == 2.5; }\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(x: f64) -> bool { x == 0.5 }\n") {
            f.validate().unwrap();
        }
    }
}
