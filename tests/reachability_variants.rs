//! Planted variants for `reachability` (CodeRipper's own self-check, decision 180). `tests/reachability.rs` and the
//! `conformance/reachability` fixture seed ONE spelling of a dead `pub` function; the cases here plant its siblings (the
//! other item kinds and function qualifiers a `pub` item can be written with, and the ways an item can be live) and state the
//! verdict the check must give. A case that failed when it was written is named in the CHANGELOG with its fix.
//!
//! Every case is a library crate (`src/lib.rs`): that is where the `pub` exemption this check exists to see through applies.

mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::reachability::ReachabilityCheck;
#[path = "common/manifest.rs"]
mod manifest;
use common::git_repo_with;
use manifest::MANIFEST;

/// The symbols the check reports for a library whose root is `lib_rs`, sorted.
fn reported(lib_rs: &str) -> Vec<String> {
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", lib_rs)]);
    let findings = ReachabilityCheck::new()
        .run(&CheckContext::new(repo.path().to_path_buf()))
        .unwrap_or_else(|e| panic!("the check could not run: {e:#}"));
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

#[test]
fn a_dead_pub_function_is_reported_in_every_qualifier_spelling() {
    let src = "\
pub fn plain() {}
pub const fn is_const() -> i32 { 1 }
pub async fn is_async() {}
pub unsafe fn is_unsafe() {}
pub extern \"C\" fn is_extern() {}
pub unsafe extern \"C\" fn is_unsafe_extern() {}
pub const unsafe fn is_const_unsafe() -> i32 { 1 }
pub fn generic<T: Default>() -> T { T::default() }
pub fn with_where<T>() where T: Default {}
";
    assert_eq!(
        reported(src),
        vec![
            "generic",
            "is_async",
            "is_const",
            "is_const_unsafe",
            "is_extern",
            "is_unsafe",
            "is_unsafe_extern",
            "plain",
            "with_where",
        ]
    );
}

#[test]
fn a_dead_pub_item_is_reported_for_every_item_kind() {
    let src = "\
pub struct UnitStruct;
pub struct TupleStruct(i32);
pub struct BraceStruct { a: i32 }
pub enum DeadEnum { A, B }
pub type DeadAlias = i32;
pub const DEAD_CONST: i32 = 1;
pub static DEAD_STATIC: i32 = 2;
pub trait DeadTrait { fn m(&self); }
pub mod dead_mod {
    pub fn inside() {}
}
";
    let got = reported(src);
    for want in [
        "UnitStruct",
        "TupleStruct",
        "BraceStruct",
        "DeadEnum",
        "DeadAlias",
        "DEAD_CONST",
        "DEAD_STATIC",
        "DeadTrait",
        "inside",
    ] {
        assert!(
            got.iter().any(|s| s == want),
            "{want} not reported: {got:?}"
        );
    }
}

#[test]
fn a_dead_pub_union_is_reported() {
    let src = "pub union DeadUnion { a: u32, b: f32 }\n";
    assert_eq!(reported(src), vec!["DeadUnion"]);
}

#[test]
fn a_dead_pub_method_in_an_impl_is_reported() {
    let src = "\
pub(crate) struct S;
impl S {
    pub fn dead_method(&self) {}
    pub fn live_method(&self) {}
}
#[no_mangle]
pub extern \"C\" fn anchor() { S.live_method(); }
";
    assert_eq!(reported(src), vec!["dead_method"]);
}

#[test]
fn a_dead_pub_item_declared_on_one_line_inside_a_module_is_reported() {
    // The inner `pub` does not start its line, so it is not downgraded itself; it is reported because the enclosing
    // `pub mod` is, which makes everything in it crate-visible.
    let src = "pub mod m { pub fn dead_in_one_line() {} }\n";
    let got = reported(src);
    assert!(got.contains(&"dead_in_one_line".to_string()), "{got:?}");
}

#[test]
fn a_pub_item_behind_an_attribute_on_the_same_line_is_reported() {
    let src = "#[derive(Debug)] pub struct SameLineAttr;\n#[inline] pub fn same_line_fn() {}\n";
    let got = reported(src);
    assert!(got.contains(&"SameLineAttr".to_string()), "{got:?}");
    assert!(got.contains(&"same_line_fn".to_string()), "{got:?}");
}

#[test]
fn a_pub_item_written_with_extra_whitespace_is_reported() {
    let src = "pub  fn two_spaces() {}\npub\tfn tab() {}\n";
    assert_eq!(reported(src), vec!["tab", "two_spaces"]);
}

#[test]
fn every_way_an_item_is_live_is_clean() {
    let src = "\
pub(crate) mod inner {
    pub fn via_path() {}
    pub fn via_rename() {}
    pub fn via_glob() {}
    pub const VIA_CONST_EXPR: usize = 3;
    pub struct ViaTypePosition;
    pub trait ViaImplBound { fn go(&self) {} }
}
use inner::via_rename as renamed;
use inner::*;
#[no_mangle]
pub extern \"C\" fn anchor() -> usize {
    inner::via_path();
    renamed();
    via_glob();
    let _a = [0u8; inner::VIA_CONST_EXPR];
    let _t: Option<inner::ViaTypePosition> = None;
    fn bound<T: inner::ViaImplBound>(t: &T) { t.go() }
    struct W;
    impl inner::ViaImplBound for W {}
    bound(&W);
    0
}
";
    assert_eq!(reported(src), Vec::<String>::new());
}

#[test]
fn an_item_the_compiler_keeps_for_the_linker_or_the_author_is_clean() {
    let src = "\
#[no_mangle]
pub extern \"C\" fn exported_symbol() {}
#[allow(dead_code)]
pub fn waived_by_the_author() {}
#[used]
pub static KEPT: u8 = 1;
#[cfg(feature = \"absent\")]
pub fn not_compiled() {}
";
    assert_eq!(reported(src), Vec::<String>::new());
}

#[test]
fn a_second_item_on_one_line_keeps_the_pub_exemption_a_documented_limit() {
    // Pinned, not endorsed: the downgrade is line-based (see the module docs' known limitations).
    let src = "pub fn first_on_line() {} pub fn second_on_line() {}\n";
    assert_eq!(reported(src), vec!["first_on_line"]);
}
