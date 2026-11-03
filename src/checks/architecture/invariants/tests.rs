// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn facts(src: &str) -> Vec<TypeFacts> {
    collect(&[file("src/lib.rs", src)], &|p| p.to_string()).unwrap()
}

#[test]
fn public_fields_derives_and_a_checking_constructor_are_read() {
    let t = facts(
        "#[derive(Debug, Default, Clone)]\npub struct Money { pub cents: i64, currency: String }\nimpl Money { pub fn try_new(c: i64) -> Result<Self, String> { if c < 0 { return Err(\"neg\".into()); } Ok(Self { cents: c, currency: String::new() }) } }\n",
    );
    assert_eq!(t.len(), 1);
    let m = &t[0];
    assert_eq!(m.name, "Money");
    assert!(m.public);
    assert_eq!(m.pub_fields, ["cents"]);
    assert_eq!(m.field_count, 2);
    assert!(m.derives.contains("Default") && m.derives.contains("Clone"));
    assert_eq!(m.constructors, ["try_new"]);
    assert_eq!(m.line, 2);
}

#[test]
fn a_constructor_that_only_wraps_is_not_a_validating_one() {
    let t = facts(
        "pub struct W { pub v: i64 }\nimpl W { pub fn new(v: i64) -> Result<Self, ()> { Ok(Self { v }) } pub fn from_raw(v: i64) -> Self { Self { v } } }\n",
    );
    assert!(t[0].constructors.is_empty(), "{:?}", t[0].constructors);
    // positive control: the same constructor with a rejection is one
    let t = facts(
        "pub struct W { pub v: i64 }\nimpl W { pub fn new(v: i64) -> Option<Self> { if v < 0 { None } else { Some(Self { v }) } } }\n",
    );
    assert_eq!(t[0].constructors, ["new"]);
}

