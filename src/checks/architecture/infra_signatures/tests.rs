// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn banned(names: &[&str], prefixes: &[&str], crates: &[(&str, &str)]) -> Banned {
    Banned {
        names: names.iter().map(|s| (*s).to_string()).collect(),
        prefixes: prefixes
            .iter()
            .map(|p| p.split("::").map(str::to_string).collect())
            .collect(),
        crates: crates
            .iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect(),
    }
}

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn run(files: &[SourceFile], b: &Banned) -> Vec<Finding> {
    let (sites, _) = signature_sites(files, "core").unwrap();
    judge("domain", "core/Cargo.toml", &sites, b, "p")
}

fn lib(text: &str) -> Vec<SourceFile> {
    vec![file("src/lib.rs", text)]
}

fn subjects(f: &[Finding]) -> Vec<String> {
    f.iter().filter_map(|x| x.subject.clone()).collect()
}

#[test]
fn a_parameter_and_a_return_type_from_a_banned_namespace_are_found_through_use() {
    let f = run(
        &lib("use sqlx::PgPool;\npub fn load(p: &PgPool) -> u8 { 0 }\npub fn open() -> sqlx::Transaction<'static> { todo!() }\n"),
        &banned(&[], &["sqlx"], &[]),
    );
    assert_eq!(subjects(&f), ["domain:load", "domain:open"], "{f:#?}");
    assert!(f[0].summary.contains("sqlx::PgPool"));
}

#[test]
fn a_renamed_import_is_still_seen() {
    let f = run(
        &lib("use sqlx::PgPool as Db;\npub fn load(p: &Db) {}\n"),
        &banned(&[], &["sqlx"], &[]),
    );
    assert_eq!(f.len(), 1, "{f:#?}");
}

#[test]
fn a_name_pattern_matches_the_last_segment() {
    let f = run(
        &lib("mod other { pub struct DbConn; }\npub use other::DbConn;\npub fn f(c: DbConn) {}\n"),
        &banned(&["DbConn"], &[], &[]),
    );
    assert_eq!(subjects(&f), ["domain:f"], "{f:#?}");
}

#[test]
fn a_type_from_a_volatile_component_crate_is_found() {
    let f = run(
        &lib("pub struct Order { pub store: pg_store::Handle }\n"),
        &banned(&[], &[], &[("pg_store", "adapters")]),
    );
    assert_eq!(subjects(&f), ["domain:Order.store"], "{f:#?}");
    assert!(f[0].summary.contains("adapters"));
}

#[test]
fn generics_bounds_and_trait_objects_are_read() {
    let f = run(
        &lib("pub fn a<E: sqlx::Executor>(e: E) {}\npub fn b(x: Box<dyn axum::Handler>) {}\n"),
        &banned(&[], &["sqlx", "axum"], &[]),
    );
    assert_eq!(subjects(&f), ["domain:a", "domain:b"], "{f:#?}");
}

#[test]
fn private_items_and_private_modules_are_not_public_api() {
    let f = run(
        &lib("fn a(p: sqlx::Pool) {}\npub(crate) fn b(p: sqlx::Pool) {}\nmod hidden { pub fn c(p: sqlx::Pool) {} }\npub struct S { f: sqlx::Pool }\n"),
        &banned(&[], &["sqlx"], &[]),
    );
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn a_public_module_is_walked_and_a_method_of_a_public_type_counts() {
    let files = vec![
        file("src/lib.rs", "pub mod repo;\n"),
        file(
            "src/repo.rs",
            "pub struct Repo;\nimpl Repo { pub fn pool(&self) -> sqlx::Pool { todo!() } fn private(&self, p: sqlx::Pool) {} }\nstruct Hidden;\nimpl Hidden { pub fn x(&self, p: sqlx::Pool) {} }\n",
        ),
    ];
    let f = run(&files, &banned(&[], &["sqlx"], &[]));
    assert_eq!(subjects(&f), ["domain:Repo::pool"], "{f:#?}");
    assert_eq!(f[0].location.as_ref().unwrap().file, "core/src/repo.rs");
}

#[test]
fn enum_variant_fields_and_trait_items_are_read() {
    let f = run(
        &lib("pub enum E { A(sqlx::Pool), B(u8) }\npub trait T { fn m(&self) -> sqlx::Row; type X: axum::Marker; }\n"),
        &banned(&[], &["sqlx", "axum"], &[]),
    );
    assert_eq!(
        subjects(&f),
        ["domain:E::A", "domain:T::m", "domain:T::X"],
        "{f:#?}"
    );
}

#[test]
fn an_alias_does_not_hide_the_type() {
    let f = run(
        &lib("pub type Db = sqlx::PgPool;\npub fn load(d: Db) {}\n"),
        &banned(&[], &["sqlx"], &[]),
    );
    assert_eq!(subjects(&f), ["domain:Db", "domain:load"], "{f:#?}");
}

#[test]
fn cfg_test_items_trait_impls_and_clean_signatures_are_not_reported() {
    let f = run(
        &lib("pub struct S;\nimpl From<sqlx::Error> for S { fn from(e: sqlx::Error) -> S { S } }\n#[cfg(test)]\nmod t { pub fn x(p: sqlx::Pool) {} }\npub fn ok(a: u8, b: String) -> Vec<u8> { vec![] }\n"),
        &banned(&[], &["sqlx"], &[]),
    );
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn a_binary_crate_has_no_api_and_says_so() {
    let (sites, notes) =
        signature_sites(&[file("src/main.rs", "pub fn f(p: sqlx::Pool) {}")], "app").unwrap();
    assert!(sites.is_empty());
    assert!(notes[0].contains("no src/lib.rs"));
}

#[test]
fn an_unparsable_file_is_an_error_not_a_smaller_count() {
    assert!(signature_sites(&lib("pub fn ("), "x").is_err());
}

#[test]
fn what_is_declared_decides_whether_the_rule_runs() {
    let a = |t: &str| {
        crate::config::ConfigFile::parse(t, std::path::Path::new(".coderipper.toml"))
            .unwrap()
            .architecture
            .unwrap()
    };
    assert!(missing_declaration(None, &[]).is_some());
    assert!(missing_declaration(Some(&a("[architecture]\n")), &[])
        .unwrap()
        .contains("policy"));
    let policy = "[architecture]\n[[architecture.component]]\nname = \"d\"\nrole = \"policy\"\npaths = [\"core\"]\n";
    assert!(missing_declaration(Some(&a(policy)), &[])
        .unwrap()
        .contains("nothing is declared"));
    assert!(missing_declaration(Some(&a(policy)), &["sqlx::".into()]).is_none());
    let with_ns = format!("{policy}[architecture.volatile]\nnamespaces = [\"sqlx\"]\n");
    assert!(missing_declaration(Some(&a(&with_ns)), &[]).is_none());
}
