// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-001: public APIs wrap domain values in dedicated types (Rust).
//!
//! **What this judges (Rust, from the source alone):** a public function whose parameter is *named* for a domain value and *typed* as a bare
//! primitive, so the compiler cannot tell a swapped pair apart. It reads tracked `.rs` files of a library (outside vendored directories,
//! tests, examples, benches, `build.rs`, a `main.rs` and `src/bin`, and outside `#[cfg(test)]` and `#[doc(hidden)]` items), parsed with `syn`.
//! A function is read when it is `pub` (free functions, `pub` methods of an inherent `impl`, the methods of a `pub` trait); the methods of
//! `impl Trait for Type` are not (the trait dictates their signature).
//!
//! **Three classes, by name** (the name is read as `snake_case` segments):
//!
//! * **money**: a segment `price`, `amount`, `money`, `cost`, `balance`, `fee` or `salary`, typed as a float or an integer;
//! * **duration**: a segment `duration`, `timeout`, `ttl` or `delay`, typed as a float or an integer (`std::time::Duration` is the type);
//! * **identifier**: the last segment `id` (`id`, `user_id`, `order_id`), typed as an integer, `String`, `&str`, `&String` or `Cow<str>`.
//!
//! A name that already says its unit is not reported: a segment `ms`, `millis`, `secs`, `seconds`, `minutes`, `hours`, `days`, `micros`,
//! `nanos`, `us`, `ns` or `sec` for a duration, `cents`, `pence`, `minor` or `micros` for money (`timeout_ms`, `amount_cents` document
//! their own unit, which is the point of a newtype). A generic parameter, a reference to a user type and every other type are not primitives.
//!
//! One finding per function, at its signature, naming each parameter and its class. Low severity and Low confidence (the catalogue record's own:
//! "only where confusion is plausible or an invariant exists", which a name cannot know).
//!
//! **Not judged:** return types, struct fields, a type alias that hides the primitive, a newtype that is itself a bare wrapper, parameters whose
//! name is not one of the words above, other languages. A file `syn` cannot parse is counted in a note, never read as clean.

use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{
    FnArg, ImplItem, ItemFn, ItemImpl, ItemMod, ItemTrait, Pat, Type, TypeReference, Visibility,
};

const MONEY: &[&str] = &[
    "price", "amount", "money", "cost", "balance", "fee", "salary",
];
const DURATION: &[&str] = &["duration", "timeout", "ttl", "delay"];
const DURATION_UNITS: &[&str] = &[
    "ms", "millis", "secs", "seconds", "minutes", "hours", "days", "micros", "nanos", "us", "ns",
    "sec",
];
const MONEY_UNITS: &[&str] = &["cents", "pence", "minor", "micros"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    Money,
    Duration,
    Id,
}

