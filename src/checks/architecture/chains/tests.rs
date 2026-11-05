// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(text: &str) -> SourceFile {
    SourceFile {
        path: "src/lib.rs".into(),
        text: text.into(),
    }
}

const SHAPES: &str = "
pub struct City { pub name: String }
pub struct Address { city: City }
impl Address { pub fn city(&self) -> &City { &self.city } }
pub struct Customer { address: Address }
impl Customer { pub fn address(&self) -> &Address { &self.address } }
pub struct Order { customer: Customer, items: Vec<u32> }
impl Order { pub fn customer(&self) -> &Customer { &self.customer } }
";

fn chains_of(body: &str) -> Vec<Chain> {
    let src = format!("{SHAPES}\n{body}");
    find_chains(&[file(&src)]).unwrap().0
}

#[test]
fn a_chain_of_accessors_through_three_structs_has_depth_three() {
    let c = chains_of("pub fn f(o: &Order) -> usize { o.customer().address().city().name.len() }");
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].depth, 3);
    assert_eq!(c[0].text, "customer().address().city()");
    assert_eq!(c[0].function, "f");
}

#[test]
fn fields_count_the_same_as_accessors() {
    let c = chains_of("pub fn f(o: &Order) -> usize { o.customer.address.city.name.len() }");
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].text, "customer.address.city");
}

#[test]
fn a_chain_that_does_not_connect_is_not_one_run() {
    // `address` then `customer` is not a path through the structs: Address has no `customer`
    let c = chains_of("pub fn f(a: &Address) -> usize { a.city().name.len() }");
    assert!(c.is_empty(), "one hop is not a chain: {c:?}");
    let c = chains_of("pub fn f(o: &Order) { o.customer().city() }");
    assert!(c.iter().all(|c| c.depth < 2), "{c:?}");
}

#[test]
fn a_builder_is_not_a_reach_through() {
    let c = chains_of(
        "pub struct B { n: u32 }
impl B { pub fn new() -> B { B { n: 0 } } pub fn a(self) -> B { self } pub fn b(self) -> B { self } pub fn c(self) -> B { self } pub fn d(self) -> B { self } }
pub fn f() -> B { B::new().a().b().c().d() }",
    );
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn an_iterator_pipeline_is_not_a_reach_through() {
    let c = chains_of(
        "pub fn f(v: Vec<u32>) -> Vec<u32> { v.iter().map(|x| x + 1).filter(|x| x % 2 == 0).map(|x| x * 3).collect() }",
    );
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn a_method_with_an_argument_ends_the_run() {
    let c = chains_of(
        "impl Customer { pub fn nth(&self, i: usize) -> &Address { &self.address } }
pub fn f(o: &Order) { o.customer().nth(1).city(); }",
    );
    assert!(c.iter().all(|c| c.depth < 3), "{c:?}");
}

#[test]
fn test_code_is_not_read() {
    let c = chains_of(
        "#[cfg(test)] mod t { use super::*; fn f(o: &Order) { o.customer().address().city(); } }",
    );
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn the_threshold_decides_what_is_reported_and_the_note_counts_the_rest() {
    let src = format!(
        "{SHAPES}\npub fn f(o: &Order) {{ o.customer().address(); o.customer().address().city(); }}"
    );
    let files = [file(&src)];
    let (low, notes) = judge(&files, 2, "p").unwrap();
    assert_eq!(low.len(), 2, "{low:?}");
    let (high, notes_high) = judge(&files, 3, "p").unwrap();
    assert_eq!(high.len(), 1);
    assert_eq!(
        high[0].subject.as_deref(),
        Some("customer().address().city()")
    );
    assert_eq!(high[0].severity, Severity::Info);
    assert_eq!(high[0].confidence, Confidence::Low);
    assert!(
        notes[0].contains("2 hops: 1") && notes[0].contains("3 hops: 1"),
        "{notes:?}"
    );
    assert!(
        notes_high[0].contains("1 reported at 3 hops or more"),
        "{notes_high:?}"
    );
}

#[test]
fn the_depth_is_read_from_the_repository_file() {
    assert_eq!(
        crate::config::mod009_min_hops("[mod009]\nmin_hops = 4\n").unwrap(),
        Some(4)
    );
    assert_eq!(crate::config::mod009_min_hops("").unwrap(), None);
    assert!(crate::config::mod009_min_hops("[mod009]\nmin_hops = 1\n").is_err());
}
