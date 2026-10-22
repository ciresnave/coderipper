// SPDX-License-Identifier: MIT OR Apache-2.0
//! Planted variants for `unused-return-values` (CodeRipper's own self-check, decision 180). `tests/unused_return_values.rs` and
//! the `conformance/unused-return-values` fixture seed ONE spelling of each shape; the cases here plant the siblings the
//! check's documentation says it judges (or clean twins of them) and state the verdict it must give. A case that failed when
//! it was written is named in the CHANGELOG with its fix.

mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::UnusedReturnValuesCheck;
#[path = "common/manifest.rs"]
mod manifest;
use common::git_repo_with;
use manifest::MANIFEST;

/// The functions the check reports, by the name in each finding's summary, over a crate whose root is `src/main.rs`.
fn reported(main_rs: &str) -> Vec<String> {
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", main_rs)]);
    let findings = UnusedReturnValuesCheck::new()
        .run(&CheckContext::new(repo.path().to_path_buf()))
        .unwrap();
    let mut v: Vec<String> = findings
        .iter()
        .map(|f| f.summary.split('`').nth(1).unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn a_value_every_caller_discards_is_reported_in_every_function_spelling() {
    let src = "\
struct S;
impl S {
    fn make() -> i32 { 1 }
    fn method(&self) -> i32 { 2 }
}
mod inner { pub fn in_mod() -> i32 { 3 } }
fn generic<T: Default>() -> T { T::default() }
const fn is_const() -> i32 { 4 }
unsafe fn is_unsafe() -> i32 { 5 }
pub(crate) fn restricted() -> i32 { 6 }
fn returns_tuple() -> (i32, i32) { (1, 2) }
fn returns_string() -> String { String::new() }
fn main() {
    S::make();
    S.method();
    inner::in_mod();
    generic::<i32>();
    is_const();
    unsafe { is_unsafe(); }
    restricted();
    returns_tuple();
    returns_string();
}
";
    assert_eq!(
        reported(src),
        vec![
            "generic",
            "in_mod",
            "is_const",
            "is_unsafe",
            "make",
            "method",
            "restricted",
            "returns_string",
            "returns_tuple",
        ]
    );
}

#[test]
fn a_discard_inside_a_closure_a_nested_fn_or_a_match_arm_is_still_a_discard() {
    let src = "\
fn in_closure() -> i32 { 1 }
fn in_nested() -> i32 { 2 }
fn in_arm() -> i32 { 3 }
fn main() {
    let c = || { in_closure(); };
    c();
    fn helper() { in_nested(); }
    helper();
    match 1 { 1 => { in_arm(); } _ => {} }
}
";
    assert_eq!(reported(src), vec!["in_arm", "in_closure", "in_nested"]);
}

#[test]
fn a_caller_defined_on_the_same_line_as_its_callee_still_discards_it() {
    // The recursion filter drops a site that lies inside the callee's own line range; a caller on the callee's line is not
    // recursion.
    let src = "fn callee() -> i32 { 1 } fn caller() { callee(); }\nfn main() { caller(); }\n";
    assert_eq!(reported(src), vec!["callee"]);
}

#[test]
fn every_way_of_consuming_a_value_is_clean() {
    let src = "\
fn bound() -> i32 { 1 }
fn tail() -> i32 { 2 }
fn arg() -> i32 { 3 }
fn pointer() -> i32 { 4 }
fn cond() -> i32 { 5 }
fn discarded_on_purpose() -> i32 { 6 }
fn question() -> Option<i32> { Some(1) }
fn user() -> Option<i32> { let q = question()?; Some(q) }
fn takes(x: i32) -> i32 { x }
fn through() -> i32 { tail() }
fn main() {
    let _b = bound();
    println!(\"{}\", through());
    let _t = takes(arg());
    let f: fn() -> i32 = pointer;
    let _ = f();
    if cond() > 0 {}
    let _ = discarded_on_purpose();
    let _ = user();
}
";
    assert_eq!(reported(src), Vec::<String>::new());
}

#[test]
fn a_function_that_returns_unit_or_diverges_is_never_reported() {
    let src = "\
fn unit() {}
fn explicit_unit() -> () {}
fn never() -> ! { std::process::exit(0) }
fn main() {
    unit();
    explicit_unit();
    never();
}
";
    assert_eq!(reported(src), Vec::<String>::new());
}
