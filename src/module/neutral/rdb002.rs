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
//! **Not reported:** an item under `#[allow(non_snake_case)]`, `non_camel_case_types`, `non_upper_case_globals` or `nonstandard_style`, a
//! module or file under an inner `#![allow(nonstandard_style)]` (an inner allow of one of the other three lints is **not** read yet), an item with `#[no_mangle]` or `#[export_name]` (not in the edition-2024 `#[unsafe(no_mangle)]` spelling: 0.47.12 reports those, fixed in 0.47.14), anything inside an `extern` block, a method of
//! a trait implementation (its name is the trait's), and anything written inside a macro invocation (`syn` does not expand it).
//!
//! **What it cannot see:** the conventions of a project that differ from the language's, an acronym rule (`HTTPServer` is reported
//! only if it breaks the rule above, which it does not), a name made by a macro, and the other languages. rustc has the same three
//! lints (`non_snake_case`, `non_camel_case_types`, `non_upper_case_globals`) at type-checker precision and warns by default; this
//! rule reads a tree without building it and reports a repository that has silenced them. A file `syn` cannot parse is named in a
//! note, never read as clean. Exception in 0.47.12: when it is the repository's only Rust file the run says `not applicable` and does not name it (fixed in 0.47.14).

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
    /// The naming lints an enclosing module or the file allows (`#![allow(non_snake_case)]` and the like).
    allowed: Vec<&'static str>,
}

const LINTS: [&str; 3] = [
    "non_snake_case",
    "non_camel_case_types",
    "non_upper_case_globals",
];

impl Walk {
    /// Push the lints that `attrs` allow for everything below (the file's inner attributes, which are never popped).
    fn allow_lints(&mut self, attrs: &[syn::Attribute]) {
        for lint in LINTS {
            if allows(attrs, lint) {
                self.allowed.push(lint);
            }
        }
    }