impl Class {
    fn word(self) -> &'static str {
        match self {
            Self::Money => "money",
            Self::Duration => "duration",
            Self::Id => "identifier",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub line: u32,
    pub function: String,
    /// `(parameter name, class, its type as written)`.
    pub params: Vec<(String, Class, String)>,
}

fn segments(name: &str) -> Vec<String> {
    name.to_ascii_lowercase()
        .split('_')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The domain class a parameter name says, or `None`.
pub(super) fn class_of(name: &str) -> Option<Class> {
    let segs = segments(name);
    let has = |set: &[&str]| segs.iter().any(|s| set.contains(&s.as_str()));
    if has(MONEY) && !has(MONEY_UNITS) {
        Some(Class::Money)
    } else if has(DURATION) && !has(DURATION_UNITS) {
        Some(Class::Duration)
    } else if segs.last().is_some_and(|s| s == "id") {
        Some(Class::Id)
    } else {
        None
    }
}

/// The primitive a type is, written as `u64`, `f64`, `String`, `&str`: its text, or `None` for anything else.
fn primitive(ty: &Type) -> Option<(String, bool, bool)> {
    // (text, is a number, is text)
    match ty {
        Type::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
            let id = p.path.segments[0].ident.to_string();
            let number = matches!(
                id.as_str(),
                "u8" | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "f32"
                    | "f64"
            );
            let text = id == "String";
            (number || text).then_some((id, number, text))
        }
        Type::Reference(TypeReference { elem, .. }) => match &**elem {
            Type::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
                let id = p.path.segments[0].ident.to_string();
                matches!(id.as_str(), "str" | "String").then(|| (format!("&{id}"), false, true))
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_int(text: &str) -> bool {
    text.starts_with('u') || text.starts_with('i')
}

/// Whether a parameter of this class may be typed as this primitive and be reported.
fn reportable(class: Class, number: bool, text: bool, name: &str) -> bool {
    match class {
        Class::Money | Class::Duration => number,
        Class::Id => number && is_int(name) || text,
    }
}

fn hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("doc") && a.parse_args::<syn::Ident>().is_ok_and(|i| i == "hidden")
    })
}

struct Scan {
    out: Vec<Site>,
}

impl Scan {
    fn function(&mut self, name: &str, sig: &syn::Signature) {
        let mut params = Vec::new();
        for arg in &sig.inputs {
            let FnArg::Typed(t) = arg else {
                continue;
            };
            let Pat::Ident(p) = &*t.pat else {
                continue;
            };
            let pname = p.ident.to_string();
            let Some(class) = class_of(&pname) else {
                continue;
            };
            let Some((text, number, is_text)) = primitive(&t.ty) else {
                continue;
            };
            if reportable(class, number, is_text, &text) {
                params.push((pname, class, text));
            }
        }
        if !params.is_empty() {
            self.out.push(Site {
                line: line_of(sig.ident.span()),
                function: name.to_string(),
                params,
            });
        }
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if matches!(f.vis, Visibility::Public(_))
            && !f.attrs.iter().any(is_test_attr)
            && !hidden(&f.attrs)
        {
            self.function(&f.sig.ident.to_string(), &f.sig);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        // `impl Trait for Type`: the trait dictates the signature
        if i.trait_.is_some() || i.attrs.iter().any(is_test_attr) || hidden(&i.attrs) {
            return;
        }
        for item in &i.items {
            if let ImplItem::Fn(f) = item {
                if matches!(f.vis, Visibility::Public(_))
                    && !f.attrs.iter().any(is_test_attr)
                    && !hidden(&f.attrs)
                {
                    self.function(&f.sig.ident.to_string(), &f.sig);
                }
            }
        }
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        if !matches!(t.vis, Visibility::Public(_))
            || t.attrs.iter().any(is_test_attr)
            || hidden(&t.attrs)
        {
            return;
        }
        for item in &t.items {
            if let syn::TraitItem::Fn(f) = item {
                self.function(&f.sig.ident.to_string(), &f.sig);
            }
        }
    }
}

/// Whether a path is library code this rule reads.
fn is_library_file(path: &str) -> bool {
    let segs: Vec<&str> = path.split('/').collect();
    let (dirs, file) = segs.split_at(segs.len() - 1);
    !dirs
        .iter()
        .any(|s| matches!(*s, "tests" | "examples" | "benches" | "bin"))
        && !matches!(file[0], "main.rs" | "build.rs" | "tests.rs")
}

pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan { out: Vec::new() };
    scan.visit_file(&parsed);
    Some(scan.out)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let id = crate::rule_id::TYP_001;
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p) && is_library_file(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust library source file outside tests, examples and binaries".into(),
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
            let list = site
                .params
                .iter()
                .map(|(n, c, t)| format!("`{n}: {t}` ({})", c.word()))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(
                Finding::new(
                    id,
                    Severity::Low,
                    Confidence::Low,
                    view.name(),
                    format!(
                        "public `{}` takes a domain value as a bare primitive: {list}",
                        site.function
                    ),
                    format!(
                        "`{}` in `{path}` is public and takes {list}. A bare number or string can be an id, a count, a price or a duration, and the \
                         compiler cannot tell a swapped pair of arguments apart. Introduce a newtype (or `std::time::Duration` for a duration) where \
                         confusion is plausible or an invariant exists; if the name is only a name and the type is deliberate, say so in the signature's \
                         documentation. Read from the name and the type only: whether confusion is plausible is not judged.",
                        site.function
                    ),
                )
                .location(Location::new(path.as_str(), Some(site.line)))
                .subject(format!("{path}:{}", site.function))
                .positive_control(format!(
                    "{} Rust file(s) parsed with syn; the signature was read at {path}:{}",
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

    fn got(src: &str) -> Vec<(String, Vec<(String, Class)>)> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| {
                (
                    s.function,
                    s.params.into_iter().map(|(n, c, _)| (n, c)).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn money_duration_and_identifier_parameters_typed_as_primitives_are_reported() {
        let src = "pub fn charge(user_id: u64, amount: f64, timeout: u32) {}";
        assert_eq!(
            got(src),
            vec![(
                "charge".to_string(),
                vec![
                    ("user_id".to_string(), Class::Id),
                    ("amount".to_string(), Class::Money),
                    ("timeout".to_string(), Class::Duration)
                ]
            )]
        );
    }

    #[test]
    fn a_name_that_says_its_unit_a_newtype_and_a_generic_are_not_reported() {
        let src = "pub fn f(timeout_ms: u64, amount_cents: i64, d: std::time::Duration, price: Money, id: T, user_id: UserId) {}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn an_identifier_may_be_a_string_but_money_and_duration_must_be_numbers() {
        assert_eq!(got("pub fn f(order_id: &str) {}").len(), 1);
        assert_eq!(got("pub fn f(id: String) {}").len(), 1);
        assert!(got("pub fn f(price: String, timeout: &str) {}").is_empty());
        assert!(got("pub fn f(id: f64) {}").is_empty());
    }

    #[test]
    fn only_public_functions_of_inherent_impls_and_public_traits_are_read() {
        let src = "fn private(id: u64) {}\npub(crate) fn p2(id: u64) {}\nstruct S;\nimpl S { pub fn m(&self, id: u64) {} fn q(&self, id: u64) {} }\nimpl Trait for S { fn t(&self, id: u64) {} }\npub trait T { fn x(&self, id: u64); }\ntrait Hidden { fn y(&self, id: u64); }";
        let names: Vec<String> = got(src).into_iter().map(|(f, _)| f).collect();
        assert_eq!(names, vec!["m", "x"]);
    }

    #[test]
    fn test_code_and_hidden_items_are_skipped() {
        let src = "#[cfg(test)]\nmod tests { pub fn f(id: u64) {} }\n#[doc(hidden)]\npub fn g(id: u64) {}\n#[test]\npub fn h(id: u64) {}";
        assert!(got(src).is_empty());
    }

    #[test]
    fn library_paths_exclude_tests_examples_binaries_and_build_scripts() {
        assert!(is_library_file("src/lib.rs"));
        assert!(is_library_file("crates/a/src/api.rs"));
        for p in [
            "tests/x.rs",
            "examples/y.rs",
            "src/main.rs",
            "src/bin/z.rs",
            "build.rs",
            "benches/b.rs",
        ] {
            assert!(!is_library_file(p), "{p}");
        }
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("pub fn (((").is_none());
    }
}
