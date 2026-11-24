// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn members() -> BTreeSet<String> {
    ["sibling".to_string()].into()
}

fn run(files: &[SourceFile]) -> (Totals, Vec<String>) {
    surface(files, "", &members()).unwrap()
}

#[test]
fn public_items_are_counted_by_kind_and_private_ones_are_not() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub fn a() {}\nfn b() {}\npub(crate) fn c() {}\npub struct S;\npub enum E { X, Y }\npub trait T { fn t(&self); }\npub type A = u8;\npub const K: u8 = 1;\npub static G: u8 = 2;\n#[macro_export]\nmacro_rules! m { () => {} }\nmacro_rules! hidden { () => {} }\n",
    )]);
    assert_eq!(t.item_count(), 8, "{t:?}");
    assert_eq!(t.items.get("fn"), Some(&1));
    assert_eq!(t.items.get("macro"), Some(&1));
}

#[test]
fn members_are_pub_fields_variants_trait_items_and_inherent_pub_methods() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub struct S { pub a: u8, b: u8, pub c: u8 }\nimpl S { pub fn new() -> Self { todo!() } fn private(&self) {} pub const Z: u8 = 0; }\nimpl Default for S { fn default() -> Self { todo!() } }\npub enum E { X, Y, Z }\npub trait T { fn one(&self); fn two(&self); }\n",
    )]);
    // S: 2 fields + 2 pub associated items; E: 3 variants; T: 2 items
    assert_eq!(t.members, 9, "{t:?}");
    // positive control: the items themselves are the three declarations
    assert_eq!(t.item_count(), 3);
}

#[test]
fn an_item_in_a_private_module_is_hidden_unless_it_is_re_exported() {
    let files = [
        file("src/lib.rs", "mod imp;\npub mod open;\npub use imp::Shown;\n"),
        file("src/imp.rs", "pub struct Shown { pub x: u8 }\npub struct Hidden { pub y: u8 }\nimpl Shown { pub fn go(&self) {} }\n"),
        file("src/open.rs", "pub fn visible() {}\n"),
    ];
    let (t, notes) = run(&files);
    // items: `open` (mod), `visible`, and the re-export `Shown`; members: Shown's field and method; Hidden is not public
    assert_eq!(t.item_count(), 3, "{t:?}");
    assert_eq!(t.members, 2, "{t:?}");
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_pub_use_that_repeats_a_declared_name_is_not_counted_twice() {
    let files = [
        file("src/lib.rs", "pub mod a;\npub use a::Thing;\n"),
        file("src/a.rs", "pub struct Thing;\n"),
    ];
    let (t, _) = run(&files);
    assert_eq!(t.item_count(), 2, "the module and the struct: {t:?}");
}

#[test]
fn test_items_and_test_modules_are_not_counted() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub fn real() {}\n#[cfg(test)]\nmod tests { pub fn helper() {} }\n#[cfg(test)]\npub fn only_in_tests() {}\n",
    )]);
    assert_eq!(t.item_count(), 1, "{t:?}");
}

#[test]
fn a_re_export_of_a_library_is_third_party_and_one_of_our_own_is_not() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub use serde::Serialize;\npub use ::bytes::Bytes;\npub use sibling::Thing;\npub use std::collections::HashMap;\npub use crate::inner::X;\npub use local::Y;\npub mod local { pub struct Y; }\nmod inner { pub struct X; }\npub use serde_json::{Value, Map as M};\n",
    )]);
    let paths: Vec<&str> = t.third_party.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "::bytes::Bytes",
            "serde::Serialize",
            "serde_json::Map",
            "serde_json::Value"
        ],
        "{t:?}"
    );
}

#[test]
fn a_glob_re_export_is_one_item_and_is_counted_as_a_glob() {
    let (t, _) = run(&[file("src/lib.rs", "pub use other::*;\n")]);
    assert_eq!(t.items.get("re-export"), Some(&1));
    assert_eq!(t.globs, 1);
}

