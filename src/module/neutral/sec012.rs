// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-012: no `Debug` on a type that holds a secret (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories is parsed with `syn`; items under
//! `#[cfg(test)]` and functions marked `#[test]` are skipped. The repository is read in two passes: each `struct`, with the fields that
//! look like secrets, and each `impl Debug for Type`. A struct is reported once, at its declaration, when it has a secret field that is
//! printed by `Debug`, either because the struct derives `Debug`, or because a hand-written `impl Debug` for it reads that field and
//! carries no sign of redaction.
//!
//! **A secret field** is one whose name says so, or whose type is `Zeroizing`:
//!
//! * the name, read as `snake_case` segments, has a segment `token`, `secret`, `password`, `passwd`, `passphrase`, `credential`,
//!   `credentials`, `authorization`, `bearer` or `apikey`, or `api` followed by `key`, or `key` with a secret qualifier directly before it
//!   (`private_key`, `secret_key`, `signing_key`, `encryption_key`, `master_key`, `access_key`, `session_key`, `auth_key`, `hmac_key`,
//!   `shared_key`, `symmetric_key`). A bare `key` is not read (cache keys and map keys would drown the rule);
//! * unless the name also has a companion segment that says it is about the secret and not the secret itself (`token_type`,
//!   `password_hash`, `key_id`, `token_url`, `secret_name`, `token_ttl`, and so on), or the field's type is a `bool`, a number or a
//!   `Duration`;
//! * a field of type `Zeroizing<..>` is a secret whatever it is called: it wipes memory on drop but its `Debug` prints the contents.
//!
//! **Redacted by construction, so not reported:** a field whose type is `Secret`, `SecretString`, `SecretBox`, `SecretVec`, `SecretSlice`
//! or `SecretValue` (these print a mask), and a hand-written `Debug` impl that never reads the field, or that carries `redact`,
//! `REDACTED`, `<hidden>`, `***` or `finish_non_exhaustive` in its body.
//!
//! **Not judged:** whether a field named `token` really holds a secret (a count of LLM tokens, an iterator of lexer tokens), enum
//! variants, a type alias that hides the wrapper, a `Debug` bound by another derive macro (`derive_more::Debug`), and other languages. A
//! struct name declared twice in the repository is skipped (the impl cannot be matched to one of them without name resolution). A file
//! `syn` cannot parse is counted in a note, never read as clean.

use super::efx003::is_dev_path;
use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashMap;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Attribute, Fields, ItemImpl, ItemMod, ItemStruct, Path, Token, Type};

/// Name segments that say the field is a secret.
const SECRET_SEGMENTS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "passphrase",
    "credential",
    "credentials",
    "authorization",
    "bearer",
    "apikey",
];

/// Segments that, directly before `key`, say it is a secret key.
const KEY_QUALIFIERS: &[&str] = &[
    "private",
    "secret",
    "signing",
    "encryption",
    "master",
    "access",
    "session",
    "auth",
    "hmac",
    "shared",
    "symmetric",
    "api",
];

/// Segments that say the field is *about* a secret (its kind, location, lifetime, digest), not the secret itself.
const COMPANIONS: &[&str] = &[
    "type",
    "url",
    "uri",
    "endpoint",
    "count",
    "ttl",
    "expiry",
    "expires",
    "expiration",
    "len",
    "length",
    "limit",
    "max",
    "min",
    "id",
    "kind",
    "name",
    "path",
    "header",
    "hash",
    "version",
    "policy",
    "required",
    "enabled",
    "prefix",
    "algorithm",
    "alg",
    "format",
    "source",
    "store",
    "manager",
    "provider",
    "config",
    "file",
    "env",
    "var",
    "lifetime",
    "secs",
    "seconds",
    "millis",
    "details",
    "definition",
    "definitions",
    "types",
    "complexity",
    "revocation",
    "offered",
    "allowed",
    "require",
    "requires",
    "method",
    "methods",
    "scheme",
    "schemes",
    "aud",
    "iss",
    "jti",
    "info",
    "reference",
    "exclude",
];

/// Types whose `Debug` prints a mask, so a field of one is redacted by construction.
const REDACTING: &[&str] = &[
    "Secret",
    "SecretString",
    "SecretBox",
    "SecretVec",
    "SecretSlice",
    "SecretValue",
];