#[test]
fn an_ensure_counts_a_question_mark_and_a_missing_argument_do_not() {
    let t = facts(
        "pub struct A { pub v: u8 }
impl A { pub fn from_str_value(s: &str) -> Result<Self, std::num::ParseIntError> { let v = s.parse()?; Ok(Self { v }) } }
pub struct B { pub v: u8 }
impl B { pub fn parse(v: u8) -> anyhow::Result<Self> { anyhow::ensure!(v > 0); Ok(Self { v }) } }
pub struct C { pub v: u8 }
impl C { pub fn from_env() -> Result<Self, String> { if true { return Err(\"x\".into()); } Ok(Self { v: 1 }) } }
",
    );
    // `?` only propagates another call's failure
    assert!(t[0].constructors.is_empty(), "{:?}", t[0].constructors);
    // `anyhow::Result<Self>` is a path ending in Result with Self first
    assert_eq!(t[1].constructors, ["parse"]);
    // a constructor that takes nothing reads its input from elsewhere (a configuration loader)
    assert!(t[2].constructors.is_empty(), "{:?}", t[2].constructors);
}

#[test]
fn serde_try_from_counts_as_a_validated_path_and_test_items_are_skipped() {
    let t = facts(
        "#[derive(serde::Deserialize)]\n#[serde(try_from = \"u8\")]\npub struct A(u8);\n#[derive(serde::Deserialize)]\npub struct B(pub u8);\n#[cfg(test)]\nmod t { pub struct C { pub x: u8 } }\n",
    );
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].serde_validated && !t[1].serde_validated);
    assert_eq!(t[1].pub_fields, ["0"]);
}

fn krate(component: Option<&str>, types: Vec<TypeFacts>) -> CrateFacts {
    CrateFacts {
        name: "core".into(),
        component: component.map(str::to_string),
        types,
    }
}

fn money(pub_fields: &[&str], ctors: &[&str], derives: &[&str]) -> TypeFacts {
    TypeFacts {
        name: "Money".into(),
        file: "core/src/lib.rs".into(),
        line: 4,
        public: true,
        pub_fields: pub_fields.iter().map(|s| (*s).to_string()).collect(),
        field_count: 2,
        derives: derives.iter().map(|s| (*s).to_string()).collect(),
        constructors: ctors.iter().map(|s| (*s).to_string()).collect(),
        ..TypeFacts::default()
    }
}

fn declared(comp: &str, names: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    [(
        comp.to_string(),
        names.iter().map(|s| (*s).to_string()).collect(),
    )]
    .into()
}

#[test]
fn a_declared_type_with_a_public_field_is_a_low_finding() {
    let c = krate(Some("core"), vec![money(&["cents"], &["try_new"], &[])]);
    let (f, notes) = judge(&[c], Some(&declared("core", &["Money"])), "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
    assert_eq!(f[0].subject.as_deref(), Some("Money"));
    assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
    assert!(
        f[0].detail
            .contains("`try_new` rejects bad input and can be bypassed"),
        "{}",
        f[0].detail
    );
    assert!(notes[0].contains("1 struct(s)"), "{notes:?}");
}

#[test]
fn private_fields_are_clean_and_a_default_or_deserialize_derive_is_info() {
    let clean = krate(Some("core"), vec![money(&[], &["try_new"], &[])]);
    let (f, _) = judge(&[clean], Some(&declared("core", &["Money"])), "p");
    assert!(f.is_empty(), "{f:?}");
    let derived = krate(
        Some("core"),
        vec![money(&[], &["try_new"], &["Default", "Deserialize"])],
    );
    let (f, _) = judge(&[derived], Some(&declared("core", &["Money"])), "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Info);
    assert!(
        f[0].summary.contains("Default and Deserialize"),
        "{}",
        f[0].summary
    );
}

#[test]
fn a_declared_name_found_in_no_crate_of_its_component_is_a_stale_info_with_a_control() {
    let c = krate(Some("core"), vec![money(&[], &[], &[])]);
    let (f, _) = judge(&[c], Some(&declared("core", &["Gone"])), "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("Gone:stale"));
    assert_eq!(f[0].claim, Claim::Absence);
    assert!(f[0]
        .positive_control
        .as_deref()
        .unwrap()
        .contains("1 struct(s)"));
}

#[test]
fn the_inferred_half_needs_a_public_type_a_public_field_and_a_checking_constructor() {
    let c = krate(None, vec![money(&["cents"], &["new"], &[])]);
    let (f, notes) = judge(&[c], None, "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].confidence, Confidence::Low);
    assert!(f[0].summary.contains("a caller can go around the check"));
    assert!(notes[0].contains("1 finding(s)"));
    // without a public field, without a checking constructor, or a type that is not public: none
    for t in [
        money(&[], &["new"], &[]),
        money(&["cents"], &[], &[]),
        TypeFacts {
            public: false,
            ..money(&["cents"], &["new"], &[])
        },
    ] {
        let (f, _) = judge(&[krate(None, vec![t])], None, "p");
        assert!(f.is_empty(), "{f:?}");
    }
}

#[test]
fn a_declared_type_is_not_also_reported_by_the_inferred_half() {
    let c = krate(Some("core"), vec![money(&["cents"], &["new"], &[])]);
    let (f, _) = judge(&[c], Some(&declared("core", &["Money"])), "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let w = |p: &str, t: &str| {
        let path = dir.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    };
    w(
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"core\"]\n",
    );
    w(
        "core/Cargo.toml",
        "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    w(
        "core/src/lib.rs",
        "pub struct Money { pub cents: i64 }\nimpl Money { pub fn new(c: i64) -> Result<Self, String> { if c < 0 { return Err(\"neg\".into()); } Ok(Self { cents: c }) } }\n",
    );
    w(
        ".coderipper.toml",
        "[architecture]\n[[architecture.component]]\nname = \"core\"\npaths = [\"core\"]\ninvariant_types = [\"Money\"]\n",
    );
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    dir
}

#[test]
fn a_real_workspace_with_a_declared_type_is_judged_end_to_end() {
    let dir = repo();
    let (f, _) = InvariantFieldsCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
    assert_eq!(f[0].location.as_ref().unwrap().file, "core/src/lib.rs");
}

#[test]
fn none_as_a_field_value_is_not_a_rejection_for_a_constructor_that_returns_a_result() {
    let t = facts(
        "pub struct S { pub x: Option<u8>, pub v: u8 }
impl S { pub fn new(v: u8) -> Result<Self, String> { let c = make()?; Ok(Self { x: None, v }) } }
pub struct O { pub v: u8 }
impl O { pub fn new(v: u8) -> Option<Self> { if v == 0 { None } else { Some(Self { v }) } } }
",
    );
    assert!(t[0].constructors.is_empty(), "{:?}", t[0].constructors);
    // positive control: for an `Option` constructor `None` is the rejection
    assert_eq!(t[1].constructors, ["new"]);
}
