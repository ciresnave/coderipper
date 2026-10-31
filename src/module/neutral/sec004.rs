// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-004 (first part): a Rust key built from a literal, and TLS verification switched off.
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories (`tests/`, `benches/`,
//! `examples/`, `build.rs`, `tests.rs`-style files) is parsed with `syn`, and items under `#[cfg(test)]` or `#[test]` are skipped.
//! Two checks, both on API names, with no types:
//!
//! * **4a, hard-coded key material (High).** A call to `from_secret`, `new_from_slice`, `Key::from_slice`, `hmac::Key::new` or
//!   `aead::UnboundKey::new` whose key argument is a non-empty string or byte literal (`b".."`, `"..".as_bytes()`, `&b".."[..]`, an
//!   array of integer literals), or a local bound to one earlier in the same function.
//! * **4b, TLS verification off (High).** `danger_accept_invalid_certs(x)` or `danger_accept_invalid_hostnames(x)` unless `x` is the
//!   literal `false`, and rustls' `.dangerous()` with no arguments. A pass-through of a configuration value (`x` not a literal) is
//!   a finding of lower confidence that says to check the default.
//!
//! **What it cannot see:** a key built from `concat!`, a `const` or `static`, a helper, or an environment variable (the last is
//! right); a key literal in a macro's token stream; a certificate check disabled through a custom verifier; other languages. A
//! deliberate fixture key in non-test code is what an `[[allow]]` entry on the site is for. A file `syn` cannot parse is counted in
//! a note, never read as clean.
//!
//! Weak algorithms (4c) and the lettered checks that need types (4d, 4e) are not judged here.

use super::efx003::is_dev_path;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, Expr, ExprCall, ExprMethodCall, ImplItemFn, ItemFn, ItemMod, Lit, Pat};

const ID: &str = crate::rule_id::SEC_004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Key,
    Tls,
}

#[derive(Debug)]
struct Site {
    part: Part,
    line: u32,
    what: String,
    /// A configuration pass-through rather than a literal.
    soft: bool,
}

#[derive(Default)]
struct Scan {
    /// Names bound to a literal key in the function being read (one set per nested function).
    locals: Vec<HashSet<String>>,
    out: Vec<Site>,
}

fn is_test_attr(attr: &Attribute) -> bool {
    if attr.path().is_ident("test") {
        return true;
    }
    if attr.path().is_ident("cfg") {
        return attr
            .parse_args::<syn::Meta>()
            .is_ok_and(|m| m.path().is_ident("test"));
    }
    false
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

/// Whether `e` is a non-empty string or byte literal, possibly borrowed, converted to bytes or sliced.
fn literal_key(e: &Expr) -> bool {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) => !s.value().is_empty(),
            Lit::ByteStr(b) => !b.value().is_empty(),
            _ => false,
        },
        Expr::Reference(r) => literal_key(&r.expr),
        Expr::Paren(p) => literal_key(&p.expr),
        Expr::Index(i) => literal_key(&i.expr),
        Expr::MethodCall(m) => {
            m.args.is_empty()
                && matches!(
                    m.method.to_string().as_str(),
                    "as_bytes" | "to_vec" | "into" | "as_ref" | "to_owned" | "to_string"
                )
                && literal_key(&m.receiver)
        }
        Expr::Array(a) => {
            !a.elems.is_empty()
                && a.elems.iter().all(
                    |x| matches!(x, Expr::Lit(l) if matches!(l.lit, Lit::Int(_) | Lit::Byte(_))),
                )
        }
        _ => false,
    }
}

impl Scan {
    fn is_key(&self, e: &Expr) -> bool {
        match e {
            Expr::Path(p) => p.path.get_ident().is_some_and(|id| {
                self.locals
                    .last()
                    .is_some_and(|set| set.contains(&id.to_string()))
            }),
            Expr::Reference(r) => self.is_key(&r.expr),
            Expr::Paren(p) => self.is_key(&p.expr),
            Expr::Index(i) => self.is_key(&i.expr),
            Expr::MethodCall(m) => {
                m.args.is_empty()
                    && matches!(
                        m.method.to_string().as_str(),
                        "as_bytes" | "to_vec" | "into" | "as_ref" | "to_owned"
                    )
                    && self.is_key(&m.receiver)
            }
            _ => literal_key(e),
        }
    }