    /// Push the lints a module's `attrs` allow, and say how many, so the caller can pop them when the module ends.
    fn allow_scope(&mut self, attrs: &[syn::Attribute]) -> usize {
        let before = self.allowed.len();
        self.allow_lints(attrs);
        self.allowed.len() - before
    }
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

/// `#[no_mangle]` and `#[export_name = ..]`, also in the edition-2024 form `#[unsafe(no_mangle)]` and `#[unsafe(export_name = ..)]`.
fn is_foreign_name(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = a.path();
        if p.is_ident("no_mangle") || p.is_ident("export_name") {
            return true;
        }
        let mut hit = false;
        if p.is_ident("unsafe") {
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("no_mangle") || m.path.is_ident("export_name") {
                    hit = true;
                }
                // consume `= "name"` so the walk does not stop on it
                if m.input.peek(syn::Token![=]) {
                    let _ = m.value().and_then(|v| v.parse::<syn::Expr>());
                }
                Ok(())
            });
        }
        hit
    })
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
        if style.holds(&name) || allows(attrs, style.lint()) || self.allowed.contains(&style.lint())
        {
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
        if is_cfg_test(&i.attrs) {
            return;
        }
        let pushed = self.allow_scope(&i.attrs);
        syn::visit::visit_item_mod(self, i);
        self.allowed.truncate(self.allowed.len() - pushed);
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
        return super::rust_package::nothing_to_read(ID, &loaded);
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            let mut walk = Walk {
                sites: Vec::new(),
                in_trait_impl: 0,
                allowed: Vec::new(),
            };
            walk.allow_lints(&s.file.attrs);
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
    fn an_inner_allow_of_a_file_or_module_covers_only_its_own_lint_and_its_scope() {
        // the file allows snake case only: a camel-case violation is still reported, a snake-case one is not
        let file = "#![allow(non_snake_case)]
pub fn DoThing() {}
pub struct lower_type;
";
        assert_eq!(subjects(file), vec!["struct:lower_type".to_string()]);
        // `nonstandard_style` covers all three
        assert!(found(
            "#![allow(nonstandard_style)]
pub fn A() {}
pub struct b;
"
        )
        .is_empty());
        // a module's inner allow covers the module and not the code after it
        let module = "pub mod m {
    #![allow(non_snake_case)]
    pub fn Inside() {}
}
pub fn Outside() {}
";
        assert_eq!(subjects(module), vec!["function:Outside".to_string()]);
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

    fn kinds(src: &str) -> Vec<String> {
        found(src)
            .into_iter()
            .map(|f| f.subject.unwrap().split_once(':').unwrap().1.to_string())
            .collect()
    }

    #[test]
    fn every_documented_kind_of_name_is_read_including_methods_unions_and_parameters() {
        let src = "pub struct S;\nimpl S { pub fn Bad(&self, Arg: u8) {} }\npub union Un_ion { a: u8 }\npub enum E { Ok_v }\n";
        let k = kinds(src);
        for want in ["method:Bad", "binding:Arg", "union:Un_ion", "variant:Ok_v"] {
            assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
        }
    }

    #[test]
    fn the_camel_case_rule_is_start_not_lower_no_double_underscore_and_no_underscore_beside_a_letter(
    ) {
        for bad in [
            "lower",
            "Bad__Name",
            "Snake_Case",
            "snake_case",
            "Point_2D",
            "Ab_c",
        ] {
            assert_eq!(found(&format!("pub struct {bad};")).len(), 1, "{bad}");
        }
        for ok in ["Point2D", "HTTPServer", "A", "_Leading", "Trailing_", "X1"] {
            assert!(found(&format!("pub struct {ok};")).is_empty(), "{ok}");
        }
    }

    #[test]
    fn the_snake_and_screaming_rules_are_no_upper_case_and_no_lower_case_letter() {
        for bad in ["camelCase", "Pascal", "x1Y"] {
            assert_eq!(found(&format!("pub fn {bad}() {{}}")).len(), 1, "{bad}");
        }
        for ok in ["snake_case", "x1", "_private", "__dunder__", "r#type"] {
            assert!(found(&format!("pub fn {ok}() {{}}")).is_empty(), "{ok}");
        }
        for bad in ["Max", "max_size", "MAX_size"] {
            assert_eq!(
                found(&format!("pub const {bad}: u8 = 1;")).len(),
                1,
                "{bad}"
            );
            assert_eq!(
                found(&format!("pub static {bad}: u8 = 1;")).len(),
                1,
                "{bad}"
            );
        }
        for ok in ["MAX", "MAX_SIZE", "_MAX", "M1"] {
            assert!(
                found(&format!("pub const {ok}: u8 = 1;")).is_empty(),
                "{ok}"
            );
        }
    }

    #[test]
    fn no_mangle_export_name_extern_blocks_trait_impl_methods_and_macro_bodies_are_not_read() {
        for src in [
            "#[no_mangle]\npub extern \"C\" fn CExport() {}",
            "#[export_name = \"x\"]\npub fn CExport2() {}",
            "#[unsafe(no_mangle)]\npub extern \"C\" fn CExport3() {}",
            "extern \"C\" { fn CImport(); static Cstat: u8; }",
            "pub struct S;\nimpl std::fmt::Display for S { fn fmt(&self, Fm: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }",
            "macro_rules! m { () => { pub fn BadInMacro() {} } }",
            "my_macro! { pub fn BadInInvocation() {} }",
        ] {
            assert!(found(src).is_empty(), "{src}: {:?}", kinds(src));
        }
    }

    #[test]
    fn an_allow_of_the_matching_lint_or_nonstandard_style_clears_an_item_and_another_lint_does_not()
    {
        assert!(found("#[allow(non_camel_case_types)]\npub struct lower;").is_empty());
        assert!(found("#[allow(nonstandard_style)]\npub struct lower;").is_empty());
        assert_eq!(
            found("#[allow(non_snake_case)]\npub struct lower;").len(),
            1,
            "another lint's allow does not clear a type name"
        );
        assert!(found("#[allow(non_upper_case_globals)]\npub const low: u8 = 1;").is_empty());
    }

    #[test]
    fn a_finding_is_low_severity_high_confidence_at_the_name_and_an_unparsable_file_is_noted() {
        let f = found("\npub fn BadName() {}\n");
        assert_eq!(
            (f[0].severity, f[0].confidence),
            (Severity::Low, Confidence::High)
        );
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        // one file that parses and one that does not: the bad one is named
        for files in [
            vec![
                ("Cargo.toml", MANIFEST),
                ("src/lib.rs", "pub fn ok() {}"),
                ("src/bad.rs", "pub fn ("),
            ],
            // the only source file does not parse: still a note naming it, not "no Rust source"
            vec![("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn (")],
        ] {
            match judge(&RepoView::from_files(&files)) {
                Verdict::Noted(_, notes) => assert!(
                    notes.iter().any(|n| n.contains("could not be parsed")),
                    "{notes:?}"
                ),
                _ => panic!("an unparsable file is a note, not a clean read: {files:?}"),
            }
        }
    }
}
