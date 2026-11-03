// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn facts(files: &[SourceFile]) -> Facts {
    let mut f = collect(files).unwrap();
    f.test_impls.extend(cfg_test_impls(files).unwrap());
    f
}

fn single(src: &str) -> Vec<String> {
    let f = facts(&[file("src/lib.rs", src)]);
    single_impl_traits(&f, &BTreeSet::new())
        .into_iter()
        .map(|t| format!("{}<-{}", t.name, t.implementor))
        .collect()
}

#[test]
fn a_trait_with_one_implementation_and_no_double_is_a_shape() {
    assert_eq!(
        single("trait Store { fn get(&self); }\nstruct Disk;\nimpl Store for Disk { fn get(&self) {} }\n"),
        ["Store<-Disk"]
    );
}

#[test]
fn a_second_implementation_a_test_double_or_a_double_by_name_means_it_earns_its_place() {
    // two implementations
    assert!(single("trait S { fn g(&self); }\nstruct A;\nstruct B;\nimpl S for A { fn g(&self) {} }\nimpl S for B { fn g(&self) {} }\n").is_empty());
    // a test double written in a cfg(test) module
    assert!(single("trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\n#[cfg(test)]\nmod t { struct M; impl super::S for M { fn g(&self) {} } }\n").is_empty());
    // the one implementation is named like a double
    assert!(single("trait S { fn g(&self); }\nstruct InMemoryStore;\nimpl S for InMemoryStore { fn g(&self) {} }\n").is_empty());
    // positive control: the plain form is a shape
    assert_eq!(
        single("trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\n").len(),
        1
    );
}

#[test]
fn a_blanket_impl_a_marker_trait_a_supertrait_and_public_api_are_not_reported() {
    assert!(single("trait S { fn g(&self); }\nimpl<T> S for T { fn g(&self) {} }\n").is_empty());
    assert!(single("trait Marker {}\nstruct A;\nimpl Marker for A {}\n").is_empty());
    // `Base` is a supertrait of `Ext` and is not reported; `Ext` itself has one implementation and is
    assert_eq!(
        single(
            "trait Base { fn b(&self); }
trait Ext: Base { fn e(&self); }
struct A;
impl Base for A { fn b(&self) {} }
impl Ext for A { fn e(&self) {} }
"
        ),
        ["Ext<-A"]
    );
    let f = facts(&[file(
        "src/lib.rs",
        "pub trait Api { fn g(&self); }\nstruct A;\nimpl Api for A { fn g(&self) {} }\n",
    )]);
    let public: BTreeSet<String> = ["Api".to_string()].into();
    assert!(single_impl_traits(&f, &public).is_empty());
    assert_eq!(single_impl_traits(&f, &BTreeSet::new()).len(), 1);
}

#[test]
fn an_implementation_in_a_test_file_is_a_double() {
    let f = {
        let files = [file(
            "src/lib.rs",
            "trait S { fn g(&self); }\nstruct A;\nimpl S for A { fn g(&self) {} }\n",
        )];
        let mut f = facts(&files);
        f.test_impls.extend(
            test_file_impls(&[file(
                "src/tests.rs",
                "struct M;\nimpl super::S for M { fn g(&self) {} }\n",
            )])
            .unwrap(),
        );
        f
    };
    assert!(single_impl_traits(&f, &BTreeSet::new()).is_empty());
}

fn chains(src: &str) -> Vec<Vec<String>> {
    forwarding_chains(&facts(&[file("src/lib.rs", src)]))
        .into_iter()
        .map(|c| c.types)
        .collect()
}

