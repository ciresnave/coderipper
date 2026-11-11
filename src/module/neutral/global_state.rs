// SPDX-License-Identifier: MIT OR Apache-2.0
//! EFX-002 and CON-011: process-wide mutable state (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories (`tests/`, `benches/`,
//! `examples/`, `build.rs`, `tests.rs`-style files) is parsed with `syn`; items under `#[cfg(test)]` and functions marked `#[test]` are
//! skipped. A site is a `static` item (at module level or inside a function) that is one of:
//!
//! * `static mut NAME: T`;
//! * a `static` whose type names an interior-mutable or lock type: `Mutex`, `RwLock`, `RefCell`, `Cell`, `UnsafeCell`, `OnceCell`
//!   (write-once, but a registry in practice, so only when it wraps a lock, see below), an `Atomic*` integer or bool, `ArcSwap`;
//!   `OnceLock` and `LazyLock` / `Lazy` are read through to the type they hold, so `static X: LazyLock<Mutex<Vec<T>>>` is a site and
//!   `static X: LazyLock<Regex>` is not;
//! * a `lazy_static! { static ref NAME: T = ..; }` or `thread_local! { static NAME: T = ..; }` entry whose type names one of those.
//!
//! **One scan, two rules.** `EFX-002` (only an allowlisted module owns global mutable state) and `CON-011` (shared state has a declared
//! owner) read the same sites, so they cannot disagree; `EFX-002` reports them at Medium severity, `CON-011` at Low, as the catalog says.
//!
//! **Not reported:** a `static` of a plain type (`&str`, an array, a `Regex` behind a lazy cell, a `OnceLock<T>` of a plain `T`), a
//! `const`, anything in tests, anything inside a macro other than the two above (`syn` does not look inside macro tokens). Other
//! languages are not read.
//!
//! **What it cannot see (and the record says so):** a type alias that hides the lock (`static X: Shared`), a lock reached through a
//! function-local `static` in a macro, an owner declared in a document (the rule asks for an inventory; this reads none, so every site
//! is a finding to waive with the owner named, or to remove), and statics in dependencies. A file `syn` cannot parse is counted in a
//! note, never read as clean.

use super::efx003::is_dev_path;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{Attribute, ItemMacro, ItemMod, ItemStatic, StaticMutability, Type};

#[derive(Debug, Clone)]
pub(super) struct Site {
    pub line: u32,
    /// The static's name.
    pub name: String,
    /// Why it is mutable shared state: `static mut`, or the type word that decided it (`Mutex`, `AtomicUsize`).
    pub kind: String,
}

#[derive(Default)]
struct Scan {
    out: Vec<Site>,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
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
    attr.path().is_ident("cfg")
        && attr
            .parse_args::<syn::Meta>()
            .is_ok_and(|m| m.path().is_ident("test"))
}

/// The type words that make a static mutable shared state.
fn mutable_word(word: &str) -> bool {
    matches!(
        word,
        "Mutex" | "RwLock" | "RefCell" | "Cell" | "UnsafeCell" | "ArcSwap" | "AtomicPtr"
    ) || (word.starts_with("Atomic") && word.len() > "Atomic".len())
}

/// Words that hold a value without making it mutable: read through to the arguments.
fn transparent(word: &str) -> bool {
    matches!(
        word,
        "OnceLock"
            | "OnceCell"
            | "LazyLock"
            | "LazyCell"
            | "Lazy"
            | "Arc"
            | "Rc"
            | "Box"
            | "Option"
    )
}

