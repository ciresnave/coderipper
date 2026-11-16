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
//! **Read as not a secret, though the name matches (the false positives of the 0.47.5 pin-move round):**
//!
//! * a count (`..._token_num`, `num`/`number` are companions) and a list of *names* of secrets (`secret_names`: `names` is a companion);
//! * a `token` written with a vocabulary or LLM qualifier directly before it (`bos_token`, `eos_token`, `pad_token`, `special_token`,
//!   `prompt_token`, `draft_token`, ..): a lexer's token, not a credential. `bearer_token`, `access_token`, `refresh_token` are still read;
//! * a collection of numbers: `Vec<u32>`, `BTreeSet<u64>`, a map whose *values* are numbers (`HashMap<String, u32>`, a vocabulary's
//!   token to id table). `Vec<u8>`, `Vec<String>` and `HashMap<String, String>` can hold a credential and are still read;
//! * the word `token` alone, in a struct whose name says vocabulary (`Vocab..`, `Tokenizer..`, `Lexer..`, `SpecialTokens..`) or in a file
//!   whose path says so (`baracuda-cuda-vocab/src/manifest.rs`). Only the word `token` is set aside there: a `password` in such a struct is
//!   still reported.
//!
//! **Not judged:** whether a field named `token` really holds a secret in any other context (a count of LLM tokens, an iterator of lexer
//! tokens that carries none of the words above), enum variants, a type alias that hides the wrapper, a `Debug` bound by another derive macro (`derive_more::Debug`), and other languages. A
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
    "public",
    "subject",
    "issuer",
    "configuration",
    "configurations",
    "supported",
    "ids",
    "names",
    "num",
    "number",
    "allow",
    "reference",
    "exclude",
];

/// Segments that, directly before `token`, say it is a lexer's or a model's token (a vocabulary entry, a count's subject), not a credential.
const TOKEN_QUALIFIERS: &[&str] = &[
    "bos",
    "eos",
    "pad",
    "unk",
    "cls",
    "sep",
    "mask",
    "sos",
    "special",
    "added",
    "prompt",
    "completion",
    "generated",
    "draft",
    "accepted",
    "emitted",
    "input",
    "output",
    "stop",
];

/// Words that, in a file path or a struct name, say the item is about a vocabulary or a lexer.
const VOCAB_WORDS: &[&str] = &["vocab", "tokeniz", "lexer", "specialtoken"];

/// Whether a path or a type name says it is about a vocabulary (`crates/baracuda-cuda-vocab/..`, `VocabEntry`, `TokenizerConfig`).
fn vocabulary_context(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    VOCAB_WORDS.iter().any(|w| lower.contains(w))
}

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
    /// For each exposed field, the identifiers of its type: a repo type among them with a redacting `Debug` clears it.
    pub field_types: Vec<(String, Vec<String>)>,
    /// Every `// sec012: public` marker on a field of the struct.
    pub markers: Vec<FieldMarker>,
}

/// A `// sec012: public - <reason>` comment on a field line or directly above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldMarker {
    pub field: String,
    pub line: u32,
    /// The reason after the dash; `None` when it is missing or empty.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A secret field that `Debug` prints.
    Leak,
    /// A flagged field carrying a marker with a reason: reported as Info, never dropped.
    Waived,
    /// A marker with no reason: a finding of its own, and it waives nothing.
    MarkerWithoutReason,
    /// A marker on a field that is not flagged.
    Stale,
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
    pub kind: Kind,
    /// The marker's reason, for a waived field.
    pub reason: String,
}

/// The marker on a field, read from the field's own line (a trailing comment) or the comment and attribute lines directly above it.
fn marker_for(lines: &[&str], field_line: u32) -> Option<Option<String>> {
    let idx = (field_line as usize).checked_sub(1)?;
    let parse = |comment: &str| -> Option<Option<String>> {
        let rest = comment.trim_start().strip_prefix("sec012: public")?;
        let reason = rest
            .trim()
            .strip_prefix('-')
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        Some(reason)
    };
    if let Some(m) = lines
        .get(idx)
        .and_then(|l| l.find("//").map(|c| &l[c + 2..]))
        .and_then(parse)
    {
        return Some(m);
    }
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines.get(i)?.trim();
        if let Some(c) = t.strip_prefix("//") {
            if let Some(m) = parse(c) {
                return Some(m);
            }
        } else if !t.starts_with("#[") {
            break;
        }
    }
    None
}

