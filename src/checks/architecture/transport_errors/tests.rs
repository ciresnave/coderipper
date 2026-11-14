// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::checks::rust_sources::SourceFile;

fn lib(text: &str) -> Vec<SourceFile> {
    vec![SourceFile {
        path: "src/lib.rs".into(),
        text: text.into(),
    }]
}

fn run(text: &str, extra: &[&[&str]]) -> Vec<Finding> {
    let (sites, _) = signature_sites(&lib(text), "core").unwrap();
    let mut namespaces: Vec<&[&str]> = TRANSPORT_NAMESPACES.iter().map(|_| &[][..]).collect();
    namespaces.clear();
    let owned: Vec<[&str; 1]> = TRANSPORT_NAMESPACES.iter().map(|n| [*n]).collect();
    for n in &owned {
        namespaces.push(&n[..]);
    }
    namespaces.extend_from_slice(extra);
    let banned = Banned::from_namespaces(&namespaces);
    judge("domain", "core/Cargo.toml", &sites, &banned, "p")
}

fn subjects(f: &[Finding]) -> Vec<String> {
    f.iter().filter_map(|x| x.subject.clone()).collect()
}

#[test]
fn a_driver_error_inside_a_domain_error_is_found_through_use() {
    let f = run(
        "use sqlx::Error as DbError;\npub enum OrderError {\n    NotFound,\n    Db(DbError),\n    Http(http::StatusCode),\n}\n",
        &[],
    );
    assert_eq!(
        subjects(&f),
        ["domain:OrderError::Db", "domain:OrderError::Http"],
        "{f:#?}"
    );
    assert!(f[0].summary.contains("sqlx::Error"), "{}", f[0].summary);
}

#[test]
fn a_struct_error_field_that_holds_a_status_code_is_found() {
    let f = run(
        "pub struct ApiError {\n    pub status: http::StatusCode,\n    pub message: String,\n}\n",
        &[],
    );
    assert_eq!(subjects(&f), ["domain:ApiError.status"], "{f:#?}");
}

#[test]
fn a_type_that_is_not_an_error_is_left_to_arc_004() {
    let f = run(
        "pub struct Order {\n    pub pool: sqlx::PgPool,\n}\npub fn load(p: &sqlx::PgPool) {}\n",
        &[],
    );
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn an_error_in_domain_terms_is_clean() {
    let f = run(
        "pub enum OrderError {\n    NotFound(u64),\n    OutOfStock { sku: String },\n    Invalid(String),\n}\n",
        &[],
    );
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn a_declared_volatile_namespace_counts_too() {
    let f = run(
        "pub enum StoreError {\n    Row(billing_db::Row),\n}\n",
        &[&["billing_db"]],
    );
    assert_eq!(subjects(&f), ["domain:StoreError::Row"], "{f:#?}");
}

#[test]
fn an_error_type_is_recognised_by_its_name() {
    assert!(in_error_type("OrderError::Db"));
    assert!(in_error_type("ApiError.status"));
    assert!(!in_error_type("Order.pool"));
    assert!(!in_error_type("load"));
    assert!(!in_error_type("ErrorKind"));
}

#[test]
fn a_private_error_type_is_not_read() {
    let f = run("enum HiddenError {\n    Db(sqlx::Error),\n}\n", &[]);
    assert!(f.is_empty(), "{f:#?}");
}
