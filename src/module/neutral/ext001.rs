// SPDX-License-Identifier: MIT OR Apache-2.0
//! EXT-001: a `match` over a closed variant set of the package lists the variants, with no catch-all arm outside the defining module.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. The package's own enums are collected (an
//! enum whose name is held by two enums of the package is left out, as is an enum marked `#[non_exhaustive]`, which is open by its
//! author's own word). A `match` is reported when
//!
//! 1. its arms name variants of exactly one of those enums by a path with the enum's name (`Shape::Circle`, `crate::shape::Shape::Circle
//!    { .. }`, alone or in an or-pattern),
//! 2. it has a catch-all arm without a guard (`_`, or a lower-case binding such as `other`),
//! 3. that catch-all arm absorbs at least one variant that no other unguarded arm lists, and
//! 4. the `match` sits in another file than the one that defines the enum (the file stands for the module here).
//!
//! One **Medium**-severity, Medium-confidence finding per `match`, at the catch-all arm, naming the absorbed variants.
//!
//! **What it cannot see:** a variant matched by its bare name after `use Shape::*`, `Self::Variant` patterns, an enum of another crate,
//! a tuple scrutinee over two enums, `if let` and `matches!`, and whether a catch-all is a deliberate default. That last point is why
//! the rule is opt-in. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::EXT_001;

/// An enum of the package: where it is defined and what it holds.
struct Def {
    file: String,
    variants: BTreeSet<String>,
    non_exhaustive: bool,
}

#[derive(Default)]
struct Enums {
    defs: BTreeMap<String, Vec<Def>>,
}

struct Collect<'a> {
    file: &'a str,
    enums: &'a mut Enums,
}

fn has_non_exhaustive(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("non_exhaustive"))
}

impl<'ast> Visit<'ast> for Collect<'_> {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.enums
            .defs
            .entry(i.ident.to_string())
            .or_default()
            .push(Def {
                file: self.file.to_string(),
                variants: i.variants.iter().map(|v| v.ident.to_string()).collect(),
                non_exhaustive: has_non_exhaustive(&i.attrs),
            });
    }
}

/// The `(enum, variant)` pairs a pattern names by a path with the enum's name in it.
fn refs(pat: &syn::Pat, out: &mut Vec<(String, String)>) {
    let path = match pat {
        syn::Pat::Path(p) => Some(&p.path),
        syn::Pat::TupleStruct(p) => Some(&p.path),
        syn::Pat::Struct(p) => Some(&p.path),
        syn::Pat::Or(o) => {
            for c in &o.cases {
                refs(c, out);
            }
            None
        }
        syn::Pat::Reference(r) => {
            refs(&r.pat, out);
            None
        }
        syn::Pat::Paren(p) => {
            refs(&p.pat, out);
            None
        }
        syn::Pat::Guard(g) => {
            refs(&g.pat, out);
            None
        }
        _ => None,
    };
    if let Some(path) = path {
        let n = path.segments.len();
        if n >= 2 {
            let owner = path.segments[n - 2].ident.to_string();
            if owner != "Self" {
                out.push((owner, path.segments[n - 1].ident.to_string()));
            }
        }
    }
}

fn is_catch_all(pat: &syn::Pat) -> bool {
    match pat {
        syn::Pat::Wild(_) => true,
        syn::Pat::Ident(i) => {
            i.subpat.is_none()
                && i.ident
                    .to_string()
                    .starts_with(|c: char| c.is_lowercase() || c == '_')
        }
        syn::Pat::Or(o) => o.cases.iter().any(is_catch_all),
        syn::Pat::Paren(p) => is_catch_all(&p.pat),
        _ => false,
    }
}

struct Site {
    line: u32,
    function: String,
    enum_name: String,
    absorbed: Vec<String>,
}

struct Walk<'a> {
    file: &'a str,
    enums: &'a Enums,
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
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

    fn visit_expr_match(&mut self, m: &'ast syn::ExprMatch) {
        self.judge(m);
        syn::visit::visit_expr_match(self, m);
    }
}