/// The word that makes a type mutable shared state, found by walking its path segments and generic arguments.
fn mutable_in(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => {
            for seg in &p.path.segments {
                let word = seg.ident.to_string();
                if mutable_word(&word) {
                    return Some(word);
                }
                if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                    for arg in &a.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            if transparent(&word) || mutable_word(&word) {
                                if let Some(found) = mutable_in(inner) {
                                    return Some(found);
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        Type::Paren(p) => mutable_in(&p.elem),
        Type::Group(g) => mutable_in(&g.elem),
        _ => None,
    }
}

/// Words of a type written as tokens (inside `lazy_static!`), for the same decision on the text.
fn mutable_in_tokens(tokens: &str) -> Option<String> {
    tokens
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|w| mutable_word(w))
        .map(str::to_string)
}

impl Scan {
    fn push(&mut self, line: u32, name: String, kind: String) {
        self.out.push(Site { line, name, kind });
    }
}

impl<'ast> Visit<'ast> for Scan {
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

    fn visit_item_static(&mut self, s: &'ast ItemStatic) {
        if s.attrs.iter().any(is_test_attr) {
            return;
        }
        let name = s.ident.to_string();
        let line = line_of(s.ident.span());
        if matches!(s.mutability, StaticMutability::Mut(_)) {
            self.push(line, name, "static mut".to_string());
        } else if let Some(word) = mutable_in(&s.ty) {
            self.push(line, name, word);
        }
    }

    fn visit_item_macro(&mut self, m: &'ast ItemMacro) {
        let last = m.mac.path.segments.last().map(|s| s.ident.to_string());
        if !matches!(last.as_deref(), Some("lazy_static" | "thread_local")) {
            return;
        }
        let tokens = m.mac.tokens.to_string();
        // entries are separated by `;`; each is `[pub] static [ref] NAME : TYPE = EXPR`
        for entry in tokens.split(';') {
            let Some(after) = entry.split("static").nth(1) else {
                continue;
            };
            let after = after.trim_start().trim_start_matches("ref").trim_start();
            let Some((name, rest)) = after.split_once(':') else {
                continue;
            };
            let ty = rest.split('=').next().unwrap_or(rest);
            if let Some(word) = mutable_in_tokens(ty) {
                self.push(
                    line_of(m.mac.path.segments[0].ident.span()),
                    name.trim().to_string(),
                    word,
                );
            }
        }
    }
}

pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan::default();
    scan.visit_file(&parsed);
    let mut out = scan.out;
    out.sort_by_key(|s| (s.line, s.name.clone()));
    out.dedup_by_key(|s| (s.line, s.name.clone()));
    Some(out)
}

fn vendored(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

fn judge(view: &RepoView, id: &'static str, severity: Severity) -> Verdict {
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
            findings.push(
                Finding::new(
                    id,
                    severity,
                    Confidence::Medium,
                    view.name(),
                    format!("`{}` is process-wide mutable state ({})", site.name, site.kind),
                    format!(
                        "`{}` in `{path}` is a global that code anywhere can change (`{}`), so behaviour depends on call order and on what ran before, \
                         and tests cannot run in isolation. Pass the state in explicitly from a composition root. If it must stay global (logging, \
                         metrics, a registry), allow the site and name its owner. Read from the syntax only: a type alias that hides the lock is not seen.",
                        site.name, site.kind
                    ),
                )
                .location(Location::new(path.as_str(), Some(site.line)))
                .subject(format!("{}:{}", path, site.name))
                .positive_control(format!(
                    "{} Rust file(s) parsed with syn; the site was read at {path}:{}",
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

pub(super) fn judge_efx002(view: &RepoView) -> Verdict {
    judge(view, crate::rule_id::EFX_002, Severity::Medium)
}

pub(super) fn judge_con011(view: &RepoView) -> Verdict {
    judge(view, crate::rule_id::CON_011, Severity::Low)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn got(src: &str) -> Vec<(String, String)> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| (s.name, s.kind))
            .collect()
    }

    fn pair(n: &str, k: &str) -> (String, String) {
        (n.to_string(), k.to_string())
    }

    #[test]
    fn static_mut_and_locks_are_reported() {
        let src = "static mut COUNT: u32 = 0;
static REG: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static HITS: AtomicUsize = AtomicUsize::new(0);
static CELL: RefCell<u8> = todo();";
        assert_eq!(
            got(src),
            vec![
                pair("COUNT", "static mut"),
                pair("REG", "Mutex"),
                pair("HITS", "AtomicUsize"),
                pair("CELL", "RefCell")
            ]
        );
    }

    #[test]
    fn a_lazy_or_once_cell_is_read_through_to_what_it_holds() {
        let src = "static A: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(vec![]));
static B: LazyLock<Regex> = LazyLock::new(|| Regex::new(\"x\").unwrap());
static C: OnceLock<Config> = OnceLock::new();
static D: OnceLock<RwLock<Config>> = OnceLock::new();
static E: Lazy<Arc<Mutex<u8>>> = Lazy::new(|| Arc::new(Mutex::new(0)));";
        let names: Vec<String> = got(src).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["A", "D", "E"]);
    }

    #[test]
    fn plain_statics_and_consts_are_not_reported() {
        let src = "static NAME: &str = \"x\";
static TABLE: [u8; 2] = [1, 2];
const LIMIT: Mutex<u8> = Mutex::new(0);";
        assert!(got(src).is_empty());
    }

    #[test]
    fn lazy_static_and_thread_local_entries_are_read() {
        let src = "lazy_static! {
    static ref REGISTRY: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static ref RE: Regex = Regex::new(\"x\").unwrap();
}
thread_local! {
    static DEPTH: RefCell<u32> = RefCell::new(0);
}";
        assert_eq!(
            got(src),
            vec![pair("REGISTRY", "Mutex"), pair("DEPTH", "RefCell")]
        );
    }

    #[test]
    fn a_function_local_static_is_reported_and_test_code_is_not() {
        let src = "fn f() { static N: AtomicU32 = AtomicU32::new(0); }
#[cfg(test)]
mod tests { static T: Mutex<u8> = Mutex::new(0); }
#[test]
fn t() { static U: AtomicU32 = AtomicU32::new(0); }";
        assert_eq!(got(src), vec![pair("N", "AtomicU32")]);
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("static (((").is_none());
    }
}
