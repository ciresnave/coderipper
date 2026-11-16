// SPDX-License-Identifier: MIT OR Apache-2.0
//! RDB-002: identifiers follow the naming conventions of Rust.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. Four kinds of name are checked against the
//! convention of the Rust API guidelines: a function or method name, a `let` binding and a parameter must be `snake_case` (no upper-case
//! letter); a struct, enum, union, trait, type alias and enum variant must be `UpperCamelCase` (it does not start in lower case, has no
//! `__`, and no `_` beside a letter); a `const` and a `static` must be `SCREAMING_SNAKE_CASE` (no lower-case letter). A leading `_`
//! and a raw-identifier prefix (`r#`) are ignored. One **Low**-severity, High-confidence finding per offending name, at the name.
//!
//! **Not reported:** an item under `#[allow(non_snake_case)]`, `non_camel_case_types` or `non_upper_case_globals` (also as an inner
//! attribute of the module or file), an item with `#[no_mangle]` or `#[export_name]`, anything inside an `extern` block, a method of
//! a trait implementation (its name is the trait's), and anything written inside a macro invocation (`syn` does not expand it).
//!
//! **What it cannot see:** the conventions of a project that differ from the language's, an acronym rule (`HTTPServer` is reported
//! only if it breaks the rule above, which it does not), a name made by a macro, and the other languages. rustc has the same three
//! lints (`non_snake_case`, `non_camel_case_types`, `non_upper_case_globals`) at type-checker precision and warns by default; this
//! rule reads a tree without building it and reports a repository that has silenced them. A file `syn` cannot parse is named in a
//! note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;

const ID: &str = crate::rule_id::RDB_002;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Snake,
    Camel,
    ScreamingSnake,
}

impl Style {
    fn lint(self) -> &'static str {
        match self {
            Style::Snake => "non_snake_case",
            Style::Camel => "non_camel_case_types",
            Style::ScreamingSnake => "non_upper_case_globals",
        }
    }

    fn wanted(self) -> &'static str {
        match self {
            Style::Snake => "snake_case",
            Style::Camel => "UpperCamelCase",
            Style::ScreamingSnake => "SCREAMING_SNAKE_CASE",
        }
    }

    fn holds(self, name: &str) -> bool {
        let name = name.strip_prefix("r#").unwrap_or(name);
        let core = name.trim_matches('_');
        if core.is_empty() {
            return true;
        }
        match self {
            Style::Snake => !core.chars().any(char::is_uppercase),
            Style::ScreamingSnake => !core.chars().any(char::is_lowercase),
            Style::Camel => {
                let cased = |c: char| c.is_uppercase() || c.is_lowercase();
                let chars: Vec<char> = core.chars().collect();
                !chars[0].is_lowercase()
                    && !core.contains("__")
                    && !chars
                        .windows(2)
                        .any(|w| (cased(w[0]) && w[1] == '_') || (cased(w[1]) && w[0] == '_'))
            }
        }
    }
}

struct Site {
    line: u32,
    name: String,
    kind: &'static str,
    style: Style,
}

struct Walk {
    sites: Vec<Site>,
    in_trait_impl: u32,
}

fn allows(attrs: &[syn::Attribute], lint: &str) -> bool {
    attrs.iter().any(|a| {
        let p = a.path();
        if !(p.is_ident("allow") || p.is_ident("expect")) {
            return false;
        }
        let mut hit = false;
        let _ = a.parse_nested_meta(|m| {
            if m.path.is_ident(lint) || m.path.is_ident("nonstandard_style") {
                hit = true;
            }
            Ok(())
        });
        hit
    })
}

fn is_foreign_name(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("no_mangle") || a.path().is_ident("export_name"))
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

impl Walk {
    fn check(
        &mut self,
        ident: &syn::Ident,
        kind: &'static str,
        style: Style,
        attrs: &[syn::Attribute],
    ) {
        let name = ident.to_string();
        if style.holds(&name) || allows(attrs, style.lint()) {
            return;
        }
        self.sites.push(Site {
            line: u32::try_from(ident.span().start().line).unwrap_or(0),
            name,
            kind,
            style,
        });
    }

