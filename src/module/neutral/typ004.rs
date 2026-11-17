// SPDX-License-Identifier: MIT OR Apache-2.0
//! TYP-004: outside data becomes a domain type where it enters, not a bare string deep inside.
//!
//! **What this judges (Rust, from the source alone, as a deliberately narrow syntactic approximation of a data-flow rule):** every
//! tracked `.rs` file of a package (outside vendored directories, tests, examples, tools and `build.rs`, and outside `#[cfg(test)]`
//! items) is parsed with `syn`. A `pub` function or inherent method (not a trait-impl method) is reported for a parameter that is a
//! bare string (`String`, `&str`, `&String`, `Cow<str>`, `impl AsRef<str>`, `impl Into<String>`) whose name has a **domain word** among its
//! `_`-separated words: `email`, `url`, `uri`, `hostname`, `uuid`, `ip`, `phone`, `iban`, `currency`, `locale`, `timezone`, `isbn`,
//! `postcode`, `zipcode`. Such a value has rules (a shape, a range, a normal form) that a domain type checks once, where the data enters,
//! so that the interior only ever holds a valid one. A function whose own name says it is that entry (a word `parse`, `from`, `try`,
//! `new`, `validate`, `is`, `check`, `verify`, `normalize`, `sanitize`, `deserialize`, `decode`, `read`, `load`, `lookup`, `find`, `get`,
//! `with`, `set`) is not reported, since taking the raw text is its job. One **Medium**-severity, Low-confidence finding per parameter, at
//! the function. The record marks this rule *contested*: the finding is a prompt, not a proof.
//!
//! **What it cannot see:** where the data really enters the program, a domain type that is itself a `String` alias, parameters of another
//! shape (a bare `u64` that is an id, a `Vec<u8>` that is a token), a name outside the word list, and the other languages. A file `syn`
//! cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;

const ID: &str = crate::rule_id::TYP_004;

const DOMAIN_WORDS: &[&str] = &[
    "email", "url", "uri", "hostname", "uuid", "ip", "phone", "iban", "currency", "locale",
    "timezone", "isbn", "postcode", "zipcode",
];

const ENTRY_WORDS: &[&str] = &[
    "parse",
    "from",
    "try",
    "new",
    "validate",
    "is",
    "check",
    "verify",
    "normalize",
    "sanitize",
    "deserialize",
    "decode",
    "read",
    "load",
    "lookup",
    "find",
    "get",
    "with",
    "set",
];

fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split('_')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn bare_string(t: &syn::Type) -> bool {
    match t {
        syn::Type::Reference(r) => bare_string(&r.elem),
        syn::Type::Paren(p) => bare_string(&p.elem),
        syn::Type::Path(p) => {
            let Some(last) = p.path.segments.last() else {
                return false;
            };
            match last.ident.to_string().as_str() {
                "String" | "str" => true,
                "Cow" => {
                    matches!(&last.arguments, syn::PathArguments::AngleBracketed(a)
                        if a.args.iter().any(|g| matches!(g, syn::GenericArgument::Type(t) if bare_string(t))))
                }
                _ => false,
            }
        }
        syn::Type::ImplTrait(i) => i.bounds.iter().any(|b| match b {
            syn::TypeParamBound::Trait(t) => t.path.segments.last().is_some_and(|s| {
                (s.ident == "AsRef" || s.ident == "Into")
                    && matches!(&s.arguments, syn::PathArguments::AngleBracketed(a)
                        if a.args.iter().any(|g| matches!(g, syn::GenericArgument::Type(t) if bare_string(t))))
            }),
            _ => false,
        }),
        _ => false,
    }
}

