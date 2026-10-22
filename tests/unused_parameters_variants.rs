// SPDX-License-Identifier: MIT OR Apache-2.0
//! Planted variants for `unused-parameters` (CodeRipper's own self-check, decision 180). `tests/unused_parameters.rs` and the
//! `conformance/unused-parameters` fixture seed ONE spelling of each shape; the cases here plant the siblings the check's
//! documentation says it judges (or clean twins of them) and state the verdict it must give. A case that failed when it was
//! written is named in the CHANGELOG with its fix.

mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::UnusedParametersCheck;
#[path = "common/manifest.rs"]
mod manifest;
use common::git_repo_with;
use manifest::MANIFEST;

fn subjects(files: &[(&str, &str)]) -> Vec<String> {
    let repo = git_repo_with(files);
    let findings = UnusedParametersCheck::new()
        .run(&CheckContext::new(repo.path().to_path_buf()))
        .unwrap();
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

fn lib(src: &str) -> Vec<String> {
    subjects(&[("Cargo.toml", MANIFEST), ("src/lib.rs", src)])
}

#[test]
fn every_spelling_of_a_function_item_is_reported() {
    let src = "\
pub async fn is_async(a: i32) {}
pub const fn is_const(b: i32) {}
pub unsafe fn is_unsafe(c: i32) {}
pub(crate) fn restricted(d: i32) {}
pub fn generic<T>(e: T) {}
pub fn bounded<T: Clone>(f: T) where T: Send {}
pub fn opaque(g: impl std::fmt::Display) {}
pub fn by_ref(ref h: i32) {}
pub fn by_struct(S { x, y }: S) -> i32 { x }
pub struct S { pub x: i32, pub y: i32 }
";
    assert_eq!(
        lib(src),
        vec![
            "bounded::f",
            "by_ref::h",
            "by_struct::y",
            "generic::e",
            "is_async::a",
            "is_const::b",
            "is_unsafe::c",
            "opaque::g",
            "restricted::d",
        ]
    );
}

#[test]
fn every_spelling_of_an_inherent_impl_is_reported() {
    let src = "\
pub struct W<T>(pub T);
impl<T> W<T> {
    pub fn generic_impl(&self, a: i32) {}
    pub fn boxed(self: Box<Self>, b: i32) {}
    pub async fn is_async(&self, c: i32) {}
    pub fn assoc(d: i32) {}
}
pub struct R<'a>(pub &'a str);
impl<'a> R<'a> {
    pub fn lifetime(&self, e: i32) {}
}
impl W<u8> {
    pub fn specific(&self, f: i32) {}
}
";
    assert_eq!(
        lib(src),
        vec![
            "R::lifetime::e",
            "W::assoc::d",
            "W::boxed::b",
            "W::generic_impl::a",
            "W::is_async::c",
            "W::specific::f",
        ]
    );
}

#[test]
fn nested_and_conditionally_compiled_items_are_reported_where_they_build() {
    let src = "\
pub mod inline { pub fn in_mod(a: i32) {} }
pub mod file_mod;
pub fn outer() {
    fn in_fn(b: i32) {}
    struct L;
    impl L { fn in_impl_in_fn(&self, c: i32) {} }
    in_fn(1);
    L.in_impl_in_fn(1);
}
#[cfg(test)]
mod tests {
    fn helper(d: i32) {}
    #[test]
    fn t() { helper(1); }
}
#[cfg(feature = \"never_enabled\")]
pub fn gated_off(e: i32) {}
";
    let got = subjects(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", src),
        ("src/file_mod.rs", "pub fn in_file(f: i32) {}\n"),
    ]);
    // `gated_off` is not compiled, so rustc says nothing about it and neither does the check.
    assert_eq!(
        got,
        vec![
            "in_file::f",
            "inline::in_mod::a",
            "outer::L::in_impl_in_fn::c",
            "outer::in_fn::b",
            "tests::helper::d",
        ]
    );
}

#[test]
fn a_parameter_that_is_shadowed_before_any_read_is_still_unused() {
    let src = "pub fn shadow(a: i32) -> i32 { let a = 2; a }\n";
    assert_eq!(lib(src), vec!["shadow::a"]);
}

#[test]
fn every_way_of_reading_a_parameter_is_clean() {
    let src = "\
pub fn in_macro(a: i32) { println!(\"{}\", a); }
pub fn in_closure(b: i32) -> impl Fn() -> i32 { move || b }
pub fn in_return(c: i32) -> i32 { return c; }
pub fn by_discard(d: i32) { let _ = d; }
pub fn in_struct(e: i32) -> (i32,) { (e,) }
pub fn in_fmt_inline(f: i32) -> String { format!(\"{f}\") }
pub fn in_match(g: Option<i32>) -> i32 { match g { Some(v) => v, None => 0 } }
pub struct S;
impl S {
    pub fn used(&self, h: i32) -> i32 { h }
}
";
    assert_eq!(lib(src), Vec::<String>::new());
}

#[test]
fn an_explicit_allow_or_expect_at_any_enclosing_level_is_respected() {
    let src = "\
#[allow(unused_variables)]
pub mod allowed_mod { pub fn a(x: i32) {} }
pub struct S;
#[allow(unused_variables)]
impl S { pub fn b(&self, y: i32) {} }
#[allow(unused)]
pub fn c(z: i32) {}
#[expect(unused_variables)]
pub fn d(w: i32) {}
";
    assert_eq!(lib(src), Vec::<String>::new());
}

#[test]
fn trait_impl_shapes_are_left_alone_in_every_spelling() {
    let src = "\
pub trait T { fn m(&self, a: i32); fn d(&self, b: i32) {} }
pub struct S;
impl T for S { fn m(&self, a: i32) {} }
impl<X> T for Vec<X> { fn m(&self, a: i32) {} }
impl T for &S { fn m(&self, a: i32) {} }
impl T for [u8; 2] { fn m(&self, a: i32) {} }
impl std::str::FromStr for S {
    type Err = ();
    fn from_str(s: &str) -> Result<S, ()> { Ok(S) }
}
";
    assert_eq!(lib(src), Vec::<String>::new());
}

#[test]
fn only_src_is_analyzed_so_a_parameter_in_tests_or_examples_is_not_reported() {
    // Today's scope, pinned so that widening it is a deliberate change: the parameter sites are read from `src/` only (the
    // same boundary `unused-return-values` documents), so rustc's report on `tests/it.rs` and `examples/ex.rs` finds no
    // site to join. If this check starts reading those directories, this case is the one to change.
    let got = subjects(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f(a: i32) -> i32 { a }\n"),
        (
            "tests/it.rs",
            "fn helper(t: i32) {}\n#[test]\nfn it() { helper(1); }\n",
        ),
        (
            "examples/ex.rs",
            "fn helper(u: i32) {}\nfn main() { helper(1); }\n",
        ),
        (
            "src/main.rs",
            "fn helper(m: i32) {}\nfn main() { helper(1); }\n",
        ),
    ]);
    assert_eq!(got, vec!["helper::m"]);
}
