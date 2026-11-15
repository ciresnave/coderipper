// SPDX-License-Identifier: MIT OR Apache-2.0
//! ERR-009 and SEC-008: a secret named in a message that leaves the program.
//!
//! One scan of each package's Rust source (the same walk for both rules, so they cannot disagree about a site) finds a macro whose
//! tokens name a sensitive value. Every tracked `.rs` file of a package (outside vendored directories, tests, examples, tools and
//! `build.rs`, and outside `#[cfg(test)]` and test functions) is parsed with `syn`, and the tokens of each macro are read directly
//! (`syn` does not parse a macro's arguments).
//!
//! * **`SEC-008` (no sensitive data in logs or telemetry)** reports a call of a logging macro (`error!`, `warn!`, `info!`, `debug!`,
//!   `trace!`, `log!`, `event!` and the `tracing` span macros) that names a sensitive value.
//! * **`ERR-009` (errors and logs leak no sensitive data)** reports the same reading for the macros that build an error message:
//!   `anyhow!`, `bail!`, `ensure!` (not its condition), `eyre!`, `format_err!`, `panic!`, a `format!` inside `Err(..)`, `.context(..)`,
//!   `.with_context(..)`, `.map_err(..)`, `.ok_or(..)`, `.ok_or_else(..)` or `.expect(..)`, and `write!`, `writeln!` or `format!` inside
//!   the `Display` implementation of a type whose name ends in `Error` or `Err`.
//!
//! A name is **sensitive** when its last word is `password`, `passwd`, `passphrase`, `secret`, `token`, `credential(s)`, `cookie`,
//! `authorization`, `bearer` or `apikey`, or its last two words are `api_key`, `private_key`, `secret_key`, `signing_key`, `access_key`,
//! `session_key`, `auth_key`, `master_key` or `encryption_key` (snake_case and camelCase are both split into words, so `token_count`,
//! `token_type` and `tokenizer` are not). A name whose first word is `is`, `has`, `num`, `max`, `min` or `len` is not. It counts when it
//! is a variable or field in the arguments (`self.password`, `cfg.api_key`), a named field of a `tracing` macro (`token = ?t`), or an
//! inline capture of a format string (`"bad {token}"`). It does not count when it is the receiver of `len`, `is_empty`, `is_some`,
//! `is_none`, `is_ok` or `is_err`, when it is passed through a call whose name says it redacts (`redact`, `mask`, `hash`, `digest`,
//! `fingerprint`, `obscure`, `hide`, `scrub`, `sanitize`, `anonym`, `censor`, `safe`), or when a method of that kind is called on it (`credential.safe_display()`), when it is a name followed by `(` (a call), or when a named
//! field is given a literal.
//!
//! **What it cannot see:** a secret behind a name that does not say so, a `{:?}` of a whole struct that holds one, a value whose type
//! redacts itself on display, a log made through a wrapper, other languages, and a secret named in a macro the package defines. The
//! finding is a reading of names, not of data flow: **Low confidence**. `SEC-008` is Medium severity, `ERR-009` High. A file `syn`
//! cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use proc_macro2::{Delimiter, Spacing, TokenTree};
use syn::visit::Visit;

const LOG_MACROS: &[&str] = &[
    "error",
    "warn",
    "info",
    "debug",
    "trace",
    "log",
    "event",
    "span",
    "trace_span",
    "debug_span",
    "info_span",
    "warn_span",
    "error_span",
];
const ERROR_MACROS: &[&str] = &["anyhow", "bail", "ensure", "eyre", "format_err", "panic"];
const FORMAT_MACROS: &[&str] = &["format", "write", "writeln"];
const ERROR_CALLS: &[&str] = &[
    "context",
    "with_context",
    "map_err",
    "ok_or",
    "ok_or_else",
    "expect",
];
const SINGLE: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "credential",
    "credentials",
    "cookie",
    "authorization",
    "bearer",
    "apikey",
];
const PAIRS: &[&str] = &[
    "api_key",
    "private_key",
    "secret_key",
    "signing_key",
    "access_key",
    "session_key",
    "auth_key",
    "master_key",
    "encryption_key",
];
const NOT_FIRST: &[&str] = &["is", "has", "num", "max", "min", "len"];
const SAFE_METHODS: &[&str] = &["len", "is_empty", "is_some", "is_none", "is_ok", "is_err"];
const REDACTORS: &[&str] = &[
    "redact",
    "mask",
    "hash",
    "digest",
    "fingerprint",
    "obscure",
    "hide",
    "scrub",
    "sanitize",
    "anonym",
    "censor",
    "safe",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Log,
    Error,
}

