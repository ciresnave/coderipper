// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::checks::rust_sources::SourceFile;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn totals(files: &[SourceFile]) -> Totals {
    surface(files, "", &BTreeSet::new()).unwrap().0
}

fn lib(src: &str) -> Totals {
    totals(&[file("src/lib.rs", src)])
}

#[test]
fn documented_and_undocumented_public_items_are_told_apart() {
    let t = lib(
        "/// Adds.\npub fn add() {}\npub fn sub() {}\n/// A thing.\npub struct S;\npub struct T;\n#[doc = \"a const\"]\npub const K: u8 = 1;\nfn private() {}\n",
    );
    assert_eq!(t.documented, 3);
    let names: Vec<&str> = t.undocumented.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["sub", "T"]);
    assert_eq!(t.undocumented[0].kind, "fn");
    assert_eq!(t.undocumented[0].line, 3);
}

#[test]
fn doc_hidden_items_are_in_neither_list_and_private_items_are_not_public() {
    let t = lib("#[doc(hidden)]\npub fn internal() {}\npub(crate) fn also() {}\nfn private() {}\n");
    assert_eq!(t.documented, 0);
    assert!(t.undocumented.is_empty(), "{:?}", t.undocumented);
    // positive control: the same function without the attribute is undocumented
    let t = lib("pub fn internal() {}\n");
    assert_eq!(t.undocumented.len(), 1);
}

#[test]
fn public_inherent_functions_of_public_types_count_and_private_types_do_not() {
    let t = lib(
        "/// S.\npub struct S;\nimpl S {\n    /// Documented.\n    pub fn a(&self) {}\n    pub fn b(&self) {}\n    fn c(&self) {}\n}\nstruct Hidden;\nimpl Hidden { pub fn d(&self) {} }\n",
    );
    let names: Vec<&str> = t.undocumented.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["S::b"]);
    assert_eq!(t.undocumented[0].kind, "method");
    // S itself and S::a are documented
    assert_eq!(t.documented, 2);
}

#[test]
fn an_item_in_a_private_module_is_not_part_of_the_surface() {
    let t = totals(&[
        file("src/lib.rs", "mod inner;\npub mod open;\n"),
        file("src/inner.rs", "pub fn hidden() {}\n"),
        file("src/open.rs", "pub fn shown() {}\n"),
    ]);
    let names: Vec<&str> = t.undocumented.iter().map(|u| u.name.as_str()).collect();
    // the public module itself and its function; not the function of the private module
    assert_eq!(names, ["open", "shown"]);
    assert_eq!(t.undocumented[1].file, "src/open.rs");
}

#[test]
fn a_fully_documented_surface_has_no_finding_and_a_gap_is_info_below_the_share() {
    let clean = lib("/// A.\npub fn a() {}\n/// B.\npub fn b() {}\n");
    assert!(judge(&clean, &PARAMS, "p").is_empty());
    // one gap among ten is Info
    let mut src = String::new();
    for i in 0..9 {
        src.push_str(&format!("/// F{i}.\npub fn f{i}() {{}}\n"));
    }
    src.push_str("pub fn gap() {}\n");
    let f = judge(&lib(&src), &PARAMS, "p");
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert!(
        f[0].summary.starts_with("1 of 10 public items"),
        "{}",
        f[0].summary
    );
    assert!(f[0].summary.contains("(10%)"), "{}", f[0].summary);
}

#[test]
fn most_of_a_large_surface_undocumented_is_low_and_a_small_one_never_is() {
    let mut big = String::new();
    for i in 0..30 {
        big.push_str(&format!("pub fn f{i}() {{}}\n"));
    }
    let f = judge(&lib(&big), &PARAMS, "p");
    assert_eq!(f[0].severity, Severity::Low);
    // three items, all undocumented, is below the minimum size
    let f = judge(
        &lib("pub fn a() {}\npub fn b() {}\npub fn c() {}\n"),
        &PARAMS,
        "p",
    );
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn the_finding_states_both_counts_and_the_first_places() {
    let t = lib("/// S.\npub struct S { pub a: u8, pub b: u8 }\nimpl S { pub fn go(&self) {} }\npub fn free() {}\n");
    let f = judge(&t, &PARAMS, "pkg");
    assert_eq!(f.len(), 1);
    // surface: 2 declarations (S, free), members: 2 fields + 1 inherent function
    assert!(
        f[0].detail.contains("The public surface is 2 item(s)"),
        "{}",
        f[0].detail
    );
    assert!(f[0].detail.contains("3 member(s)"), "{}", f[0].detail);
    assert!(
        f[0].detail.contains("`S::go` (method at src/lib.rs:3)"),
        "{}",
        f[0].detail
    );
    assert_eq!(f[0].subject.as_deref(), Some("pkg"));
    assert_eq!(f[0].location.as_ref().unwrap().line, Some(3));
}

#[test]
fn a_binary_only_package_has_no_surface_and_no_finding() {
    let (t, notes) = surface(
        &[file("src/main.rs", "pub fn f() {}\n")],
        "",
        &BTreeSet::new(),
    )
    .unwrap();
    assert!(judge(&t, &PARAMS, "p").is_empty());
    assert!(notes[0].contains("binary crate"), "{notes:?}");
}