/// Signs, in the body of a hand-written `Debug`, that the output is masked.
const REDACTION_MARKS: &[&str] = &[
    "redact",
    "REDACTED",
    "<hidden>",
    "***",
    "finish_non_exhaustive",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StructDecl {
    pub name: String,
    pub line: u32,
    pub derives_debug: bool,
    /// The secret fields that are not redacted by their type.
    pub exposed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DebugImpl {
    pub type_name: String,
    /// Whether the body reads one of the struct's exposed fields without any sign of redaction. Filled once the struct is known.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hit {
    pub path: String,
    pub line: u32,
    pub name: String,
    pub fields: Vec<String>,
    /// `derived` or `hand-written`.
    pub how: &'static str,
}

struct Scan<'a> {
    lines: Vec<&'a str>,
    structs: Vec<StructDecl>,
    impls: Vec<DebugImpl>,
}

fn segments(name: &str) -> Vec<String> {
    name.to_ascii_lowercase()
        .split('_')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether a field name says it holds a secret.
pub(super) fn secret_name(name: &str) -> bool {
    let segs = segments(name);
    if segs.iter().any(|s| COMPANIONS.contains(&s.as_str())) {
        return false;
    }
    if segs.iter().any(|s| SECRET_SEGMENTS.contains(&s.as_str())) {
        return true;
    }
    segs.windows(2)
        .any(|w| w[1] == "key" && KEY_QUALIFIERS.contains(&w[0].as_str()))
}

/// Every identifier in a type's path segments and generic arguments.
fn type_words(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Path(p) => {
            for seg in &p.path.segments {
                out.push(seg.ident.to_string());
                if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                    for arg in &a.args {
                        if let syn::GenericArgument::Type(t) = arg {
                            type_words(t, out);
                        }
                    }
                }
            }
        }
        Type::Reference(r) => type_words(&r.elem, out),
        Type::Paren(p) => type_words(&p.elem, out),
        Type::Group(g) => type_words(&g.elem, out),
        Type::Slice(s) => type_words(&s.elem, out),
        Type::Array(a) => type_words(&a.elem, out),
        _ => {}
    }
}

fn is_plain_scalar(words: &[String]) -> bool {
    // `Option<bool>` is as scalar as `bool`
    let words = match words {
        [first, rest @ ..] if first == "Option" => rest,
        all => all,
    };
    words.first().is_some_and(|w| {
        matches!(
            w.as_str(),
            "bool"
                | "u8"
                | "u16"
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
                | "Duration"
        )
    })
}

/// A field is an exposed secret when it looks like one and its type does not mask it.
fn exposed_secret(name: &str, ty: &Type) -> bool {
    let mut words = Vec::new();
    type_words(ty, &mut words);
    if words.iter().any(|w| REDACTING.contains(&w.as_str())) {
        return false;
    }
    if words.iter().any(|w| w == "Zeroizing") {
        return true;
    }
    !is_plain_scalar(&words) && secret_name(name)
}