impl Walk<'_> {
    fn judge(&mut self, m: &syn::ExprMatch) {
        let mut named: Vec<(String, String)> = Vec::new();
        for arm in &m.arms {
            refs(&arm.pat, &mut named);
        }
        let owners: BTreeSet<&String> = named.iter().map(|(e, _)| e).collect();
        let Some(owner) = (owners.len() == 1)
            .then(|| owners.into_iter().next())
            .flatten()
        else {
            return;
        };
        let Some(defs) = self.enums.defs.get(owner) else {
            return;
        };
        let [def] = defs.as_slice() else {
            return;
        };
        if def.non_exhaustive || def.file == self.file {
            return;
        }
        let Some(catch_all) = m.arms.iter().find(|a| is_catch_all(&a.pat)) else {
            return;
        };
        // a name that is not a variant of the enum is another type of the same name
        if named.iter().any(|(_, v)| !def.variants.contains(v)) {
            return;
        }
        let mut covered: BTreeSet<String> = BTreeSet::new();
        for arm in &m.arms {
            if !matches!(arm.pat, syn::Pat::Guard(_)) && !is_catch_all(&arm.pat) {
                let mut r = Vec::new();
                refs(&arm.pat, &mut r);
                covered.extend(r.into_iter().map(|(_, v)| v));
            }
        }
        let absorbed: Vec<String> = def.variants.difference(&covered).cloned().collect();
        if absorbed.is_empty() {
            return;
        }
        self.sites.push(Site {
            line: u32::try_from(catch_all.pat.span().start().line).unwrap_or(0),
            function: self
                .functions
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".into()),
            enum_name: owner.clone(),
            absorbed,
        });
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
        let mut enums = Enums::default();
        for s in &p.sources {
            Collect {
                file: &s.path,
                enums: &mut enums,
            }
            .visit_file(&s.file);
        }
        let known = enums.defs.len();
        for s in &p.sources {
            let mut walk = Walk {
                file: &s.path,
                enums: &enums,
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
                            "`match` over `{}` in `{}` ends in a catch-all arm that absorbs {}",
                            site.enum_name,
                            site.function,
                            site.absorbed
                                .iter()
                                .map(|v| format!("`{v}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        "A catch-all arm absorbs every variant added later without a compiler error. List the variants (an or-pattern keeps it short) so adding one points at this place.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.enum_name))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed, and {known} enum name(s) of the package were collected, with the same walk that reads match arms"
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
    const SHAPE: &str = "pub enum Shape { Circle, Square, Triangle }\n";

    fn found(user: &str) -> Vec<Finding> {
        found_with(&[("src/shape.rs", SHAPE), ("src/user.rs", user)])
    }

    fn found_with(files: &[(&str, &str)]) -> Vec<Finding> {
        let mut all = vec![("Cargo.toml", MANIFEST)];
        all.extend_from_slice(files);
        match judge(&RepoView::from_files(&all)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_catch_all_over_another_files_enum_is_reported_with_the_absorbed_variants() {
        let f = found(
            "use crate::shape::Shape;\npub fn sides(s: &Shape) -> u32 {\n    match s {\n        Shape::Circle => 0,\n        _ => 4,\n    }\n}\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(5));
        assert!(
            f[0].summary.contains("`Square`, `Triangle`"),
            "{}",
            f[0].summary
        );
        assert!(f[0].subject.as_deref().unwrap().ends_with(":sides:Shape"));
    }

    #[test]
    fn a_binding_catch_all_counts_and_a_full_listing_or_a_guard_does_not() {
        let src = "use crate::shape::Shape;
pub fn a(s: Shape) -> u32 { match s { Shape::Circle | Shape::Square => 1, other => drop_it(other) } }
pub fn b(s: Shape) -> u32 { match s { Shape::Circle => 0, Shape::Square => 1, Shape::Triangle => 2 } }
pub fn c(s: Shape) -> u32 { match s { Shape::Circle => 0, Shape::Square => 1, Shape::Triangle => 2, _ => 9 } }
";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        let guarded = "pub fn g(s: Shape, n: u32) -> u32 { match s { Shape::Circle => 0, _ if n > 1 => 1, Shape::Square | Shape::Triangle => 2 } }\n";
        assert!(found(guarded).is_empty());
    }

    #[test]
    fn the_defining_file_non_exhaustive_enums_and_duplicate_names_are_left_out() {
        let same = format!(
            "{SHAPE}pub fn f(s: Shape) -> u32 {{ match s {{ Shape::Circle => 0, _ => 1 }} }}\n"
        );
        assert!(found_with(&[("src/shape.rs", &same)]).is_empty());
        let open = "#[non_exhaustive]\npub enum Shape { Circle, Square }\n";
        let user = "pub fn f(s: Shape) -> u32 { match s { Shape::Circle => 0, _ => 1 } }\n";
        assert!(found_with(&[("src/shape.rs", open), ("src/user.rs", user)]).is_empty());
        let twin = "pub enum Shape { Circle, Square }\n";
        assert!(found_with(&[
            ("src/shape.rs", SHAPE),
            ("src/other.rs", twin),
            ("src/user.rs", user)
        ])
        .is_empty());
    }

    #[test]
    fn an_enum_of_another_crate_a_bare_variant_and_test_code_are_not_seen() {
        let src = "use std::cmp::Ordering;\npub fn f(o: Ordering) -> u32 { match o { Ordering::Less => 0, _ => 1 } }\n#[cfg(test)]\nmod t { use crate::shape::Shape; fn g(s: Shape) -> u32 { match s { Shape::Circle => 0, _ => 1 } } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(s: &Shape) -> u32 { match s { Shape::Circle => 0, _ => 1 } }\n") {
            f.validate().unwrap();
        }
    }
}