struct Site {
    line: u32,
    function: String,
    kind: Kind,
    macro_name: String,
    name: String,
}

/// The words of an identifier: snake_case and camelCase both split, lower-cased.
fn words(ident: &str) -> Vec<String> {
    let mut spaced = String::new();
    let mut prev_lower = false;
    for c in ident.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            spaced.push('_');
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        spaced.push(c.to_ascii_lowercase());
    }
    spaced
        .split('_')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn sensitive(ident: &str) -> bool {
    let w = words(ident);
    let Some(last) = w.last() else {
        return false;
    };
    if NOT_FIRST.contains(&w[0].as_str()) && w.len() > 1 {
        return false;
    }
    if SINGLE.contains(&last.as_str()) {
        return true;
    }
    w.len() >= 2 && PAIRS.contains(&format!("{}_{}", w[w.len() - 2], last).as_str())
}

fn is_redactor(ident: &str) -> bool {
    let l = ident.to_ascii_lowercase();
    REDACTORS.iter().any(|r| l.contains(r))
}

/// The sensitive names a format-string literal captures inline: `"bad {token}"`, `"{api_key:?}"`.
fn captures(lit: &str) -> Vec<String> {
    let b: Vec<char> = lit.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '{' {
            if b.get(i + 1) == Some(&'{') {
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
                j += 1;
            }
            if j > i + 1 && matches!(b.get(j), Some('}') | Some(':')) {
                let name: String = b[i + 1..j].iter().collect();
                if sensitive(&name) {
                    out.push(name);
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// The tokens of one argument value after a named field's `=`, up to the next top-level comma.
fn value_after_eq(tokens: &[TokenTree], from: usize) -> &[TokenTree] {
    let end = tokens[from..]
        .iter()
        .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','))
        .map_or(tokens.len(), |n| from + n);
    &tokens[from..end]
}

fn mentions_redactor(tokens: &[TokenTree]) -> bool {
    tokens.iter().any(|t| match t {
        TokenTree::Ident(i) => is_redactor(&i.to_string()),
        TokenTree::Group(g) => mentions_redactor(&g.stream().into_iter().collect::<Vec<_>>()),
        _ => false,
    })
}

fn scan(tokens: &[TokenTree], out: &mut Vec<(u32, String)>) {
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Group(g) => {
                let after_redactor = i > 0
                    && matches!(&tokens[i - 1], TokenTree::Ident(id) if is_redactor(&id.to_string()));
                if !after_redactor {
                    scan(&g.stream().into_iter().collect::<Vec<_>>(), out);
                }
            }
            TokenTree::Literal(l) => {
                let text = l.to_string();
                if text.starts_with('"') || text.starts_with("r\"") || text.starts_with("r#") {
                    for name in captures(&text) {
                        out.push((u32::try_from(l.span().start().line).unwrap_or(0), name));
                    }
                }
            }
            TokenTree::Ident(id) => {
                let name = id.to_string();
                let next = tokens.get(i + 1);
                let is_call_or_path = match next {
                    Some(TokenTree::Group(g)) => g.delimiter() == Delimiter::Parenthesis,
                    Some(TokenTree::Punct(p)) => {
                        p.as_char() == '!' || (p.as_char() == ':' && p.spacing() == Spacing::Joint)
                    }
                    _ => false,
                };
                if !is_call_or_path && sensitive(&name) {
                    let line = u32::try_from(id.span().start().line).unwrap_or(0);
                    let safe_method = matches!(next, Some(TokenTree::Punct(p)) if p.as_char() == '.')
                        && matches!(tokens.get(i + 2), Some(TokenTree::Ident(m)) if SAFE_METHODS.contains(&m.to_string().as_str()) || is_redactor(&m.to_string()));
                    let named_field = matches!(next, Some(TokenTree::Punct(p)) if p.as_char() == '=' && p.spacing() == Spacing::Alone);
                    if safe_method {
                        i += 3;
                        continue;
                    }
                    // `auth_token.user_id`: a field of a token object, not the secret itself (the field is read on its own turn).
                    let plain_field = matches!(next, Some(TokenTree::Punct(p)) if p.as_char() == '.')
                        && matches!(tokens.get(i + 2), Some(TokenTree::Ident(_)))
                        && !matches!(tokens.get(i + 3), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis);
                    if plain_field {
                        i += 1;
                        continue;
                    }
                    if named_field {
                        let value = value_after_eq(tokens, i + 2);
                        let literal = value.len() == 1 && matches!(value[0], TokenTree::Literal(_));
                        if !literal && !mentions_redactor(value) {
                            out.push((line, name));
                        }
                    } else {
                        out.push((line, name));
                    }
                }
            }
            TokenTree::Punct(_) => {}
        }
        i += 1;
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

fn last_segment(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|s| s.ident.to_string())
}

struct Walk {
    functions: Vec<String>,
    /// How many `Err(..)`, `.context(..)` and similar calls enclose the current expression.
    error_depth: usize,
    /// How many `impl Display for ...Error` blocks enclose it.
    error_display: usize,
    sites: Vec<Site>,
}

impl Walk {
    fn report(&mut self, kind: Kind, mac: &syn::Macro, skip_first_arg: bool) {
        let mut tokens: Vec<TokenTree> = mac.tokens.clone().into_iter().collect();
        if skip_first_arg {
            let cut = tokens
                .iter()
                .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','))
                .map_or(tokens.len(), |n| n + 1);
            tokens.drain(..cut);
        }
        let mut found = Vec::new();
        scan(&tokens, &mut found);
        found.sort();
        found.dedup();
        let macro_name = last_segment(&mac.path).unwrap_or_default();
        for (line, name) in found {
            self.sites.push(Site {
                line,
                function: self
                    .functions
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "<module>".into()),
                kind,
                macro_name: macro_name.clone(),
                name,
            });
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

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let display = i
            .trait_
            .as_ref()
            .and_then(|(p, _)| last_segment(p))
            .is_some_and(|n| n == "Display");
        let error_type = matches!(&*i.self_ty, syn::Type::Path(p)
            if last_segment(&p.path).is_some_and(|n| n.ends_with("Error") || n.ends_with("Err")));
        let on = display && error_type;
        if on {
            self.error_display += 1;
        }
        syn::visit::visit_item_impl(self, i);
        if on {
            self.error_display -= 1;
        }
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        let err = matches!(&*c.func, syn::Expr::Path(p) if last_segment(&p.path).is_some_and(|n| n == "Err"));
        if err {
            self.error_depth += 1;
        }
        syn::visit::visit_expr_call(self, c);
        if err {
            self.error_depth -= 1;
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let err = ERROR_CALLS.contains(&m.method.to_string().as_str());
        self.visit_expr(&m.receiver);
        if err {
            self.error_depth += 1;
        }
        for a in &m.args {
            self.visit_expr(a);
        }
        if err {
            self.error_depth -= 1;
        }
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if let Some(name) = last_segment(&m.path) {
            let n = name.as_str();
            if LOG_MACROS.contains(&n) {
                self.report(Kind::Log, m, false);
            } else if ERROR_MACROS.contains(&n) {
                self.report(Kind::Error, m, n == "ensure");
            } else if FORMAT_MACROS.contains(&n) && (self.error_depth > 0 || self.error_display > 0)
            {
                self.report(Kind::Error, m, n == "write" || n == "writeln");
            }
        }
        syn::visit::visit_macro(self, m);
    }
}

