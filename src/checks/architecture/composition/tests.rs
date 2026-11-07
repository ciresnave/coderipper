// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(text: &str) -> SourceFile {
    SourceFile {
        path: "src/lib.rs".into(),
        text: text.into(),
    }
}

fn adapters() -> BTreeMap<String, BTreeSet<String>> {
    BTreeMap::from([(
        "pg_store".to_string(),
        BTreeSet::from(["PgStore".to_string(), "Cache".to_string()]),
    )])
}

fn run(text: &str) -> Vec<Finding> {
    constructions(&file(text), "core/src/lib.rs", "domain", &adapters(), "p").unwrap()
}

fn subjects(f: &[Finding]) -> Vec<String> {
    f.iter().filter_map(|x| x.subject.clone()).collect()
}

#[test]
fn a_constructor_call_through_a_use_path_is_found() {
    let f = run("use pg_store::PgStore;\npub fn go() { let s = PgStore::new(1); }\n");
    assert_eq!(subjects(&f), ["domain:pg_store::PgStore"], "{f:#?}");
    assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
}

#[test]
fn a_fully_qualified_call_and_a_struct_literal_are_found() {
    let f = run("pub fn a() { pg_store::PgStore::connect(\"x\"); }\nuse pg_store::Cache;\npub fn b() { let _ = Cache { size: 3 }; }\n");
    assert_eq!(f.len(), 2, "{f:#?}");
}

#[test]
fn a_renamed_import_is_still_seen() {
    let f = run("use pg_store::PgStore as Db;\npub fn a() { Db::default(); }\n");
    assert_eq!(f.len(), 1, "{f:#?}");
}

#[test]
fn taking_the_port_is_clean() {
    let f = run("pub trait Store { fn get(&self) -> u8; }\npub fn a(s: &dyn Store) -> u8 { s.get() }\nuse pg_store::PgStore;\npub fn b(s: PgStore) -> u8 { 0 }\n");
    assert!(f.is_empty(), "naming the type is not building it: {f:#?}");
}

#[test]
fn a_method_that_is_not_a_constructor_a_test_and_another_type_are_not_reported() {
    let f = run("use pg_store::PgStore;\nuse other::Thing;\npub fn a() { PgStore::shutdown(); Thing::new(); }\n#[cfg(test)]\nmod t { use pg_store::PgStore; fn x() { PgStore::new(1); } }\n");
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn constructions_in_impl_blocks_and_nested_modules_are_found() {
    let f = run("use pg_store::PgStore;\npub struct S;\nimpl S { pub fn make() -> PgStore { PgStore::with_pool(2) } }\nmod inner { pub fn z() { super::PgStore::new(0); } }\n");
    assert_eq!(
        f.len(),
        1,
        "super:: path is not an adapter crate path: {f:#?}"
    );
}

#[test]
fn constructor_names() {
    for n in [
        "new",
        "default",
        "open",
        "new_in",
        "with_pool",
        "from_env",
        "try_new",
    ] {
        assert!(looks_like_constructor(n), "{n}");
    }
    for n in ["get", "shutdown", "newest", "within"] {
        assert!(!looks_like_constructor(n), "{n}");
    }
}

#[test]
fn an_unparsable_file_is_an_error_not_a_smaller_count() {
    assert!(constructions(&file("fn ("), "x", "c", &adapters(), "p").is_err());
}

#[test]
fn what_is_declared_decides_whether_the_rule_runs() {
    let a = |t: &str| {
        crate::config::ConfigFile::parse(t, std::path::Path::new(".coderipper.toml"))
            .unwrap()
            .architecture
            .unwrap()
    };
    assert!(missing_declaration(None).is_some());
    let comps = "[architecture]\n[[architecture.component]]\nname = \"d\"\nrole = \"policy\"\npaths = [\"core\"]\n[[architecture.component]]\nname = \"s\"\nrole = \"adapter\"\npaths = [\"store\"]\n";
    assert!(missing_declaration(Some(&a(comps)))
        .unwrap()
        .contains("roots"));
    let with_roots = format!("{comps}[architecture.composition]\nroots = [\"app/src/main.rs\"]\n");
    assert!(missing_declaration(Some(&a(&with_roots))).is_none());
    let no_adapter = "[architecture]\n[[architecture.component]]\nname = \"d\"\nrole = \"policy\"\npaths = [\"core\"]\n[architecture.composition]\nroots = [\"x.rs\"]\n";
    assert!(missing_declaration(Some(&a(no_adapter)))
        .unwrap()
        .contains("adapter"));
}

#[test]
fn a_unit_struct_used_as_a_value_is_found_but_a_type_position_is_not() {
    let f = run("use pg_store::PgStore;
pub fn a() { let _x = PgStore; }
pub fn b(_s: PgStore) -> Option<PgStore> { None }
");
    assert_eq!(f.len(), 1, "{f:#?}");
}
