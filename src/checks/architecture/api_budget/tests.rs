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
        totals,
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
            totals,
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
