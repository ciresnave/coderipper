// SPDX-License-Identifier: MIT OR Apache-2.0
//! EFX-007: shared-mutation spots say who owns them (Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored, fixture and development directories is parsed with `syn`; items under
//! `#[cfg(test)]` and functions marked `#[test]` are skipped. A site is a struct field (named or tuple) or a `static` item whose type is
//! interior-mutable or a lock, by the same reading `EFX-002` uses (`Cell`, `RefCell`, `UnsafeCell`, `Mutex`, `RwLock`, `ArcSwap`,
//! `Atomic*`, read through `Arc`, `Rc`, `Box`, `Option`, `OnceLock`, `LazyLock` and `Lazy` to what they hold). A site is reported when it
//! carries no documentation: no `///` or `#[doc]` on it, and no `//` comment on the line directly above it.
//!
//! **How it differs from `EFX-002`.** `EFX-002` asks which modules may own *global* mutable state and reports every such static;
//! this rule asks whether *any* shared-mutation spot, field or static, says who owns it, and stays silent when it does. A documented
//! static is clean here and still an `EFX-002` finding.
//!
//! **Not judged:** whether the comment names an owner, an access discipline or an invalidation rule (presence only), a type alias that
//! hides the lock, `lazy_static!` and `thread_local!` entries, enum variant fields, other languages. A file `syn` cannot parse is counted
//! in a note, never read as clean.

use super::efx003::is_dev_path;
use super::global_state::{is_test_attr, line_of, mutable_in, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;
use syn::{Attribute, Fields, ItemMod, ItemStatic, ItemStruct, StaticMutability};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub line: u32,
    /// `Struct.field` for a field, the name for a static.
    pub name: String,
    /// The type word that decided it, or `static mut`.
    pub kind: String,
    pub is_static: bool,
}

struct Scan<'a> {
    lines: Vec<&'a str>,
    out: Vec<Site>,
}

fn has_doc(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("doc"))
}

impl Scan<'_> {
    /// A `//` or `/* */` comment on the line directly above the 1-based `line` (attributes in between are skipped).
    fn comment_above(&self, line: u32) -> bool {
        let mut i = usize::try_from(line).unwrap_or(0).saturating_sub(1);
        while i > 0 {
            i -= 1;
            let t = self.lines.get(i).map_or("", |l| l.trim());
            if t.starts_with("#[") || t.starts_with("#!") {
                continue;
            }
            return t.starts_with("//") || t.ends_with("*/");
        }
        false
    }

    fn push(&mut self, site: Site, attrs: &[Attribute]) {
        if !has_doc(attrs) && !self.comment_above(site.line) {
            self.out.push(site);
        }
    }
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

    fn visit_item_static(&mut self, s: &'ast ItemStatic) {
        if s.attrs.iter().any(is_test_attr) {
            return;
        }
        let kind = if matches!(s.mutability, StaticMutability::Mut(_)) {
            Some("static mut".to_string())
        } else {
            mutable_in(&s.ty)
        };
        if let Some(kind) = kind {
            let site = Site {
                line: line_of(s.ident.span()),
                name: s.ident.to_string(),
                kind,
                is_static: true,
            };
            self.push(site, &s.attrs);
        }
    }

    fn visit_item_struct(&mut self, s: &'ast ItemStruct) {
        if s.attrs.iter().any(is_test_attr) {
            return;
        }
        let fields: Vec<&syn::Field> = match &s.fields {
            Fields::Named(n) => n.named.iter().collect(),
            Fields::Unnamed(u) => u.unnamed.iter().collect(),
            Fields::Unit => Vec::new(),
        };
        for (i, f) in fields.into_iter().enumerate() {
            let Some(kind) = mutable_in(&f.ty) else {
                continue;
            };
            let (field, line) = match &f.ident {
                Some(id) => (id.to_string(), line_of(id.span())),
                None => (i.to_string(), line_of(syn::spanned::Spanned::span(&f.ty))),
            };
            let site = Site {
                line,
                name: format!("{}.{field}", s.ident),
                kind,
                is_static: false,
            };
            self.push(site, &f.attrs);
        }
    }
}

pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan {
        lines: text.lines().collect(),
        out: Vec::new(),
    };
    scan.visit_file(&parsed);
    let mut out = scan.out;
    out.sort_by_key(|s| (s.line, s.name.clone()));
    out.dedup_by_key(|s| (s.line, s.name.clone()));
    Some(out)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let id = crate::rule_id::EFX_007;
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
            let what = if site.is_static { "static" } else { "field" };
            findings.push(
                Finding::new(
                    id,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!(
                        "shared-mutation {what} `{}` ({}) has no comment saying who owns it",
                        site.name, site.kind
                    ),
                    format!(
                        "`{}` in `{path}` can be changed through a shared reference (`{}`) and carries no doc comment or comment above it. Say who owns \
                         it, who may change it, and when its contents are invalidated, or remove the sharing. Presence of a comment is all this reads; \
                         what it says is not judged.",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        sites(src)
            .expect("parses")
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn an_undocumented_cell_field_and_static_are_reported() {
        let src = "struct S {\n    hits: Cell<u32>,\n    cache: Arc<Mutex<Vec<u8>>>,\n    name: String,\n}\nstatic REG: Mutex<u8> = Mutex::new(0);";
        assert_eq!(names(src), vec!["S.hits", "S.cache", "REG"]);
    }

    #[test]
    fn a_doc_comment_or_a_line_comment_above_clears_the_site() {
        let src = "struct S {\n    /// Owned by the render loop.\n    a: Cell<u32>,\n    // Written only under the pool lock.\n    #[allow(dead_code)]\n    b: RefCell<u8>,\n    c: AtomicUsize,\n}";
        assert_eq!(names(src), vec!["S.c"]);
    }

    #[test]
    fn plain_fields_and_test_code_are_not_reported() {
        let src = "struct P { a: u32, b: String }\n#[cfg(test)]\nmod tests { struct T { c: Cell<u8> } }\nstatic NAME: &str = \"x\";";
        assert!(names(src).is_empty());
    }

    #[test]
    fn a_tuple_field_is_named_by_position() {
        assert_eq!(names("struct W(RefCell<u8>);"), vec!["W.0"]);
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("struct (((").is_none());
    }
}