struct Scan<'a> {
    /// Whether the file's path says it is about a vocabulary.
    vocabulary_path: bool,
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
    secret_name_in(name, false)
}

/// [`secret_name`], in a file or struct that is about a vocabulary (`vocabulary`): there the word `token` alone is a lexer's token.
fn secret_name_in(name: &str, vocabulary: bool) -> bool {
    let segs = segments(name);
    if segs.iter().any(|s| COMPANIONS.contains(&s.as_str())) {
        return false;
    }
    let says_secret = segs.iter().enumerate().any(|(i, s)| {
        if !SECRET_SEGMENTS.contains(&s.as_str()) {
            return false;
        }
        if s == "token" {
            // `bos_token` is a vocabulary entry, `bearer_token` is a credential; `token` alone is a guess, set aside in a vocabulary
            let qualified = i > 0 && TOKEN_QUALIFIERS.contains(&segs[i - 1].as_str());
            return !qualified && !vocabulary;
        }
        true
    });
    if says_secret {
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

/// The single type argument of the last path segment (`Vec<T>` gives `T`), or both of a two-argument one (`HashMap<K, V>`).
fn type_args(ty: &Type) -> Option<(&syn::PathSegment, Vec<&Type>)> {
    let Type::Path(p) = ty else {
        return None;
    };
    let seg = p.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    let args = a
        .args
        .iter()
        .filter_map(|g| match g {
            syn::GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    Some((seg, args))
}

fn is_scalar_type(ty: &Type) -> bool {
    let mut words = Vec::new();
    type_words(ty, &mut words);
    words.len() == 1 && is_plain_scalar(&words)
}

fn is_u8(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident("u8"))
}

/// Whether a type is a collection of numbers: a list or set of non-byte numbers, or a map whose values are numbers (a vocabulary's token to
/// id table, a list of token ids). Bytes (`Vec<u8>`) and strings can hold a credential, so they are not numbers here.
fn numeric_collection(ty: &Type) -> bool {
    let Some((seg, args)) = type_args(ty) else {
        return false;
    };
    let name = seg.ident.to_string();
    match (name.as_str(), args.as_slice()) {
        ("Option" | "Box" | "Arc" | "Rc" | "Mutex" | "RwLock", [inner]) => {
            numeric_collection(inner)
        }
        ("Vec" | "VecDeque" | "HashSet" | "BTreeSet" | "BinaryHeap" | "IndexSet", [el]) => {
            (is_scalar_type(el) && !is_u8(el)) || numeric_collection(el)
        }
        ("HashMap" | "BTreeMap" | "IndexMap" | "FxHashMap", [_, value]) => {
            is_scalar_type(value) || numeric_collection(value)
        }
        _ => false,
    }
}

/// A field is an exposed secret when it looks like one and its type does not mask it.
fn exposed_secret(name: &str, ty: &Type, vocabulary: bool) -> bool {
    let mut words = Vec::new();
    type_words(ty, &mut words);
    if words.iter().any(|w| REDACTING.contains(&w.as_str())) {
        return false;
    }
    if words.iter().any(|w| w == "Zeroizing") {
        return true;
    }
    !is_plain_scalar(&words) && !numeric_collection(ty) && secret_name_in(name, vocabulary)
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
        let mut field_types = Vec::new();
        let mut markers = Vec::new();
        let vocabulary = self.vocabulary_path || vocabulary_context(&s.ident.to_string());
        match &s.fields {
            Fields::Named(n) => {
                for f in &n.named {
                    if let Some(id) = &f.ident {
                        let line = line_of(id.span());
                        if let Some(reason) = marker_for(&self.lines, line) {
                            markers.push(FieldMarker {
                                field: id.to_string(),
                                line,
                                reason,
                            });
                        }
                        if exposed_secret(&id.to_string(), &f.ty, vocabulary) {
                            exposed.push(id.to_string());
                            let mut words = Vec::new();
                            type_words(&f.ty, &mut words);
                            field_types.push((id.to_string(), words));
                        }
                    }
                }
            }
            Fields::Unnamed(u) => {
                for (i, f) in u.unnamed.iter().enumerate() {
                    // a tuple field has no name: only a `Zeroizing` type says it is a secret
                    if exposed_secret("", &f.ty, vocabulary) {
                        exposed.push(i.to_string());
                        let mut words = Vec::new();
                        type_words(&f.ty, &mut words);
                        field_types.push((i.to_string(), words));
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
            field_types,
            markers,
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

fn scan(path: &str, text: &str) -> Option<(Vec<StructDecl>, Vec<DebugImpl>)> {
    let parsed = syn::parse_file(text).ok()?;
    let mut s = Scan {
        vocabulary_path: vocabulary_context(path),
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
        let Some((ss, is)) = scan(path, text) else {
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
    // A repo type whose every declaration hand-writes a masking `Debug` (one impl per declaration, none without a sign of masking): a
    // field of it prints a mask. A name declared twice is trusted only when all its impls mask; one derive or one leaking impl spoils it.
    let redacting: std::collections::HashSet<&str> = structs
        .iter()
        .filter_map(|(name, list)| {
            if list.iter().any(|(_, s)| s.derives_debug) {
                return None;
            }
            let mine: Vec<&DebugImpl> = impls.iter().filter(|i| &i.type_name == name).collect();
            (mine.len() >= list.len()
                && mine
                    .iter()
                    .all(|i| REDACTION_MARKS.iter().any(|m| i.body.contains(m))))
            .then_some(name.as_str())
        })
        .collect();
    let mut out = Vec::new();
    for (name, list) in &structs {
        let [(path, s)] = list.as_slice() else {
            continue;
        };
        let exposed: Vec<String> = s
            .exposed
            .iter()
            .filter(|f| {
                !s.field_types.iter().any(|(n, words)| {
                    &n == f
                        && words
                            .iter()
                            .any(|w| w != name && redacting.contains(w.as_str()))
                })
            })
            .cloned()
            .collect();
        let (how, mut fields): (&'static str, Vec<String>) = if s.derives_debug {
            ("derived", exposed.clone())
        } else {
            let leaked: Vec<String> = impls
                .iter()
                .filter(|i| &i.type_name == name)
                .flat_map(|i| {
                    exposed
                        .iter()
                        .filter(|f| leaks(&i.body, f))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .collect();
            ("hand-written", leaked)
        };
        for m in &s.markers {
            let flagged = fields.contains(&m.field);
            let (kind, reason) = match (&m.reason, flagged) {
                (Some(r), true) => (Kind::Waived, r.clone()),
                (None, true) => (Kind::MarkerWithoutReason, String::new()),
                (_, false) => (Kind::Stale, m.reason.clone().unwrap_or_default()),
            };
            if kind == Kind::Waived {
                fields.retain(|f| f != &m.field);
            }
            out.push(Hit {
                path: path.clone(),
                line: m.line,
                name: name.clone(),
                fields: vec![m.field.clone()],
                how,
                kind,
                reason,
            });
        }
        if !fields.is_empty() {
            out.push(Hit {
                path: path.clone(),
                line: s.line,
                name: name.clone(),
                fields,
                how,
                kind: Kind::Leak,
                reason: String::new(),
            });
        }
    }
    out.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    (out, unparsed)
}

/// The explanation of a leaking struct.
fn leak_detail(h: &Hit, list: &str) -> String {
    format!(
        "`{}` in `{}` has the field(s) {list}, and its {} `Debug` prints them. Debug output reaches logs, panic messages and traces, so          the secret goes wherever the value is formatted. Wrap the field in a type whose `Debug` prints a mask (`secrecy::Secret`, a          redacting newtype), or write `Debug` by hand and print a placeholder for it. A field that is public by design takes a comment          `// sec012: public - <reason>` (the finding then stays as an Info). Read from the syntax only: that a field named          `token` holds a secret is a guess from its name, and a type alias that hides a masking wrapper is not seen.",
        h.name, h.path, h.how
    )
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
            let (severity, summary, detail) = match h.kind {
                Kind::Leak => (
                    Severity::Medium,
                    format!(
                        "`{}` prints its secret field(s) {list} through a {} `Debug`",
                        h.name, h.how
                    ),
                    leak_detail(h, &list),
                ),
                Kind::Waived => (
                    Severity::Info,
                    format!("`{}` field {list} waived: {}", h.name, h.reason),
                    format!(
                        "`{}` in `{}` marks {list} `// sec012: public - {}`, so SEC-012 does not report it as a secret. The waiver is kept in the report, \
                         not dropped: review the reason, and remove the marker if the field is ever not public.",
                        h.name, h.path, h.reason
                    ),
                ),
                Kind::MarkerWithoutReason => (
                    Severity::Medium,
                    format!("`{}` field {list} has a `sec012: public` marker with no reason", h.name),
                    format!(
                        "`{}` in `{}` marks {list} `// sec012: public` without a reason, which waives nothing. Write `// sec012: public - <why this is not a secret>`.",
                        h.name, h.path
                    ),
                ),
                Kind::Stale => (
                    Severity::Info,
                    format!("`{}` field {list} has a stale `sec012: public` marker", h.name),
                    format!(
                        "`{}` in `{}` marks {list} `// sec012: public`, but SEC-012 does not flag that field (it is not named like a secret, is masked, or `Debug` does not print it). Remove the marker.",
                        h.name, h.path
                    ),
                ),
            };
            Finding::new(id, severity, Confidence::Medium, view.name(), summary, detail)
            .location(Location::new(h.path.as_str(), Some(h.line)))
            .subject(match h.kind {
                Kind::Leak => format!("{}:{}", h.path, h.name),
                _ => format!("{}:{}:{}", h.path, h.name, h.line),
            })
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

    fn one_at(path: &str, src: &str) -> Vec<Hit> {
        analyse(&[(path.to_string(), src.to_string())]).0
    }

    // ---- the false positives from the 0.47.5 pin-move round (baracuda, mlmf, WebHostAnt) ----

    #[test]
    fn a_list_of_secret_names_is_not_a_secret() {
        // WebHostAnt `Observed.secret_names` and `ReadBack.secret_names`: the names of secrets, never their values
        let src = "#[derive(Debug)]\nstruct Observed { secret_names: BTreeSet<String> }\n#[derive(Debug)]\nstruct ReadBack { secret_names: Vec<String> }";
        assert!(one(src).is_empty());
    }

    #[test]
    fn a_count_or_tensor_of_llm_tokens_is_not_a_secret() {
        // baracuda `SpeculativeSamplingArgs`: output_accepted_token_num, output_emitted_draft_token_num
        let src = "#[derive(Debug)]\nstruct SpeculativeSamplingArgs<'a> { output_accepted_token_num: TensorMut<'a, i32>, output_emitted_draft_token_num: TensorMut<'a, i32>, max_tokens: u32 }";
        assert!(one(src).is_empty());
    }

    #[test]
    fn special_tokens_and_vocabulary_entries_are_not_secrets() {
        // mlmf `SpecialTokensMap`, `TokenizerConfig`, `VocabEntry`
        let special = "#[derive(Debug)]\nstruct SpecialTokensMap { bos_token: Option<String>, eos_token: Option<String>, unk_token: Option<String>, pad_token: Option<String>, sep_token: Option<String>, cls_token: Option<String>, mask_token: Option<String> }";
        assert!(one(special).is_empty());
        let config = "#[derive(Debug)]\nstruct TokenizerConfig { bos_token: Option<String>, eos_token: Option<String> }";
        assert!(one(config).is_empty());
        let entry = "#[derive(Debug)]\nstruct VocabEntry { token: String, id: u32 }";
        assert!(one(entry).is_empty());
    }

    #[test]
    fn a_vocabulary_map_or_a_list_of_token_ids_is_not_a_secret() {
        let map = "#[derive(Debug)]\nstruct V { token: HashMap<String, u32> }\n#[derive(Debug)]\nstruct W { token: BTreeMap<String, usize> }";
        assert!(one(map).is_empty());
        let ids = "#[derive(Debug)]\nstruct V { token: Vec<u32> }\n#[derive(Debug)]\nstruct W { token: BTreeSet<u64> }";
        assert!(one(ids).is_empty());
    }

    #[test]
    fn a_token_in_a_vocabulary_crate_is_a_vocabulary_token() {
        // baracuda `baracuda-cuda-vocab` `Member.token`: the canonical `cuda:` token of a codec, a String
        let src = "#[derive(Debug)]\npub struct Member { pub token: String, pub class: String }";
        assert!(one_at("crates/baracuda-cuda-vocab/src/manifest.rs", src).is_empty());
        // ... but only the word `token` is neutralised there, never a password
        let both =
            "#[derive(Debug)]\npub struct Member { pub token: String, pub password: String }";
        let hits = one_at("crates/baracuda-cuda-vocab/src/manifest.rs", both);
        assert_eq!(hits[0].fields, vec!["password"]);
    }

    // ---- the true positives that must keep firing ----

    #[test]
    fn a_string_token_or_key_still_reports_outside_a_vocabulary_context() {
        // mlmf `HubRangedFile.token`, surfaceant `Config.token`, synapse `KeyPair.private_key`
        let hub = "#[derive(Debug, Clone)]\npub struct HubRangedFile { pub repo: String, pub token: Option<String> }";
        assert_eq!(
            one_at("crates/mlmf-source-hub/src/ranged.rs", hub)[0].fields,
            vec!["token"]
        );
        let config =
            "#[derive(Debug)]\npub struct Config { pub base_url: String, pub token: String }";
        assert_eq!(
            one_at("crates/surfaceant-client/src/lib.rs", config)[0].fields,
            vec!["token"]
        );
        let pair = "#[derive(Debug)]\npub struct KeyPair { pub public_key: Vec<u8>, pub private_key: Vec<u8> }";
        assert_eq!(one(pair)[0].fields, vec!["private_key"]);
    }

    #[test]
    fn access_and_refresh_tokens_and_lists_of_strings_still_report() {
        let src =
            "#[derive(Debug)]\nstruct S { access_token: Option<String>, refresh_token: String }";
        assert_eq!(one(src)[0].fields, vec!["access_token", "refresh_token"]);
        // a list or a map of strings can still hold credentials; only a list or map of NUMBERS is read as ids and counters
        let list = "#[derive(Debug)]\nstruct L { token: Vec<String> }\n#[derive(Debug)]\nstruct M { token: HashMap<String, String> }\n#[derive(Debug)]\nstruct B { token: Vec<u8> }";
        assert_eq!(one(list).len(), 3);
    }

    #[test]
    fn a_vocabulary_word_does_not_hide_a_secret_named_for_what_it_is() {
        // `bos_token` is a special token; `bearer_token` and `refresh_token` are credentials
        let src = "#[derive(Debug)]\nstruct S { bos_token: String, bearer_token: String, refresh_token: String }";
        assert_eq!(one(src)[0].fields, vec!["bearer_token", "refresh_token"]);
    }

    const REDACTING_TYPE: &str = "pub struct Inner { pub value: String }\nimpl fmt::Debug for Inner {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        f.write_str(\"Inner([REDACTED])\")\n    }\n}\n";

    #[test]
    fn a_field_of_a_repo_type_with_a_redacting_hand_written_debug_is_not_reported() {
        let src = format!("{REDACTING_TYPE}#[derive(Debug)]\nstruct Outer {{ token: Inner, access_token: Option<Inner>, tokens: Vec<Inner> }}");
        assert!(one(&src).is_empty());
    }

    #[test]
    fn a_nested_type_that_derives_debug_or_does_not_redact_still_leaks() {
        let derived = "#[derive(Debug)]\npub struct Inner { pub value: String }\n#[derive(Debug)]\nstruct Outer { token: Inner }";
        assert_eq!(one(derived)[0].name, "Outer");
        let plain = "pub struct Inner { pub value: String }\nimpl fmt::Debug for Inner {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        write!(f, \"{}\", self.value)\n    }\n}\n#[derive(Debug)]\nstruct Outer { token: Inner }";
        assert_eq!(one(plain)[0].name, "Outer");
        let unknown = "#[derive(Debug)]\nstruct Outer { token: Elsewhere }";
        assert_eq!(one(unknown)[0].name, "Outer");
    }

    #[test]
    fn a_nested_type_declared_twice_is_trusted_only_when_every_declaration_masks() {
        let src = format!("{REDACTING_TYPE}mod m {{ #[derive(Debug)] pub struct Inner {{ pub value: String }} }}\n#[derive(Debug)]\nstruct Outer {{ token: Inner }}");
        assert_eq!(one(&src)[0].name, "Outer");
        let both = format!(
            "{REDACTING_TYPE}mod m {{
{REDACTING_TYPE}}}
#[derive(Debug)]
struct Outer {{ token: Inner }}"
        );
        assert!(one(&both).is_empty());
    }

    #[test]
    fn a_marker_with_a_reason_waives_the_field_as_an_info_and_never_drops_it() {
        let src = "#[derive(Debug)]\nstruct Acme {\n    // sec012: public - ACME challenge tokens are served in the clear by design\n    token: String,\n    password: String,\n}";
        let hits = one(src);
        let leak = hits.iter().find(|h| h.kind == Kind::Leak).unwrap();
        assert_eq!(leak.fields, vec!["password"]);
        let waived = hits.iter().find(|h| h.kind == Kind::Waived).unwrap();
        assert_eq!(waived.fields, vec!["token"]);
        assert_eq!(
            waived.reason,
            "ACME challenge tokens are served in the clear by design"
        );
        assert_eq!(waived.line, 4);
    }

    #[test]
    fn a_trailing_marker_on_the_field_line_counts_too() {
        let src = "#[derive(Debug)]\nstruct A {\n    token: String, // sec012: public - published in the DNS record\n}";
        let hits = one(src);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, Kind::Waived);
    }

    #[test]
    fn a_marker_without_a_reason_is_a_finding_and_waives_nothing() {
        for marker in [
            "// sec012: public",
            "// sec012: public -",
            "// sec012: public -   ",
        ] {
            let src =
                format!("#[derive(Debug)]\nstruct A {{\n    {marker}\n    token: String,\n}}");
            let hits = one(&src);
            assert!(hits.iter().any(|h| h.kind == Kind::Leak), "{marker}");
            assert!(
                hits.iter().any(|h| h.kind == Kind::MarkerWithoutReason),
                "{marker}"
            );
            assert!(!hits.iter().any(|h| h.kind == Kind::Waived), "{marker}");
        }
    }

    #[test]
    fn a_marker_on_a_field_that_is_not_flagged_is_a_stale_info() {
        let src = "#[derive(Debug)]\nstruct A {\n    // sec012: public - was a token once\n    count: u32,\n}";
        let hits = one(src);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, Kind::Stale);
        assert_eq!(hits[0].fields, vec!["count"]);
    }

    #[test]
    fn a_marker_two_lines_above_or_in_prose_is_not_a_marker() {
        let src = "#[derive(Debug)]\nstruct A {\n    // sec012: public - reason\n    other: u32,\n    token: String,\n}";
        let hits = one(src);
        assert!(hits
            .iter()
            .any(|h| h.kind == Kind::Leak && h.fields == vec!["token"]));
        let prose = "#[derive(Debug)]\nstruct A {\n    // we do not use sec012: public anywhere\n    token: String,\n}";
        assert_eq!(one(prose).len(), 1);
        assert_eq!(one(prose)[0].kind, Kind::Leak);
    }
}
