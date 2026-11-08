// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-004, API-009, COR-001 and ERR-004: four Rust rules that read the public surface of a library from the source alone.
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories and outside binary roots
//! (`main.rs`, `src/bin/`) is parsed with `syn`. An item is public here when its own visibility is a bare `pub` and every inline
//! module above it is `pub`; items under `#[cfg(test)]`, `#[test]` functions and `#[doc(hidden)]` items are left out; the methods
//! read are the `pub` methods of inherent `impl` blocks (never a trait impl, whose contract lives on the trait) of a type that is
//! not private in the same file. One walk yields four kinds of site:
//!
//! * **API-004, a deprecation without lifecycle data.** `#[deprecated]` with no `since` or with no `note` (the note is where the
//!   replacement and the removal target are written). `#[deprecated = "text"]` has a note and no `since`. Whether the note names a
//!   replacement is not read: a note is the author's statement of one.
//! * **API-009, an exported type that cannot grow.** A `pub enum` with variants, or a `pub struct` with named fields that are all
//!   `pub`, with no `#[non_exhaustive]` and no doc comment that says the type is closed (the words `exhaustive`, `closed`,
//!   `sealed`, `fixed set`). Adding a variant or a field to such a type is a breaking change.
//! * **COR-001, a public function whose docs omit its contract.** A `pub fn` that returns `Result` and has no `# Errors` section, a
//!   `pub unsafe fn` with no `# Safety` section, or a `pub fn` whose body names `panic!`, `unreachable!`, `unimplemented!`, `todo!`
//!   or an `assert!`-family macro (not `debug_assert!`) and has no `# Panics` section. A section is a doc line that starts with `#`
//!   and names the word.
//! * **ERR-004, a public function that fails with an untyped error.** A `pub fn` returning `Result<_, E>` where `E` is `String`,
//!   `&str`, `Box<dyn ..>`, `anyhow::Error` or `eyre::Report`, or `anyhow::Result<_>`.
//!
//! **What it cannot see (and the record says so):** whether an item is reachable from the crate root through `pub use` or a
//! non-inline module file; items made by macros; `unwrap()` and `expect()` panics (too many are infallible, so only explicit panic
//! macros are read); a panic reached through a callee; whether a deprecation note really names a replacement; a type alias that
//! hides the error type; other languages. A file `syn` cannot parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeSet;
use syn::visit::Visit;
use syn::{
    Attribute, Fields, GenericArgument, ImplItem, Item, Meta, PathArguments, ReturnType, Signature,
    TraitItem, Type, Visibility,
};

/// What kind of site a walk found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    /// API-004
    Deprecation,
    /// API-009
    Closed,
    /// COR-001
    Contract,
    /// ERR-004
    UntypedError,
}

/// One place a rule is about.
#[derive(Clone, Debug)]
pub(super) struct Site {
    pub kind: Kind,
    pub line: u32,
    /// The item: a function, type or method name.
    pub name: String,
    /// What is missing or what was found, in words.
    pub detail: String,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
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
    attr.path().is_ident("cfg") && attr.parse_args::<Meta>().is_ok_and(|m| cfg_is_test(&m))
}

/// `test`, or `all(..)` with `test` among its terms: the item exists only in a test build. `any(..)` and `not(..)` do not.
fn cfg_is_test(meta: &Meta) -> bool {
    if meta.path().is_ident("test") {
        return true;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    if !list.path.is_ident("all") {
        return false;
    }
    list.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .is_ok_and(|terms| terms.iter().any(cfg_is_test))
}

fn is_doc_hidden(attr: &Attribute) -> bool {
    attr.path().is_ident("doc")
        && attr
            .parse_args::<Meta>()
            .is_ok_and(|m| m.path().is_ident("hidden"))
}

/// An item that counts: not a test, not hidden from the documentation.
fn visible(attrs: &[Attribute]) -> bool {
    !attrs.iter().any(|a| is_test_attr(a) || is_doc_hidden(a))
}

/// The text of the doc comment, one line per `#[doc = ".."]`.
fn doc_text(attrs: &[Attribute]) -> String {
    let mut out = String::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = &nv.value
        {
            out.push_str(&s.value());
            out.push('\n');
        }
    }
    out
}