#[test]
fn a_binary_crate_and_a_missing_module_file_are_noted() {
    let (t, notes) = run(&[file("src/main.rs", "pub fn f() {}\n")]);
    assert_eq!(t.item_count(), 0);
    assert!(notes[0].contains("binary crate"), "{notes:?}");
    let (_, notes) = run(&[file("src/lib.rs", "pub mod gone;\n")]);
    assert!(notes[0].contains("gone"), "{notes:?}");
    // positive control: with the file present there is no note
    let (_, notes) = run(&[
        file("src/lib.rs", "pub mod gone;\n"),
        file("src/gone.rs", "pub fn f() {}\n"),
    ]);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn an_unparsable_file_is_an_error_never_a_smaller_count() {
    let r = surface(&[file("src/lib.rs", "pub fn (")], "", &members());
    assert!(r.is_err());
}

fn counted(name: &str, budget: u32, items: usize, members: usize) -> Tally {
    let mut totals = Totals::default();
    totals.items.insert("fn", items);
    totals.members = members;
    Tally::Counted {
        component: name.into(),
        budget,
        manifest: Some(format!("{name}/Cargo.toml")),
        totals: Box::new(totals),
    }
}

#[test]
fn over_budget_is_one_low_finding_and_within_budget_is_none() {
    let (f, notes) = judge(
        vec![counted("domain", 10, 8, 5), counted("util", 20, 8, 5)],
        "p",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
    assert_eq!(f[0].subject.as_deref(), Some("domain"));
    assert!(f[0].summary.contains("13") && f[0].summary.contains("10"));
    assert!(
        notes[0].contains("domain 13/10") && notes[0].contains("util 13/20"),
        "{notes:?}"
    );
    // exactly at the budget is within it
    let (f, _) = judge(vec![counted("domain", 13, 8, 5)], "p", vec![]);
    assert!(f.is_empty());
}

#[test]
fn third_party_re_exports_are_an_info_finding_with_their_own_subject() {
    let mut totals = Totals::default();
    totals
        .third_party
        .push(("serde::Serialize".into(), "d/src/lib.rs".into()));
    let (f, _) = judge(
        vec![Tally::Counted {
            component: "domain".into(),
            budget: 100,
            manifest: None,
            totals: Box::new(totals),
        }],
        "p",
        vec![],
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].subject.as_deref(), Some("domain:third-party"));
}

#[test]
fn a_budget_for_a_component_that_does_not_exist_is_reported() {
    let (f, _) = judge(vec![Tally::Stale("ghost".into())], "p", vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert!(f[0].summary.contains("ghost"));
}

#[test]
fn not_run_names_what_to_add() {
    assert!(missing_declaration(None)
        .unwrap()
        .contains("no [architecture] table"));
    let a: ArchitectureConfig = toml::from_str("").unwrap();
    assert!(missing_declaration(Some(&a))
        .unwrap()
        .contains("api_budget"));
    let a: ArchitectureConfig = toml::from_str("[api_budget]\ndomain = 5\n").unwrap();
    assert!(missing_declaration(Some(&a)).is_none());
}

#[test]
fn a_stated_none_lets_the_rule_run_with_nothing_to_compare() {
    let parse = |t: &str| {
        crate::config::ConfigFile::parse(t, std::path::Path::new(".coderipper.toml"))
            .unwrap()
            .architecture
            .unwrap()
    };
    // the same table without the statement is a gap, with it the rule runs
    assert!(missing_declaration(Some(&parse("[architecture]\n"))).is_some());
    let stated = parse("[architecture]\nnone = [\"api_budget\"]\n");
    assert!(missing_declaration(Some(&stated)).is_none());
    // a statement about another input does not excuse this one
    let other = parse("[architecture]\nnone = [\"owns_data\"]\n");
    assert!(missing_declaration(Some(&other)).is_some());
    // and no declaration at all is still a gap
    assert!(missing_declaration(None).is_some());
}

#[test]
fn extern_block_symbols_are_kept_apart_from_the_budget() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub fn a() {}\nextern \"C\" {\n    /// Doc.\n    pub fn open();\n    pub fn close();\n    fn private();\n}\n",
    )]);
    let lines = |s: &BTreeSet<(String, usize)>| s.iter().map(|(_, l)| *l).collect::<Vec<_>>();
    assert_eq!(lines(&t.foreign_sites), [4, 5], "{t:?}");
    assert_eq!(lines(&t.foreign_documented_sites), [4], "{t:?}");
    // the budget does not change: only `a` is counted
    assert_eq!(t.item_count(), 1, "{t:?}");
}