fn derives_debug(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("derive")
            && a.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
                .is_ok_and(|paths| {
                    paths
                        .iter()
                        .any(|p| p.segments.last().is_some_and(|s| s.ident == "Debug"))
                })
    })
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !f.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_struct(&mut self, s: &'ast ItemStruct) {
        if s.attrs.iter().any(is_test_attr) {
            return;
        }
        let mut exposed = Vec::new();
        match &s.fields {
            Fields::Named(n) => {
                for f in &n.named {
                    if let Some(id) = &f.ident {
                        if exposed_secret(&id.to_string(), &f.ty) {
                            exposed.push(id.to_string());
                        }
                    }
                }
            }
            Fields::Unnamed(u) => {
                for (i, f) in u.unnamed.iter().enumerate() {
                    // a tuple field has no name: only a `Zeroizing` type says it is a secret
                    if exposed_secret("", &f.ty) {
                        exposed.push(i.to_string());
                    }
                }
            }
            Fields::Unit => {}
        }
        self.structs.push(StructDecl {
            name: s.ident.to_string(),
            line: line_of(s.ident.span()),
            derives_debug: derives_debug(&s.attrs),
            exposed,
        });
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        if i.attrs.iter().any(is_test_attr) {
            return;
        }
        let is_debug = i
            .trait_
            .as_ref()
            .and_then(|(p, _)| p.segments.last())
            .is_some_and(|s| s.ident == "Debug");
        if is_debug {
            if let Type::Path(p) = &*i.self_ty {
                if let Some(seg) = p.path.segments.last() {
                    let start = line_of(syn::spanned::Spanned::span(i));
                    let end =
                        u32::try_from(syn::spanned::Spanned::span(i).end().line).unwrap_or(start);
                    let body = (start..=end)
                        .filter_map(|l| self.lines.get((l as usize).wrapping_sub(1)).copied())
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.impls.push(DebugImpl {
                        type_name: seg.ident.to_string(),
                        body,
                    });
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

fn scan(text: &str) -> Option<(Vec<StructDecl>, Vec<DebugImpl>)> {
    let parsed = syn::parse_file(text).ok()?;
    let mut s = Scan {
        lines: text.lines().collect(),
        structs: Vec::new(),
        impls: Vec::new(),
    };
    s.visit_file(&parsed);
    Some((s.structs, s.impls))
}

/// Whether a hand-written `Debug` body reads the field (`self.name`) and shows no sign of redaction.
fn leaks(body: &str, field: &str) -> bool {
    if REDACTION_MARKS.iter().any(|m| body.contains(m)) {
        return false;
    }
    body.contains(&format!("self.{field}"))
}

/// The structs of a repository that print a secret field through `Debug`, over `(path, text)` pairs. The second value is the paths
/// that did not parse.
pub(super) fn analyse(files: &[(String, String)]) -> (Vec<Hit>, Vec<String>) {
    let mut structs: HashMap<String, Vec<(String, StructDecl)>> = HashMap::new();
    let mut impls: Vec<DebugImpl> = Vec::new();
    let mut unparsed = Vec::new();
    for (path, text) in files {
        let Some((ss, is)) = scan(text) else {
            unparsed.push(path.clone());
            continue;
        };
        for s in ss {
            structs
                .entry(s.name.clone())
                .or_default()
                .push((path.clone(), s));
        }
        impls.extend(is);
    }
    let mut out = Vec::new();
    for (name, list) in &structs {
        let [(path, s)] = list.as_slice() else {
            continue;
        };
        if s.exposed.is_empty() {
            continue;
        }
        let (how, fields): (&'static str, Vec<String>) = if s.derives_debug {
            ("derived", s.exposed.clone())
        } else {
            let leaked: Vec<String> = impls
                .iter()
                .filter(|i| &i.type_name == name)
                .flat_map(|i| {
                    s.exposed
                        .iter()
                        .filter(|f| leaks(&i.body, f))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .collect();
            if leaked.is_empty() {
                continue;
            }
            ("hand-written", leaked)
        };
        out.push(Hit {
            path: path.clone(),
            line: s.line,
            name: name.clone(),
            fields,
            how,
        });
    }
    out.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    (out, unparsed)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let id = crate::rule_id::SEC_012;
    let paths: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p) && !is_dev_path(p))
        .collect();
    if paths.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source file outside tests".into(),
        );
    }
    let mut files: Vec<(String, String)> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for p in &paths {
        match view.read(p) {
            Some(t) => files.push(((*p).clone(), t)),
            None => unread.push((*p).clone()),
        }
    }
    let (hits, mut unparsed) = analyse(&files);
    unparsed.extend(unread);
    let read = files.len().saturating_sub(unparsed.len());
    let findings: Vec<Finding> = hits
        .iter()
        .map(|h| {
            let list = h
                .fields
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Finding::new(
                id,
                Severity::Medium,
                Confidence::Medium,
                view.name(),
                format!(
                    "`{}` prints its secret field(s) {list} through a {} `Debug`",
                    h.name, h.how
                ),
                format!(
                    "`{}` in `{}` has the field(s) {list}, and its {} `Debug` prints them. Debug output reaches logs, panic messages and traces, so \
                     the secret goes wherever the value is formatted. Wrap the field in a type whose `Debug` prints a mask (`secrecy::Secret`, a \
                     redacting newtype), or write `Debug` by hand and print a placeholder for it. Read from the syntax only: that a field named \
                     `token` holds a secret is a guess from its name, and a type alias that hides a masking wrapper is not seen.",
                    h.name, h.path, h.how
                ),
            )
            .location(Location::new(h.path.as_str(), Some(h.line)))
            .subject(format!("{}:{}", h.path, h.name))
            .positive_control(format!(
                "{read} Rust file(s) parsed with syn; the struct was read at {}:{}",
                h.path, h.line
            ))
        })
        .collect();
    let mut notes = vec![format!("{id}: {read} Rust file(s) read with syn")];
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

    fn one(src: &str) -> Vec<Hit> {
        analyse(&[("lib.rs".to_string(), src.to_string())]).0
    }

    #[test]
    fn a_derived_debug_on_a_password_or_token_field_is_reported() {
        let hits = one("#[derive(Debug, Clone)]\nstruct Login { user: String, password: String, api_key: String }");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].how, "derived");
        assert_eq!(hits[0].fields, vec!["password", "api_key"]);
    }

    #[test]
    fn a_masking_wrapper_type_is_redacted_by_construction() {
        let src = "#[derive(Debug)]\nstruct A { token: Secret<String>, password: secrecy::SecretString, key: Vec<u8> }";
        assert!(one(src).is_empty());
    }

    #[test]
    fn zeroizing_is_a_secret_whatever_it_is_called() {
        let hits = one("#[derive(Debug)]\nstruct K { material: Zeroizing<Vec<u8>> }");
        assert_eq!(hits[0].fields, vec!["material"]);
        assert!(
            one("#[derive(Debug)]\nstruct K { material: Secret<Zeroizing<Vec<u8>>> }").is_empty()
        );
    }

    #[test]
    fn a_companion_word_or_a_scalar_type_says_it_is_not_the_secret() {
        let src = "#[derive(Debug)]\nstruct C { token_type: String, password_hash: String, key_id: u32, token_ttl: Duration, has_token: bool, max_tokens: usize, token_url: String, cache_key: String, key: String }";
        assert!(one(src).is_empty());
    }

    #[test]
    fn a_key_needs_a_secret_qualifier() {
        assert!(secret_name("private_key"));
        assert!(secret_name("client_secret"));
        assert!(secret_name("apikey"));
        assert!(!secret_name("key"));
        assert!(!secret_name("primary_key"));
    }

    #[test]
    fn a_hand_written_debug_that_prints_the_field_is_reported_and_a_masked_one_is_not() {
        let leaky = "struct A { token: String }\nimpl fmt::Debug for A {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        write!(f, \"A({})\", self.token)\n    }\n}";
        let hits = one(leaky);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].how, "hand-written");
        let masked = "struct A { token: String }\nimpl Debug for A {\n    fn fmt(&self, f: &mut Formatter) -> Result {\n        f.debug_struct(\"A\").field(\"token\", &\"[REDACTED]\").finish()\n    }\n}";
        assert!(one(masked).is_empty());
        let silent = "struct A { token: String, n: u8 }\nimpl Debug for A {\n    fn fmt(&self, f: &mut Formatter) -> Result {\n        write!(f, \"A {}\", self.n)\n    }\n}";
        assert!(one(silent).is_empty());
    }

    #[test]
    fn the_impl_may_live_in_another_file_and_test_code_is_skipped() {
        let files = [
            ("a.rs".to_string(), "struct A { password: String }".to_string()),
            (
                "b.rs".to_string(),
                "impl Debug for A { fn fmt(&self, f: &mut F) -> R { write!(f, \"{}\", self.password) } }".to_string(),
            ),
        ];
        assert_eq!(analyse(&files).0.len(), 1);
        assert!(
            one("#[cfg(test)]\nmod t { #[derive(Debug)] struct A { password: String } }")
                .is_empty()
        );
    }

    #[test]
    fn a_name_declared_twice_is_skipped_and_an_unparsable_file_is_listed() {
        let twice = "#[derive(Debug)]\nstruct A { password: String }\nmod m { #[derive(Debug)] struct A { password: String } }";
        assert!(one(twice).is_empty());
        let (hits, unparsed) = analyse(&[("x.rs".to_string(), "struct (((".to_string())]);
        assert!(hits.is_empty());
        assert_eq!(unparsed, vec!["x.rs".to_string()]);
    }
}