/// Whether the doc has a section: a line that starts with `#` and names `word`.
fn has_section(doc: &str, word: &str) -> bool {
    doc.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with('#') && t.contains(word)
    })
}

fn deprecation(attrs: &[Attribute], name: &str, out: &mut Vec<Site>) {
    for attr in attrs {
        if !attr.path().is_ident("deprecated") {
            continue;
        }
        let (mut since, mut note) = (false, false);
        match &attr.meta {
            Meta::Path(_) => {}
            Meta::NameValue(_) => note = true,
            Meta::List(_) => {
                let _ = attr.parse_nested_meta(|m| {
                    if m.path.is_ident("since") {
                        since = true;
                    } else if m.path.is_ident("note") {
                        note = true;
                    }
                    // consume the value so the next key parses
                    let _ = m.value().and_then(|v| v.parse::<syn::Expr>());
                    Ok(())
                });
            }
        }
        let mut missing = Vec::new();
        if !since {
            missing.push("the version it began in (`since`)");
        }
        if !note {
            missing.push("a note (what replaces it, when it goes)");
        }
        if !missing.is_empty() {
            out.push(Site {
                kind: Kind::Deprecation,
                line: line_of(attr.pound_token.spans[0]),
                name: name.to_string(),
                detail: format!("carries no {}", missing.join(" and no ")),
            });
        }
    }
}

const CLOSED_WORDS: &[&str] = &["exhaustive", "closed", "sealed", "fixed set"];

fn says_closed(attrs: &[Attribute]) -> bool {
    let mut doc = doc_text(attrs).to_lowercase();
    // a negated phrase says the opposite: "not exhaustive", "non-exhaustive", "isn't closed"
    for neg in ["not ", "non-", "non ", "n't ", "never ", "no "] {
        for w in CLOSED_WORDS {
            doc = doc.replace(&format!("{neg}{w}"), "");
        }
    }
    CLOSED_WORDS.iter().any(|w| doc.contains(w))
}

fn closed_type(attrs: &[Attribute], item: &Item, out: &mut Vec<Site>) {
    if attrs.iter().any(|a| a.path().is_ident("non_exhaustive")) || says_closed(attrs) {
        return;
    }
    match item {
        Item::Enum(e) => {
            let n = e.variants.iter().filter(|v| visible(&v.attrs)).count();
            if n > 0 {
                out.push(Site {
                    kind: Kind::Closed,
                    line: line_of(e.ident.span()),
                    name: e.ident.to_string(),
                    detail: format!("is a public enum of {n} variant(s)"),
                });
            }
        }
        Item::Struct(s) => {
            let named = match &s.fields {
                Fields::Named(f) if f.named.iter().all(|x| is_pub(&x.vis)) => Some(f),
                _ => None,
            };
            if let Some(f) = named {
                out.push(Site {
                    kind: Kind::Closed,
                    line: line_of(s.ident.span()),
                    name: s.ident.to_string(),
                    detail: format!(
                        "is a public struct whose {} field(s) are all public",
                        f.named.len()
                    ),
                });
            }
        }
        _ => {}
    }
}