fn param_name(p: &syn::Pat) -> Option<String> {
    match p {
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        syn::Pat::Type(t) => param_name(&t.pat),
        _ => None,
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

fn is_pub(v: &syn::Visibility) -> bool {
    matches!(v, syn::Visibility::Public(_))
}

struct Site {
    line: u32,
    function: String,
    parameter: String,
    word: String,
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
    trait_impl: u32,
}

impl Walk {
    fn signature(&mut self, sig: &syn::Signature) {
        let fname = sig.ident.to_string();
        if words(&fname)
            .iter()
            .any(|w| ENTRY_WORDS.contains(&w.as_str()))
        {
            return;
        }
        for input in &sig.inputs {
            let syn::FnArg::Typed(t) = input else {
                continue;
            };
            let Some(name) = param_name(&t.pat) else {
                continue;
            };
            if !bare_string(&t.ty) {
                continue;
            }
            if let Some(word) = words(&name)
                .into_iter()
                .find(|w| DOMAIN_WORDS.contains(&w.as_str()))
            {
                self.sites.push(Site {
                    line: u32::try_from(sig.ident.span().start().line).unwrap_or(0),
                    function: fname.clone(),
                    parameter: name,
                    word,
                });
            }
        }
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
        if is_pub(&i.vis) {
            self.signature(&i.sig);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let tr = u32::from(i.trait_.is_some());
        self.trait_impl += tr;
        syn::visit::visit_item_impl(self, i);
        self.trait_impl -= tr;
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        if self.trait_impl == 0 && is_pub(&i.vis) {
            self.signature(&i.sig);
        }
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return super::rust_package::nothing_to_read(ID, &loaded);
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
                        format!(
                            "`{}` takes `{}` as a bare string, though it holds a {} value",
                            site.function, site.parameter, site.word
                        ),
                        "A value with a shape or a range should be checked once, where it enters the program, and passed on as a domain type that cannot hold an invalid one. Add a parsing step that returns such a type and take that type here, or allowlist the function with the reason if it is the entry point.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!(
                        "{}:{}:{}",
                        s.path, site.function, site.parameter
                    ))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads public signatures and their parameter types"
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
    fn a_public_function_taking_a_bare_string_for_a_domain_value_is_reported() {
        let src = "pub fn send_welcome(email: &str, user_url: String, locale: impl AsRef<str>) {}
pub struct Mailer;
impl Mailer { pub fn deliver(&self, to_email: std::borrow::Cow<'_, str>) {} }
";
        let f = found(src);
        let subjects: Vec<&str> = f.iter().map(|x| x.subject.as_deref().unwrap()).collect();
        assert_eq!(
            subjects,
            [
                "src/lib.rs:send_welcome:email",
                "src/lib.rs:send_welcome:user_url",
                "src/lib.rs:send_welcome:locale",
                "src/lib.rs:deliver:to_email"
            ]
        );
    }

    #[test]
    fn the_entry_function_a_domain_type_a_private_function_and_a_trait_impl_are_not_reported() {
        let src = "pub fn parse_email(email: &str) -> Option<Email> { None }
pub fn is_valid_url(url: &str) -> bool { true }
pub fn send(email: Email, name: &str) {}
fn private(email: &str) {}
pub fn count(total: String) {}
pub struct S;
impl std::str::FromStr for S { type Err = (); fn from_str(email: &str) -> Result<S, ()> { Ok(S) } }
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn test_code_is_not_read() {
        let src =
            "#[cfg(test)]\nmod t { pub fn f(email: &str) {} }\n#[test]\npub fn g(email: &str) {}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(email: &str) {}\n") {
            f.validate().unwrap();
        }
    }
}

/// Claims audit (2026-11-17): one test per claim of the module documentation, the catalog `checked_today` and the changelog entry.
#[cfg(test)]
mod claims {
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
    fn every_documented_bare_string_type_is_one_and_other_shapes_are_not() {
        for ty in [
            "String",
            "&str",
            "&String",
            "std::borrow::Cow<'_, str>",
            "impl AsRef<str>",
            "impl Into<String>",
        ] {
            let src = format!("pub fn send(email: {ty}) {{}}");
            assert_eq!(found(&src).len(), 1, "{ty}");
        }
        for ty in ["Option<&str>", "u64", "Vec<u8>", "Email", "&[u8]"] {
            let src = format!("pub fn send(email: {ty}) {{}}");
            assert!(found(&src).is_empty(), "{ty}: {:?}", found(&src));
        }
    }

    #[test]
    fn every_documented_domain_word_is_read_as_a_whole_underscore_word() {
        for word in [
            "email", "url", "uri", "hostname", "uuid", "ip", "phone", "iban", "currency", "locale",
            "timezone", "isbn", "postcode", "zipcode",
        ] {
            let src = format!("pub fn send({word}: &str, user_{word}: &str) {{}}");
            assert_eq!(found(&src).len(), 2, "{word}");
        }
        for name in ["curl", "ipv6", "emailer", "skip", "tripod"] {
            let src = format!("pub fn send({name}: &str) {{}}");
            assert!(
                found(&src).is_empty(),
                "{name} holds a domain word but is not one: {:?}",
                found(&src)
            );
        }
    }

    #[test]
    fn every_documented_entry_word_in_the_function_name_clears_it() {
        for word in [
            "parse",
            "from",
            "try",
            "new",
            "validate",
            "is",
            "check",
            "verify",
            "normalize",
            "sanitize",
            "deserialize",
            "decode",
            "read",
            "load",
            "lookup",
            "find",
            "get",
            "with",
            "set",
        ] {
            let src =
                format!("pub fn {word}_it(email: &str) {{}}\npub fn it_{word}(email: &str) {{}}");
            assert!(found(&src).is_empty(), "{word}: {:?}", found(&src));
        }
        assert_eq!(
            found("pub fn deliver(email: &str) {}").len(),
            1,
            "positive control"
        );
    }

    #[test]
    fn only_pub_functions_and_pub_inherent_methods_outside_trait_impls_are_read() {
        assert!(found("fn f(email: &str) {}").is_empty());
        assert!(found("pub(crate) fn f(email: &str) {}").is_empty());
        assert!(found("pub struct S;\nimpl S { fn f(&self, email: &str) {} }").is_empty());
        assert!(found("pub trait T { fn f(&self, email: &str); }").is_empty());
        assert_eq!(
            found("pub struct S;\nimpl S { pub fn f(&self, email: &str) {} }").len(),
            1
        );
        assert_eq!(found("pub mod m { pub fn f(email: &str) {} }").len(), 1);
    }

    #[test]
    fn a_finding_is_medium_severity_low_confidence_at_the_function() {
        let f = found("\npub fn deliver(email: &str) {}\n");
        assert_eq!(
            (f[0].severity, f[0].confidence),
            (Severity::Medium, Confidence::Low)
        );
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
    }
}