const SERVICE_REPO_ADAPTER: &str = "
struct Service { repo: Repo }
impl Service { fn get(&self, id: u8) -> u8 { self.repo.get(id) } fn put(&self, id: u8, v: u8) { self.repo.put(id, v) } }
struct Repo { adapter: std::sync::Arc<Adapter> }
impl Repo { fn get(&self, id: u8) -> u8 { self.adapter.get(id) } fn put(&self, id: u8, v: u8) { self.adapter.put(id, v) } }
struct Adapter { map: std::collections::HashMap<u8, u8> }
impl Adapter { fn get(&self, id: u8) -> u8 { self.map[&id] } fn put(&self, id: u8, v: u8) { let _ = (id, v); } }
";

#[test]
fn a_service_repository_adapter_chain_is_reported_once_at_its_first_type() {
    // Service and Repo only forward; Adapter does the work, so it is not part of the chain
    assert_eq!(chains(SERVICE_REPO_ADAPTER), [["Service", "Repo"]]);
}

#[test]
fn one_forwarding_type_alone_a_type_that_does_something_else_and_a_one_method_type_are_no_chain() {
    // only the outer one forwards
    assert!(chains("struct A { b: B }\nimpl A { fn x(&self, i: u8) { self.b.x(i) } fn y(&self, i: u8) { self.b.y(i) } }\nstruct B { n: u8 }\nimpl B { fn x(&self, i: u8) { let _ = i; } fn y(&self, i: u8) { let _ = i; } }\n").is_empty());
    // a forwarding type with one extra method of its own is not forwarding-only
    assert!(chains("struct A { b: B }\nimpl A { fn x(&self, i: u8) { self.b.x(i) } fn y(&self, i: u8) -> u8 { i + 1 } }\nstruct B { c: C }\nimpl B { fn x(&self, i: u8) { self.c.x(i) } fn y(&self, i: u8) { self.c.y(i) } }\nstruct C;\nimpl C { fn x(&self, i: u8) { let _ = i; } fn y(&self, i: u8) { let _ = i; } }\n").is_empty());
    // a type with a single forwarding method is not enough to call it a layer
    assert!(chains("struct A { b: B }\nimpl A { fn x(&self, i: u8) { self.b.x(i) } }\nstruct B { c: C }\nimpl B { fn x(&self, i: u8) { self.c.x(i) } }\nstruct C;\nimpl C { fn x(&self, i: u8) { let _ = i; } }\n").is_empty());
}

#[test]
fn methods_in_another_file_than_the_struct_still_count() {
    let files = [
        file("src/lib.rs", "mod a;\nmod b;\n"),
        file("src/b.rs", "pub struct B { c: C }\nstruct C;\nimpl C { fn x(&self, i: u8) { let _ = i; } fn y(&self, i: u8) { let _ = i; } }\nimpl B { fn x(&self, i: u8) { self.c.x(i) } fn y(&self, i: u8) { self.c.y(i) } }\n"),
        file("src/a.rs", "struct A { b: crate::b::B }\nimpl A { fn x(&self, i: u8) { self.b.x(i) } fn y(&self, i: u8) { self.b.y(i) } }\n"),
    ];
    let c = forwarding_chains(&facts(&files));
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].types, ["A", "B"]);
}

#[test]
fn findings_are_info_low_confidence_and_name_the_shape() {
    let f = facts(&[file("src/lib.rs", &format!("{SERVICE_REPO_ADAPTER}trait S {{ fn g(&self); }}\nstruct Z;\nimpl S for Z {{ fn g(&self) {{}} }}\n"))]);
    let out = judge(
        &single_impl_traits(&f, &BTreeSet::new()),
        &forwarding_chains(&f),
        "p",
    );
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(out
        .iter()
        .all(|x| x.severity == Severity::Info && x.confidence == Confidence::Low));
    assert!(out.iter().any(|x| x
        .summary
        .contains("one implementation, `Z`, and is never implemented in test code")));
    assert!(out
        .iter()
        .any(|x| x.summary.contains("`Service` -> `Repo`")));
}

#[test]
fn an_unparsable_file_is_an_error_never_a_smaller_count() {
    assert!(collect(&[file("src/a.rs", "trait (")]).is_err());
}