/// The last two segments of a path, `a::b` for `x::a::b`.
fn tail(path: &syn::Path) -> String {
    let n = path.segments.len();
    path.segments
        .iter()
        .skip(n.saturating_sub(2))
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// The untyped error a `Result` return names, if it is one.
fn untyped_error(sig: &Signature) -> Option<String> {
    let ReturnType::Type(_, ty) = &sig.output else {
        return None;
    };
    let Type::Path(p) = &**ty else { return None };
    let last = p.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let alias = tail(&p.path);
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let types: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    if (alias == "anyhow::Result" || alias == "eyre::Result") && types.len() == 1 {
        return Some(alias);
    }
    let err = types.get(1)?;
    match err {
        Type::Reference(r) => match &*r.elem {
            Type::Path(q) if q.path.is_ident("str") => Some("&str".into()),
            _ => None,
        },
        Type::Path(q) => {
            let t = tail(&q.path);
            let last = q.path.segments.last()?.ident.to_string();
            let std_string = matches!(
                q.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
                    .as_str(),
                "std::string::String" | "alloc::string::String" | "string::String"
            );
            if std_string {
                Some("String".into())
            } else if t == "String"
                || t == "anyhow::Error"
                || t == "eyre::Report"
                || t == "eyre::Error"
            {
                Some(t)
            } else if last == "Box" {
                match &q.path.segments.last()?.arguments {
                    PathArguments::AngleBracketed(b)
                        if b.args
                            .iter()
                            .any(|a| matches!(a, GenericArgument::Type(Type::TraitObject(_)))) =>
                    {
                        Some("Box<dyn ..>".into())
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn returns_result(sig: &Signature) -> bool {
    matches!(&sig.output, ReturnType::Type(_, ty)
        if matches!(&**ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Result")))
}

/// Finds an explicit panic in a body.
#[derive(Default)]
struct Panics {
    found: Option<String>,
}

impl<'ast> Visit<'ast> for Panics {
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = m.path.segments.last().map(|s| s.ident.to_string());
        if let (None, Some(name)) = (&self.found, name) {
            if matches!(
                name.as_str(),
                "panic"
                    | "unreachable"
                    | "unimplemented"
                    | "todo"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
            ) {
                self.found = Some(format!("{name}!"));
            }
        }
        syn::visit::visit_macro(self, m);
    }
}

fn function(attrs: &[Attribute], sig: &Signature, body: Option<&syn::Block>, out: &mut Vec<Site>) {
    let name = sig.ident.to_string();
    deprecation(attrs, &name, out);
    let line = line_of(sig.ident.span());
    let doc = doc_text(attrs);
    let mut missing: Vec<String> = Vec::new();
    if returns_result(sig) && !has_section(&doc, "Errors") {
        missing.push("`# Errors` (it returns a `Result`)".into());
    }
    if matches!(sig.safety, syn::Safety::Unsafe(_)) && !has_section(&doc, "Safety") {
        missing.push("`# Safety` (it is `unsafe`)".into());
    }
    if let Some(b) = body {
        let mut p = Panics::default();
        p.visit_block(b);
        if let Some(m) = p.found {
            if !has_section(&doc, "Panics") {
                missing.push(format!("`# Panics` (its body names `{m}`)"));
            }
        }
    }
    if !missing.is_empty() {
        out.push(Site {
            kind: Kind::Contract,
            line,
            name: name.clone(),
            detail: format!("has no {} in its docs", missing.join(" and no ")),
        });
    }
    if let Some(e) = untyped_error(sig) {
        out.push(Site {
            kind: Kind::UntypedError,
            line,
            name,
            detail: format!("fails with `{e}`, which a caller cannot branch on"),
        });
    }
}

fn private_type(item: &Item) -> Option<String> {
    match item {
        Item::Struct(s) if !is_pub(&s.vis) => Some(s.ident.to_string()),
        Item::Enum(e) if !is_pub(&e.vis) => Some(e.ident.to_string()),
        Item::Union(u) if !is_pub(&u.vis) => Some(u.ident.to_string()),
        _ => None,
    }
}

fn self_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn walk(items: &[Item], out: &mut Vec<Site>) {
    let private: BTreeSet<String> = items.iter().filter_map(private_type).collect();
    for item in items {
        match item {
            Item::Fn(f) if is_pub(&f.vis) && visible(&f.attrs) => {
                function(&f.attrs, &f.sig, Some(&f.block), out)
            }
            Item::Struct(s) if is_pub(&s.vis) && visible(&s.attrs) => {
                deprecation(&s.attrs, &s.ident.to_string(), out);
                for f in s.fields.iter() {
                    if let (true, true, Some(id)) = (is_pub(&f.vis), visible(&f.attrs), &f.ident) {
                        deprecation(&f.attrs, &format!("{}.{id}", s.ident), out);
                    }
                }
                closed_type(&s.attrs, item, out);
            }
            Item::Enum(e) if is_pub(&e.vis) && visible(&e.attrs) => {
                deprecation(&e.attrs, &e.ident.to_string(), out);
                for v in e.variants.iter().filter(|v| visible(&v.attrs)) {
                    deprecation(&v.attrs, &format!("{}::{}", e.ident, v.ident), out);
                }
                closed_type(&e.attrs, item, out);
            }
            Item::Union(u) if is_pub(&u.vis) && visible(&u.attrs) => {
                deprecation(&u.attrs, &u.ident.to_string(), out)
            }
            Item::Const(c) if is_pub(&c.vis) && visible(&c.attrs) => {
                deprecation(&c.attrs, &c.ident.to_string(), out)
            }
            Item::Static(c) if is_pub(&c.vis) && visible(&c.attrs) => {
                deprecation(&c.attrs, &c.ident.to_string(), out)
            }
            Item::Type(c) if is_pub(&c.vis) && visible(&c.attrs) => {
                deprecation(&c.attrs, &c.ident.to_string(), out)
            }
            Item::Trait(t) if is_pub(&t.vis) && visible(&t.attrs) => {
                deprecation(&t.attrs, &t.ident.to_string(), out);
                for ti in &t.items {
                    match ti {
                        TraitItem::Fn(f) if visible(&f.attrs) => {
                            function(&f.attrs, &f.sig, f.default.as_ref(), out)
                        }
                        TraitItem::Const(c) if visible(&c.attrs) => {
                            deprecation(&c.attrs, &c.ident.to_string(), out)
                        }
                        TraitItem::Type(c) if visible(&c.attrs) => {
                            deprecation(&c.attrs, &c.ident.to_string(), out)
                        }
                        _ => {}
                    }
                }
            }
            Item::Impl(i)
                if i.trait_.is_none()
                    && visible(&i.attrs)
                    && !self_name(&i.self_ty).is_some_and(|n| private.contains(&n)) =>
            {
                for ii in &i.items {
                    if let ImplItem::Fn(f) = ii {
                        if is_pub(&f.vis) && visible(&f.attrs) {
                            function(&f.attrs, &f.sig, Some(&f.block), out);
                        }
                    }
                }
            }
            Item::Mod(m) if is_pub(&m.vis) && visible(&m.attrs) => {
                deprecation(&m.attrs, &m.ident.to_string(), out);
                if let Some((_, inner)) = &m.content {
                    walk(inner, out);
                }
            }
            _ => {}
        }
    }
}

/// The sites of one file, or `None` when `syn` cannot parse it.
pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut out = Vec::new();
    walk(&parsed.items, &mut out);
    out.sort_by_key(|s| (s.line, s.kind as u8));
    out.dedup_by_key(|s| (s.kind as u8, s.line, s.name.clone()));
    Some(out)
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

fn is_binary_root(path: &str) -> bool {
    path == "main.rs"
        || path.ends_with("/main.rs")
        || path.starts_with("src/bin/")
        || path.contains("/src/bin/")
}

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
        .filter(|p| p.ends_with(".rs") && !skipped(p) && !is_dev_path(p) && !is_binary_root(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust library source file outside tests and binary roots"
                .into(),
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
                    .subject(format!("{}:{}:{}", path, site.name, site.line))
                    .positive_control(format!(
                        "{} Rust file(s) parsed with syn; the item was read at {path}:{}",
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

fn deprecation_text(s: &Site) -> (String, String) {
    (
        format!("the deprecation of `{}` {}", s.name, s.detail),
        "A bare deprecation tells consumers to worry but not how or when to act. Write `#[deprecated(since = \"x.y.z\", note = \"use `new_item` instead; removed in x.(y+1).0\")]`: the version it began in, what replaces it and when it goes.".to_string(),
    )
}

fn closed_text(s: &Site) -> (String, String) {
    (
        format!("`{}` {} and is not marked `#[non_exhaustive]`", s.name, s.detail),
        "Adding a variant or a field to this type later is a breaking change that forces a major version. Mark it `#[non_exhaustive]` if it will grow, or say in its docs that it is closed (the words `closed`, `exhaustive` or `sealed`) if it will not.".to_string(),
    )
}

fn contract_text(s: &Site) -> (String, String) {
    (
        format!("`{}` {}", s.name, s.detail),
        "A contract only the author knows cannot be honoured by callers or checked by reviewers. Add the section to the doc comment: `# Errors` lists what makes it return `Err`, `# Panics` what makes it panic, `# Safety` what a caller of an `unsafe` function must guarantee.".to_string(),
    )
}

fn untyped_text(s: &Site) -> (String, String) {
    (
        format!("`{}` {}", s.name, s.detail),
        "Callers that must parse message text to decide what to do break whenever the wording changes, so the error shape is part of the API. Return an enum of the library's failures (with `thiserror` or by hand) and let applications wrap it with context.".to_string(),
    )
}

pub(super) fn judge_api004(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::API_004,
        Kind::Deprecation,
        Severity::Low,
        Confidence::Medium,
        deprecation_text,
    )
}

pub(super) fn judge_api009(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::API_009,
        Kind::Closed,
        Severity::Low,
        Confidence::Medium,
        closed_text,
    )
}

pub(super) fn judge_cor001(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::COR_001,
        Kind::Contract,
        Severity::Low,
        Confidence::Medium,
        contract_text,
    )
}

pub(super) fn judge_err004(view: &RepoView) -> Verdict {
    judge_kind(
        view,
        crate::rule_id::ERR_004,
        Kind::UntypedError,
        Severity::Medium,
        Confidence::Medium,
        untyped_text,
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
    fn a_deprecation_needs_a_version_and_a_note() {
        let src = r#"
#[deprecated]
pub fn bare() {}
#[deprecated = "use b"]
pub fn note_only() {}
#[deprecated(since = "1.2.0")]
pub fn since_only() {}
#[deprecated(since = "1.2.0", note = "use `b`; removed in 2.0")]
pub fn whole() {}
"#;
        let found = run(src, Kind::Deprecation);
        let names: Vec<_> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["bare", "note_only", "since_only"], "{found:?}");
    }

    #[test]
    fn a_public_enum_without_the_marker_is_reported_unless_it_says_it_is_closed() {
        let src = r#"
pub enum Open { A, B }
#[non_exhaustive]
pub enum Marked { A }
/// The set is closed: it will not gain variants.
pub enum Said { A }
enum Private { A }
pub struct Bag { pub a: u8, pub b: u8 }
pub struct Hidden { pub a: u8, b: u8 }
pub struct Tuple(pub u8);
"#;
        let names: Vec<_> = run(src, Kind::Closed).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Open", "Bag"]);
    }

    #[test]
    fn the_contract_sections_are_required_where_the_signature_or_body_asks() {
        let src = r#"
pub fn bare() -> Result<(), E> { Ok(()) }
/// Does it.
///
/// # Errors
/// When it fails.
pub fn documented() -> Result<(), E> { Ok(()) }
pub unsafe fn raw() {}
/// # Safety
/// Caller holds the lock.
pub unsafe fn raw_ok() {}
pub fn loud(x: u8) { assert!(x > 0); }
/// # Panics
/// When x is zero.
pub fn loud_ok(x: u8) { assert!(x > 0); }
pub fn quiet(x: u8) { debug_assert!(x > 0); let _ = x.checked_add(1).unwrap(); }
fn private() -> Result<(), E> { Ok(()) }
"#;
        let names: Vec<_> = run(src, Kind::Contract)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["bare", "raw", "loud"]);
    }

    #[test]
    fn methods_of_public_types_are_read_and_trait_impls_and_private_types_are_not() {
        let src = r#"
pub struct P;
struct Q;
impl P {
    pub fn m(&self) -> Result<(), String> { Ok(()) }
    fn hidden(&self) -> Result<(), String> { Ok(()) }
}
impl Q {
    pub fn n(&self) -> Result<(), String> { Ok(()) }
}
impl std::str::FromStr for P {
    type Err = String;
    fn from_str(_: &str) -> Result<Self, String> { Ok(P) }
}
"#;
        let names: Vec<_> = run(src, Kind::UntypedError)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["m"]);
    }

    #[test]
    fn review_cases_negation_cfg_all_union_variants_trait_fns_paths_and_aliases() {
        let closed = |src: &str| run(src, Kind::Closed).len();
        // a negated phrase does not say the type is closed
        assert_eq!(closed("/// Not exhaustive.\npub enum E { A }\n"), 1);
        assert_eq!(closed("/// This set is closed.\npub enum E { A }\n"), 0);
        // an empty named struct can be built as `S {}`
        assert_eq!(closed("pub struct S {}\n"), 1);
        // a hidden variant does not count
        assert_eq!(closed("pub enum E { #[doc(hidden)] A }\n"), 0);
        // test-only under all(..) is left out, any(..) is not
        assert_eq!(closed("#[cfg(all(test, unix))]\npub enum E { A }\n"), 0);
        assert_eq!(closed("#[cfg(any(test, unix))]\npub enum E { A }\n"), 1);
        let dep = |src: &str| run(src, Kind::Deprecation).len();
        assert_eq!(dep("pub struct S { #[deprecated] pub x: u8 }\n"), 1);
        assert_eq!(dep("#[deprecated]\npub mod m {}\n"), 1);
        assert_eq!(dep("#[deprecated]\npub union U { a: u8 }\n"), 1);
        assert_eq!(
            dep("pub trait T { #[deprecated] const C: u8; #[deprecated] type A; }\n"),
            2
        );
        assert_eq!(dep("pub enum E { #[doc(hidden)] #[deprecated] A }\n"), 0);
        let err = |src: &str| run(src, Kind::UntypedError).len();
        assert_eq!(
            err("pub trait T { fn f(&self) -> Result<(), String>; }\n"),
            1
        );
        assert_eq!(
            err("pub fn f() -> Result<(), std::string::String> { Ok(()) }\n"),
            1
        );
        assert_eq!(
            err("pub fn f() -> anyhow::Result<u8, MyError> { Ok(1) }\n"),
            0
        );
        assert_eq!(err("pub fn f() -> eyre::Result<u8> { Ok(1) }\n"), 1);
        // a method of a private union is not public
        assert_eq!(
            err("union U { a: u8 }\nimpl U { pub fn f() -> Result<(), String> { Ok(()) } }\n"),
            0
        );
    }

    #[test]
    fn untyped_errors_are_strings_str_boxed_dyn_and_anyhow() {
        let src = r#"
pub fn a() -> Result<u8, String> { Ok(1) }
pub fn b() -> Result<u8, &'static str> { Ok(1) }
pub fn c() -> Result<u8, Box<dyn std::error::Error>> { Ok(1) }
pub fn d() -> anyhow::Result<u8> { Ok(1) }
pub fn e() -> Result<u8, anyhow::Error> { Ok(1) }
pub fn f() -> Result<u8, MyError> { Ok(1) }
pub fn g() -> std::io::Result<u8> { Ok(1) }
pub fn h() -> Result<u8, Box<MyError>> { Ok(1) }
pub fn i() -> u8 { 1 }
"#;
        let names: Vec<_> = run(src, Kind::UntypedError)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn tests_hidden_items_and_private_modules_are_left_out() {
        let src = r#"
#[cfg(test)]
mod tests { pub fn t() -> Result<(), String> { Ok(()) } }
#[doc(hidden)]
pub fn h() -> Result<(), String> { Ok(()) }
mod inner { pub fn p() -> Result<(), String> { Ok(()) } }
pub mod outer { pub fn o() -> Result<(), String> { Ok(()) } }
#[test]
pub fn tt() -> Result<(), String> { Ok(()) }
"#;
        let names: Vec<_> = run(src, Kind::UntypedError)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["o"]);
    }

    #[test]
    fn an_unparsable_file_is_none() {
        assert!(sites("pub fn (((").is_none());
    }
}