// Claims audit (2026-11-17): the claims of the MOD-011 extern-block change (changelog.d/mod-011-extern-symbols.md).
#[test]
fn extern_block_functions_statics_and_types_are_all_recorded_when_pub() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "extern \"C\" {\n    pub fn open();\n    pub static VERSION: i32;\n    pub type Handle;\n    fn private();\n    static PRIVATE: i32;\n}\n",
    )]);
    let lines: Vec<usize> = t.foreign_sites.iter().map(|(_, l)| *l).collect();
    assert_eq!(lines, [2, 3, 4], "{t:?}");
}

#[test]
fn an_extern_block_in_a_module_not_reachable_through_pub_modules_is_not_recorded() {
    let (t, _) = run(&[
        file("src/lib.rs", "mod hidden;\npub mod open;\n"),
        file(
            "src/hidden.rs",
            "extern \"C\" {\n    pub fn not_reachable();\n}\n",
        ),
        file(
            "src/open.rs",
            "extern \"C\" {\n    pub fn reachable();\n}\n",
        ),
    ]);
    let sites: Vec<(&str, usize)> = t
        .foreign_sites
        .iter()
        .map(|(f, l)| (f.as_str(), *l))
        .collect();
    assert_eq!(sites, [("src/open.rs", 2)], "{t:?}");
}

#[test]
fn a_doc_hidden_extern_symbol_is_not_recorded() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "extern \"C\" {\n    #[doc(hidden)]\n    pub fn internal();\n    pub fn shown();\n}\n",
    )]);
    let lines: Vec<usize> = t.foreign_sites.iter().map(|(_, l)| *l).collect();
    assert_eq!(lines, [4], "{t:?}");
}

#[test]
fn the_foreign_sets_stay_apart_from_the_budget_counts_for_every_symbol_kind() {
    let (with, _) = run(&[file(
        "src/lib.rs",
        "pub fn a() {}\nextern \"C\" {\n    pub fn open();\n    pub static VERSION: i32;\n    pub type Handle;\n}\n",
    )]);
    let (without, _) = run(&[file("src/lib.rs", "pub fn a() {}\n")]);
    assert_eq!(with.item_count(), without.item_count(), "{with:?}");
    assert_eq!(with.public_sites, without.public_sites);
    assert_eq!(with.documented, without.documented);
}

// MOD-011 on associated consts (baracuda -sys status codes): an associated const of a public type is interface like its methods.
fn lines_of(s: &BTreeSet<(String, usize)>) -> Vec<usize> {
    s.iter().map(|(_, l)| *l).collect()
}

#[test]
fn a_pub_associated_const_of_a_public_type_is_recorded_apart_from_the_budget() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "pub mod status {
    pub struct S(pub i32);
    impl S {
        /// Doc.
        pub const OK: Self = Self(0);
        pub const BAD: Self = Self(1);
        const PRIVATE: Self = Self(2);
    }
}
",
    )]);
    assert_eq!(lines_of(&t.const_sites), [5, 6], "{t:?}");
    assert_eq!(lines_of(&t.const_documented_sites), [5], "{t:?}");
    // the budget does not change: the module and the type are the two declarations, and the consts are not added to the sites it counts
    assert_eq!(t.item_count(), 2, "{t:?}");
    assert!(
        !t.public_sites.iter().any(|(_, l)| *l == 5 || *l == 6),
        "{t:?}"
    );
}

#[test]
fn an_associated_const_of_a_type_in_a_private_module_nothing_re_exports_is_not_recorded() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "mod inner {
    pub struct S;
    impl S {
        /// Doc.
        pub const OK: i32 = 0;
    }
}
",
    )]);
    assert!(
        t.const_sites.is_empty() && t.reexport_sites.is_empty(),
        "{t:?}"
    );
}

#[test]
fn an_associated_const_of_a_type_a_pub_use_names_out_of_a_private_module_is_re_exported() {
    let (t, _) = run(&[file(
        "src/lib.rs",
        "mod inner {
    pub struct S;
    impl S {
        /// Doc.
        pub const OK: i32 = 0;
        pub const BAD: i32 = 1;
    }
}
pub use inner::S;
",
    )]);
    assert!(lines_of(&t.reexport_sites).contains(&5), "{t:?}");
    assert!(lines_of(&t.reexport_sites).contains(&6), "{t:?}");
    assert_eq!(
        lines_of(&t.reexport_documented_sites)
            .iter()
            .filter(|l| **l == 5 || **l == 6)
            .count(),
        1,
        "{t:?}"
    );
}