fn sites_of(view: &RepoView, id: &str, wanted: Kind, severity: Severity) -> Verdict {
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
            let mut walk = Walk {
                functions: Vec::new(),
                error_depth: 0,
                error_display: 0,
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites.into_iter().filter(|x| x.kind == wanted) {
                let (what, advice) = if wanted == Kind::Log {
                    (
                        format!("`{}!` names `{}`", site.macro_name, site.name),
                        "A log record outlives the request and is read by people and systems that were never meant to hold the secret. Log an identifier, a length or a fingerprint instead, or wrap the value in a type whose `Debug` and `Display` print a placeholder.",
                    )
                } else {
                    (
                        format!("`{}` puts `{}` in an error message", site.macro_name, site.name),
                        "An error message is shown to users, written to logs and sent to trackers, so a secret in it leaves the program. Name what failed and leave the value out, or wrap the value in a type whose `Debug` and `Display` print a placeholder.",
                    )
                };
                findings.push(
                    Finding::new(
                        id,
                        severity,
                        Confidence::Low,
                        view.name(),
                        format!("in `{}`: {}", site.function, what),
                        advice,
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}:{}", s.path, site.function, site.macro_name, site.name))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads the tokens of logging and error-message macros"
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
    match unparsed_note(id, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

/// `SEC-008`: a logging macro that names a sensitive value.
pub(super) fn judge_sec008(view: &RepoView) -> Verdict {
    sites_of(view, crate::rule_id::SEC_008, Kind::Log, Severity::Medium)
}

/// `ERR-009`: an error message that names a sensitive value.
pub(super) fn judge_err009(view: &RepoView) -> Verdict {
    sites_of(view, crate::rule_id::ERR_009, Kind::Error, Severity::High)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn run(judge: fn(&RepoView) -> Verdict, src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_log_macro_naming_a_secret_is_reported_in_each_form() {
        let src = "pub fn a(password: &str, cfg: &C) {
    log::info!(\"login {}\", password);
    tracing::debug!(api_key = ?cfg.api_key, \"x\");
    warn!(\"bad {access_token}\");
    error!(\"{:?}\", self.secret);
}
";
        let f = run(judge_sec008, src);
        assert_eq!(f.len(), 4, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(run(judge_err009, src).is_empty());
    }

    #[test]
    fn a_redacted_counted_or_unrelated_name_is_not_reported() {
        let src = "pub fn a(password: &str, token_count: usize, tokenizer: &T, session_id: u64) {
    info!(\"n {}\", password.len());
    info!(\"{}\", redact(password));
    info!(token = \"***\", \"x\");
    info!(\"{} {} {}\", token_count, tokenizer, session_id);
    info!(\"{}\", get_token());
    info!(\"{} {}\", auth_token.user_id, token.issuer);
    info!(\"{}\", credential.safe_display());
    info!(\"braces {{token}}\");
    #[cfg(test)] fn t() { info!(\"{}\", password); }
}
";
        let f = run(judge_sec008, src);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn an_error_message_naming_a_secret_is_reported_in_each_form() {
        let src = "pub fn a(token: &str) -> Result<(), E> {
    bail!(\"bad {}\", token);
}
pub fn b(secret: &str) -> Result<(), String> {
    Err(format!(\"rejected {secret}\"))
}
pub fn c(api_key: &str) -> Result<(), E> {
    x().map_err(|e| anyhow!(\"{e} {api_key}\"))?;
    ensure!(token == other, \"no secrets here\");
    Ok(())
}
impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, \"bad {}\", self.password)
    }
}
";
        let f = run(judge_err009, src);
        assert_eq!(f.len(), 4, "{f:?}");
        assert!(run(judge_sec008, src).is_empty());
    }

    #[test]
    fn a_format_outside_an_error_context_is_not_reported() {
        let src = "pub fn a(token: &str) -> String { format!(\"Bearer {}\", token) }
";
        assert!(run(judge_err009, src).is_empty());
    }

    #[test]
    fn words_split_both_cases() {
        assert!(sensitive("apiKey") && sensitive("AccessToken") && sensitive("API_KEY"));
        assert!(!sensitive("key") && !sensitive("has_password") && !sensitive("token_type"));
    }
}