    fn pat(&mut self, p: &syn::Pat, attrs: &[syn::Attribute]) {
        match p {
            syn::Pat::Ident(i) => self.check(&i.ident, "binding", Style::Snake, attrs),
            syn::Pat::Type(t) => self.pat(&t.pat, attrs),
            syn::Pat::Reference(r) => self.pat(&r.pat, attrs),
            syn::Pat::Tuple(t) => t.elems.iter().for_each(|e| self.pat(e, attrs)),
            syn::Pat::TupleStruct(t) => t.elems.iter().for_each(|e| self.pat(e, attrs)),
            syn::Pat::Slice(t) => t.elems.iter().for_each(|e| self.pat(e, attrs)),
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) || allows(&i.attrs, "nonstandard_style") {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_foreign_mod(&mut self, _: &'ast syn::ItemForeignMod) {}

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        if !is_foreign_name(&i.attrs) {
            self.check(&i.sig.ident, "function", Style::Snake, &i.attrs);
        }
        for input in &i.sig.inputs {
            if let syn::FnArg::Typed(t) = input {
                self.pat(&t.pat, &i.attrs);
            }
        }
        if !allows(&i.attrs, "non_snake_case") {
            syn::visit::visit_block(self, &i.block);
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        if self.in_trait_impl == 0 {
            self.check(&i.sig.ident, "method", Style::Snake, &i.attrs);
            for input in &i.sig.inputs {
                if let syn::FnArg::Typed(t) = input {
                    self.pat(&t.pat, &i.attrs);
                }
            }
        }
        if !allows(&i.attrs, "non_snake_case") {
            syn::visit::visit_block(self, &i.block);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let tr = u32::from(i.trait_.is_some());
        self.in_trait_impl += tr;
        syn::visit::visit_item_impl(self, i);
        self.in_trait_impl -= tr;
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        self.pat(&l.pat, &l.attrs);
        syn::visit::visit_local(self, l);
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.check(&i.ident, "struct", Style::Camel, &i.attrs);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.check(&i.ident, "enum", Style::Camel, &i.attrs);
        for v in &i.variants {
            self.check(&v.ident, "variant", Style::Camel, &i.attrs);
        }
    }

    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.check(&i.ident, "union", Style::Camel, &i.attrs);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.check(&i.ident, "trait", Style::Camel, &i.attrs);
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.check(&i.ident, "type alias", Style::Camel, &i.attrs);
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        // `const _: () = ..;` is an unnamed assertion, not a constant.
        if i.ident != "_" {
            self.check(&i.ident, "const", Style::ScreamingSnake, &i.attrs);
        }
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        if !is_foreign_name(&i.attrs) {
            self.check(&i.ident, "static", Style::ScreamingSnake, &i.attrs);
        }
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
        for s in &p.sources {
            if allows(&s.file.attrs, "nonstandard_style") {
                continue;
            }
            let mut walk = Walk {
                sites: Vec::new(),
                in_trait_impl: 0,
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Low,
                        Confidence::High,
                        view.name(),
                        format!(
                            "{} `{}` is not {}",
                            site.kind,
                            site.name,
                            site.style.wanted()
                        ),
                        "Readers tell kinds of things apart by the shape of a name. Rename it to the Rust convention (snake_case for functions and bindings, UpperCamelCase for types and variants, SCREAMING_SNAKE_CASE for constants), or say why it must differ with an allow attribute that names the lint.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.kind, site.name))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads item, binding and parameter names"
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

    fn subjects(src: &str) -> Vec<String> {
        found(src)
            .into_iter()
            .map(|f| f.subject.unwrap().split_once(':').unwrap().1.to_string())
            .collect()
    }

    #[test]
    fn each_kind_of_name_is_checked_against_its_own_convention() {
        let src = "pub fn DoThing(Bad: u8) { let myVar = 1; }
pub struct point_2d;
pub enum Shape { circle, Square }
pub const maxSize: u8 = 1;
pub static counter: u8 = 0;
pub trait my_trait {}
pub type alias_t = u8;
";
        let s = subjects(src);
        for want in [
            "function:DoThing",
            "binding:Bad",
            "binding:myVar",
            "struct:point_2d",
            "variant:circle",
            "const:maxSize",
            "static:counter",
            "trait:my_trait",
            "type alias:alias_t",
        ] {
            assert!(s.iter().any(|x| x == want), "{want} in {s:?}");
        }
        assert_eq!(s.len(), 9, "{s:?}");
    }

    #[test]
    fn conforming_names_are_not_reported() {
        let src = "pub fn do_thing(user_id: u8, _unused: u8) { let r#type = 1; let _ = r#type; }
pub struct Point2D;
pub struct Http2Server;
pub enum Shape { Circle, Square }
pub const MAX_SIZE: u8 = 1;
const _: () = ();
pub static COUNTER: u8 = 0;
pub trait Reader {}
pub type Alias = u8;
";
        assert!(found(src).is_empty(), "{:?}", subjects(src));
    }

    #[test]
    fn a_trait_impl_method_takes_the_trait_name_and_an_allow_clears_an_item() {
        let src = "pub trait T { fn Odd(&self); }
pub struct S;
impl T for S { fn Odd(&self) {} }
#[allow(non_snake_case)]
pub fn LegacyName() {}
#[no_mangle]
pub extern \"C\" fn CExport() {}
extern \"C\" { fn CImport(); }
";
        // The trait's declaration, the impl's method, the allowed fn, the exported fn and the import are all left alone.
        assert!(found(src).is_empty(), "{:?}", subjects(src));
        // The same name outside those shapes is reported (positive control: the walk does read these names).
        assert_eq!(
            subjects("pub struct S;\nimpl S { pub fn Odd(&self) {} }\n"),
            vec!["method:Odd".to_string()]
        );
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn BadName() {} }\n#[test]\nfn AlsoBad() {}\n";
        assert!(found(src).is_empty(), "{:?}", subjects(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn BadName() {}\n") {
            f.validate().unwrap();
        }
    }
}
