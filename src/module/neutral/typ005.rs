// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-005: an amount of money is not held in a binary float.
//!
//! **What this judges (Rust, from the source alone, by name):** every tracked `.rs` file of a package (outside vendored directories,
//! tests, examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. A name is *money-like* when one of
//! its snake_case words is `price`, `cost`, `balance`, `fee`, `salary`, `wage`, `payment`, `amount`, `money`, `revenue` or `charge`
//! (a plural counts), and it is not a *ratio*: a name with a word such as `rate`, `ratio`, `percent`, `factor` or `scale` is left out.
//! The rule reports, with the type read as written (`f32` or `f64`, also inside `Option`, `Vec`, a reference or a tuple):
//!
//! 1. a named struct field (and an enum variant field),
//! 2. a function or method parameter,
//! 3. a `let` binding with a type annotation, and
//! 4. a function whose own name is money-like and whose return type is a float.
//!
//! One **Medium**-severity, Low-confidence finding per site: the name is a guess about meaning, so the rule is opt-in.
//!
//! **What it cannot see:** a float held under a name that says nothing about money, a type alias for a float, a binding whose type is
//! inferred, a generic parameter later instantiated with a float, and whether the value is *meant* to be approximate (a price
//! elasticity, a score). A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::TYP_005;

const MONEY: &[&str] = &[
    "price", "cost", "balance", "fee", "salary", "wage", "payment", "amount", "money", "revenue",
    "charge",
];

/// A word that makes the quantity a proportion, a measure or a setting, not a sum of money.
const NOT_MONEY: &[&str] = &[
    "rate",
    "ratio",
    "percent",
    "percentage",
    "pct",
    "factor",
    "scale",
    "weight",
    "threshold",
    "elasticity",
    "multiplier",
    "fraction",
    "probability",
    "score",
    "delta",
    "epsilon",
    "tolerance",
    "bps",
];

fn words(name: &str) -> Vec<String> {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn singular(w: &str) -> &str {
    w.strip_suffix('s').unwrap_or(w)
}

fn money_like(name: &str) -> bool {
    let ws = words(name);
    ws.iter()
        .any(|w| MONEY.contains(&singular(w)) || MONEY.contains(&w.as_str()))
        && !ws
            .iter()
            .any(|w| NOT_MONEY.contains(&w.as_str()) || NOT_MONEY.contains(&singular(w)))
}

/// The float type named in a type as written, if any.
fn float_in(ty: &syn::Type) -> Option<&'static str> {
    match ty {
        syn::Type::Path(p) => {
            let last = p.path.segments.last()?;
            let name = last.ident.to_string();
            if p.path.segments.len() == 1 {
                match name.as_str() {
                    "f32" => return Some("f32"),
                    "f64" => return Some("f64"),
                    _ => {}
                }
            }
            if let syn::PathArguments::AngleBracketed(a) = &last.arguments {
                for arg in &a.args {
                    if let syn::GenericArgument::Type(t) = arg {
                        if let Some(f) = float_in(t) {
                            return Some(f);
                        }
                    }
                }
            }
            None
        }
        syn::Type::Reference(r) => float_in(&r.elem),
        syn::Type::Paren(p) => float_in(&p.elem),
        syn::Type::Group(g) => float_in(&g.elem),
        syn::Type::Slice(s) => float_in(&s.elem),
        syn::Type::Array(a) => float_in(&a.elem),
        syn::Type::Tuple(t) => t.elems.iter().find_map(float_in),
        _ => None,
    }
}

struct Site {
    line: u32,
    subject: String,
    what: String,
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
    owner: Vec<String>,
}

impl Walk {
    fn check(&mut self, name: &syn::Ident, ty: &syn::Type, kind: &str) {
        if !money_like(&name.to_string()) {
            return;
        }
        if let Some(f) = float_in(ty) {
            let owner = self.owner.last().cloned().unwrap_or_default();
            self.sites.push(Site {
                line: u32::try_from(name.span().start().line).unwrap_or(0),
                subject: if owner.is_empty() {
                    name.to_string()
                } else {
                    format!("{owner}::{name}")
                },
                what: format!("{kind} `{name}` is a `{f}`"),
            });
        }
    }

    fn pat_name(pat: &syn::Pat) -> Option<&syn::Ident> {
        match pat {
            syn::Pat::Ident(i) => Some(&i.ident),
            syn::Pat::Type(t) => Self::pat_name(&t.pat),
            syn::Pat::Reference(r) => Self::pat_name(&r.pat),
            _ => None,
        }
    }

    fn sig(&mut self, sig: &syn::Signature) {
        for input in &sig.inputs {
            if let syn::FnArg::Typed(t) = input {
                if let Some(n) = Self::pat_name(&t.pat) {
                    self.check(n, &t.ty, "the parameter");
                }
            }
        }
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            self.check(&sig.ident, ty, "the return value of");
        }
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

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.owner.push(i.ident.to_string());
        if let syn::Fields::Named(f) = &i.fields {
            for field in &f.named {
                if let Some(n) = &field.ident {
                    self.check(n, &field.ty, "the field");
                }
            }
        }
        self.owner.pop();
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        for v in &i.variants {
            if let syn::Fields::Named(f) = &v.fields {
                self.owner.push(format!("{}::{}", i.ident, v.ident));
                for field in &f.named {
                    if let Some(n) = &field.ident {
                        self.check(n, &field.ty, "the field");
                    }
                }
                self.owner.pop();
            }
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.sig(&i.sig);
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.sig(&i.sig);
        syn::visit::visit_impl_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.sig(&i.sig);
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let syn::Pat::Type(t) = &l.pat {
            if let Some(n) = Self::pat_name(&t.pat) {
                self.check(n, &t.ty, "the binding");
            }
        }
        syn::visit::visit_local(self, l);
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
                        Confidence::Low,
                        view.name(),
                        format!("{} (a binary float held under a money-like name)", site.what),
                        "A binary float cannot hold many decimal fractions exactly, so sums drift and comparisons surprise. Hold the amount as a whole number of the smallest unit (cents) or in a decimal type, and wrap it in a money type.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}", s.path, site.subject))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads fields, parameters and annotated bindings"
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
    fn a_money_named_float_field_parameter_binding_and_return_is_reported() {
        let src = "pub struct Order { pub total_price: f64, pub id: u64 }
pub fn pay(amount: f32) { let fee: f64 = 1.0; let _ = (amount, fee); }
pub fn net_balance() -> f64 { 0.0 }
";
        let f = found(src);
        assert_eq!(f.len(), 4, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(1));
    }

    #[test]
    fn a_float_inside_option_or_a_reference_is_seen() {
        let f = found(
            "pub struct A { pub prices: Vec<f64>, pub cost: Option<f32> }\npub fn f(salary: &f64) {}\n",
        );
        assert_eq!(f.len(), 3, "{f:?}");
    }

    #[test]
    fn ratios_integers_and_decimals_are_not_reported() {
        let src = "pub struct A { pub fee_rate: f64, pub tax_percent: f64, pub price_cents: u64, pub amount: Decimal, pub cost_factor: f32 }
pub fn f(discount_ratio: f64, score: f64) {}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn f(price: f64) {} }\n#[test]\nfn g() { let amount: f64 = 1.0; }\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(price: f64) {}\n") {
            f.validate().unwrap();
        }
    }
}