    fn function<F: FnOnce(&mut Self)>(&mut self, attrs: &[Attribute], walk: F) {
        if attrs.iter().any(is_test_attr) {
            return;
        }
        self.locals.push(HashSet::new());
        walk(self);
        self.locals.pop();
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !m.attrs.iter().any(is_test_attr) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        self.function(&f.attrs, |s| syn::visit::visit_item_fn(s, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        self.function(&f.attrs, |s| syn::visit::visit_impl_item_fn(s, f));
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let (Pat::Ident(id), Some(init)) = (&l.pat, &l.init) {
            if literal_key(&init.expr) {
                if let Some(set) = self.locals.last_mut() {
                    set.insert(id.ident.to_string());
                }
            }
        }
        syn::visit::visit_local(self, l);
    }

    fn visit_expr_call(&mut self, c: &'ast ExprCall) {
        if let Expr::Path(p) = &*c.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if let Some((last, rest)) = segs.split_last() {
                let qualifier = rest.last().map(String::as_str);
                let key_arg = match (last.as_str(), qualifier) {
                    ("from_secret" | "new_from_slice", _) | ("from_slice", Some("Key")) => 0,
                    ("new", Some("Key")) if rest.iter().any(|s| s == "hmac") => 1,
                    ("new", Some("UnboundKey")) => 1,
                    _ => usize::MAX,
                };
                if let Some(arg) = c.args.iter().nth(key_arg) {
                    if self.is_key(arg) {
                        self.out.push(Site {
                            part: Part::Key,
                            line: line_of(c.span()),
                            what: segs.join("::"),
                            soft: false,
                        });
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast ExprMethodCall) {
        let name = m.method.to_string();
        let line = line_of(m.method.span());
        match name.as_str() {
            "danger_accept_invalid_certs" | "danger_accept_invalid_hostnames" => {
                let arg = m.args.first();
                let is_false = arg.is_some_and(
                    |a| matches!(a, Expr::Lit(l) if matches!(&l.lit, Lit::Bool(b) if !b.value)),
                );
                if !is_false {
                    let soft = !arg.is_some_and(
                        |a| matches!(a, Expr::Lit(l) if matches!(&l.lit, Lit::Bool(_))),
                    );
                    self.out.push(Site {
                        part: Part::Tls,
                        line,
                        what: name,
                        soft,
                    });
                }
            }
            "dangerous" if m.args.is_empty() => self.out.push(Site {
                part: Part::Tls,
                line,
                what: "dangerous".into(),
                soft: true,
            }),
            _ => {}
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

/// The sites of one file, or `None` when `syn` cannot parse it.
fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan::default();
    scan.visit_file(&parsed);
    Some(scan.out)
}

fn vendored(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
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
            let (title, why, severity, confidence) = match (site.part, site.soft) {
                (Part::Key, _) => (
                    format!("`{}` is given a literal key", site.what),
                    "Read the key from a secret store or the environment at start-up; a key in the source is in every clone and every build. \
                     If this is a deliberate fixture key in non-test code, waive the site with a reason.",
                    Severity::High,
                    Confidence::High,
                ),
                (Part::Tls, false) => (
                    format!("`{}` turns TLS verification off", site.what),
                    "Leave certificate and hostname verification on; trust a private CA by adding its certificate instead. \
                     If this is a test-only client, move it under `#[cfg(test)]`.",
                    Severity::High,
                    Confidence::High,
                ),
                (Part::Tls, true) if site.what == "dangerous" => (
                    "`.dangerous()` opts out of rustls' default certificate verification".to_string(),
                    "Check that the custom verifier it installs really verifies the chain, the name and the time; if it accepts anything, keep it to tests.                      If it is deliberate, waive the site with a reason.",
                    Severity::High,
                    Confidence::Medium,
                ),
                (Part::Tls, true) => (
                    format!("`{}` takes a configuration value", site.what),
                    "Check the default: it must be `false`, and the setting must not be reachable from untrusted input.",
                    Severity::High,
                    Confidence::Medium,
                ),
            };
            findings.push(
                Finding::new(ID, severity, confidence, view.name(), title, why)
                    .location(Location::new(path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}", path, site.what))
                    .positive_control(format!(
                        "{} Rust file(s) parsed with syn; the call was read at {path}:{}",
                        files.len() - unparsed.len(),
                        site.line
                    )),
            );
        }
    }
    let notes = if unparsed.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "{ID}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        )]
    };
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(src: &str) -> Vec<(Part, bool, String)> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| (s.part, s.soft, s.what))
            .collect()
    }

    #[test]
    fn a_literal_key_is_found_directly_and_through_a_local() {
        let direct = "fn f() { let k = EncodingKey::from_secret(b\"s3cret\"); }";
        assert_eq!(found(direct).len(), 1);
        let local = "fn f() { let key = \"s3cret\"; let k = Hmac::<Sha256>::new_from_slice(key.as_bytes()); }";
        assert_eq!(found(local).len(), 1);
        let local =
            "fn f() { let key = b\"s3cret\"; let k = Hmac::<Sha256>::new_from_slice(key); }";
        assert_eq!(found(local).len(), 1);
        let ring = "fn f() { let k = hmac::Key::new(hmac::HMAC_SHA256, b\"s3cret\"); }";
        assert_eq!(found(ring).len(), 1);
        let aes = "fn f() { let k = Key::<Aes256Gcm>::from_slice(b\"0123456789abcdef0123456789abcdef\"); }";
        assert_eq!(found(aes).len(), 1);
    }

    #[test]
    fn a_key_from_elsewhere_an_empty_literal_or_another_function_is_not_found() {
        assert!(found("fn f(k: &[u8]) { let a = EncodingKey::from_secret(k); }").is_empty());
        assert!(found("fn f() { let a = EncodingKey::from_secret(&env_key()); }").is_empty());
        assert!(found("fn f() { let a = EncodingKey::from_secret(b\"\"); }").is_empty());
        // a literal in a plain `from_slice` (JSON, say) is not a key
        assert!(found("fn f() { let v = serde_json::from_slice(b\"{}\"); }").is_empty());
        // a local of one function is not a local of the next
        assert!(found(
            "fn a() { let k = b\"x\"; } fn b(k: &[u8]) { let a = EncodingKey::from_secret(k); }"
        )
        .is_empty());
    }

    #[test]
    fn test_code_is_skipped() {
        let src = "#[cfg(test)] mod t { fn f() { EncodingKey::from_secret(b\"x\"); } }\n#[test] fn g() { EncodingKey::from_secret(b\"x\"); }\nfn h() { EncodingKey::from_secret(b\"x\"); }";
        assert_eq!(found(src).len(), 1);
    }

    #[test]
    fn tls_off_is_found_unless_false_and_a_pass_through_is_soft() {
        let on = "fn f() { c.danger_accept_invalid_certs(true); c.danger_accept_invalid_hostnames(true); }";
        assert_eq!(
            found(on).iter().map(|s| s.1).collect::<Vec<_>>(),
            vec![false, false]
        );
        assert!(found("fn f() { c.danger_accept_invalid_certs(false); }").is_empty());
        let cfg = found("fn f() { c.danger_accept_invalid_certs(cfg.insecure); }");
        assert_eq!(cfg.len(), 1);
        assert!(cfg[0].1);
        assert_eq!(
            found("fn f() { b.dangerous().with_custom_certificate_verifier(v); }").len(),
            1
        );
        assert!(found("fn f() { b.dangerous(1); }").is_empty());
    }

    #[test]
    fn dev_directories_and_unparsable_files_are_handled() {
        let view = RepoView::from_files(&[
            ("src/a.rs", "fn f() { EncodingKey::from_secret(b\"x\"); }"),
            ("tests/b.rs", "fn f() { EncodingKey::from_secret(b\"x\"); }"),
            ("src/c.rs", "fn (((\n"),
        ]);
        let Verdict::Noted(f, notes) = judge(&view) else {
            panic!("expected findings and a note")
        };
        assert_eq!(f.len(), 1);
        assert!(notes[0].contains("src/c.rs"), "{notes:?}");
        let none = RepoView::from_files(&[("README.md", "x")]);
        assert!(matches!(judge(&none), Verdict::NotApplicable(_)));
    }
}
