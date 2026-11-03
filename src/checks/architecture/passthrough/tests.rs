// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn wrappers(src: &str) -> Vec<Wrapper> {
    collect(&[file("src/a.rs", src)]).unwrap().0
}

fn names(w: &[Wrapper]) -> Vec<String> {
    w.iter()
        .map(|w| format!("{}->{}", w.name, w.callee))
        .collect()
}

#[test]
fn a_function_that_only_forwards_its_arguments_is_a_wrapper() {
    let w = wrappers("fn f(a: u8, b: u8) -> u8 { g(a, b) }\nfn g(a: u8, b: u8) -> u8 { a + b }\n");
    assert_eq!(names(&w), ["f->g"]);
    assert_eq!(w[0].line, 1);
}

#[test]
fn an_alias_method_and_a_free_call_with_a_field_are_wrappers_but_a_call_on_a_field_is_not() {
    let w = wrappers(
        "struct S { inner: T, x: u8 }
impl S {
    fn a(&self, v: u8) -> u8 { self.go(v) }
    fn b(&self, v: u8) -> u8 { helper(&self.x, v) }
    fn c(&mut self, v: u8) { self.inner.set(v); }
    fn d(&self, v: u8) -> u8 { self.inner.go(v) }
}
",
    );
    // `c` and `d` hide the field `inner`: information hiding, not a wrapper
    assert_eq!(names(&w), ["S::a->self.go", "S::b->helper"]);
}

#[test]
fn trivial_wrappers_around_the_call_are_looked_through() {
    let w = wrappers(
        "fn a(v: u8) -> Result<u8, E> { Ok(g(v)) }\nfn b(v: u8) -> Option<u8> { Some(g(v)) }\nfn c(v: u8) -> u8 { g(v).into() }\nasync fn d(v: u8) -> u8 { g(v).await }\nfn e(v: u8) -> Result<u8, E> { g(v)? ; unreachable!() }\nfn f(v: u8) -> u8 { return g(v); }\n",
    );
    // `e` has two statements: not a wrapper
    assert_eq!(names(&w), ["a->g", "b->g", "c->g", "d->g", "f->g"]);
}

#[test]
fn a_changed_reordered_added_or_dropped_argument_is_not_a_pass_through() {
    for src in [
        "fn f(a: u8, b: u8) -> u8 { g(b, a) }",
        "fn f(a: u8) -> u8 { g(a + 1) }",
        "fn f(a: u8) -> u8 { g(a, 0) }",
        "fn f(a: u8, b: u8) -> u8 { g(a) }",
        "fn f(a: &u8) -> u8 { g(&a) }",
        "fn f(a: u8) -> u8 { let x = a; g(x) }",
        "fn f(a: u8) -> u8 { g(a); h(a) }",
        "fn f((a, b): (u8, u8)) -> u8 { g(a, b) }",
    ] {
        assert!(wrappers(src).is_empty(), "{src}");
    }
    // positive control: the plain form of the first is one
    assert_eq!(wrappers("fn f(a: u8, b: u8) -> u8 { g(a, b) }").len(), 1);
}

#[test]
fn a_function_with_no_parameter_to_forward_is_delegation_not_a_wrapper() {
    assert!(wrappers("struct S{i:T}\nimpl S { fn len(&self) -> usize { self.i.len() } }\nfn now() -> u8 { clock() }\n").is_empty());
}

#[test]
fn trait_impl_methods_and_the_crate_root_facade_are_not_findings() {
    let src = "struct S{i:T}\nimpl Trait for S { fn go(&self, v: u8) -> u8 { self.i.go(v) } }\n";
    assert!(wrappers(src).is_empty());
    // a pub function in lib.rs is a facade; the same in another file, or private in lib.rs, is not
    let lib = collect(&[file(
        "src/lib.rs",
        "pub fn api(v: u8) -> u8 { inner::api(v) }\nfn private(v: u8) -> u8 { inner::api(v) }\n",
    )])
    .unwrap()
    .0;
    assert_eq!(names(&lib), ["private->inner::api"]);
    let other = wrappers("pub fn api(v: u8) -> u8 { inner::api(v) }");
    assert_eq!(names(&other), ["api->inner::api"]);
    assert!(other[0].public);
}

#[test]
fn test_items_are_not_read_and_functions_are_counted() {
    let (w, n) = collect(&[file(
        "src/a.rs",
        "fn a(v: u8) -> u8 { g(v) }\n#[cfg(test)]\nmod tests { fn t(v: u8) -> u8 { g(v) } }\n#[cfg(test)]\nfn u(v: u8) -> u8 { g(v) }\nstruct S;\nimpl S { fn m(&self, v: u8) -> u8 { self.0.m(v) } }\n",
    )])
    .unwrap();
    assert_eq!(names(&w), ["a->g"]);
    assert_eq!(n, 2);
}

#[test]
fn one_info_finding_per_wrapper_names_the_pair_and_the_reasons_to_keep_it() {
    let w = wrappers("pub fn f(a: u8) -> u8 { g(a) }");
    let f = judge(&w, "p");
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].confidence, Confidence::Low);
    assert_eq!(f[0].subject.as_deref(), Some("f"));
    assert!(
        f[0].summary
            .contains("`f` only passes its arguments on to `g`"),
        "{}",
        f[0].summary
    );
    assert!(
        f[0].detail.contains("(a `pub` item)")
            && f[0].detail.contains("a name that fixes a meaning")
    );
}

#[test]
fn an_unparsable_file_is_an_error_never_a_smaller_count() {
    assert!(collect(&[file("src/a.rs", "fn (")]).is_err());
}

#[test]
fn a_tuple_struct_or_variant_constructor_is_not_a_wrapper() {
    let w = wrappers(
        "struct W(Vec<u8>);
impl W { pub fn new(v: Vec<u8>) -> Self { Self(v) } }
enum E { A(u8) }
impl E { pub fn a(v: u8) -> Self { Self::A(v) } pub fn b(v: u8) -> Self { make(v) } }
fn c(v: u8) -> W2 { W2(v) }
",
    );
    // only the call of a function named in lower case is a wrapper
    assert_eq!(names(&w), ["E::b->make"]);
}
