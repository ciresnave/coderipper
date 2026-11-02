// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

#[test]
fn a_literal_that_is_exactly_the_name_or_sql_holding_it_names_it() {
    assert!(literal_names("orders", "orders"));
    assert!(literal_names("  orders ", "orders"));
    assert!(literal_names(
        "SELECT id FROM orders WHERE id = $1",
        "orders"
    ));
    assert!(literal_names(
        "insert into \"Orders\" (id) values ($1)",
        "orders"
    ));
}

#[test]
fn prose_a_longer_identifier_and_a_part_of_a_word_do_not() {
    assert!(!literal_names("the orders could not be sent", "orders"));
    assert!(!literal_names("SELECT id FROM orders_archive", "orders"));
    assert!(!literal_names("SELECT id FROM preorders", "orders"));
    assert!(!literal_names("orders and more", "orders"));
    // positive control: the same statement names a table it does use
    assert!(literal_names(
        "SELECT id FROM orders_archive",
        "orders_archive"
    ));
}

#[test]
fn literals_are_read_from_macro_calls_and_test_items_are_skipped() {
    let src = r#"
fn live() { let _ = sqlx::query!("SELECT id FROM orders"); let _ = format!("DELETE FROM orders WHERE id = {}", 1); }
#[cfg(test)]
mod tests { fn t() { let _ = "orders"; } }
#[test]
fn also_a_test() { let _ = "orders"; }
"#;
    let lits = literals(src).unwrap();
    assert!(
        lits.iter().any(|l| l == "SELECT id FROM orders"),
        "{lits:?}"
    );
    assert!(
        lits.iter().any(|l| l.starts_with("DELETE FROM orders")),
        "{lits:?}"
    );
    assert!(!lits.iter().any(|l| l == "orders"), "{lits:?}");
}

#[test]
fn raw_and_escaped_strings_are_read_by_value() {
    let lits =
        literals("fn f() { let _ = r#\"SELECT * FROM \"orders\"\"#; let _ = \"a\nb\"; }").unwrap();
    assert!(lits.iter().any(|l| literal_names(l, "orders")), "{lits:?}");
    assert!(lits.iter().any(|l| l == "a\nb"), "{lits:?}");
}

#[test]
fn an_unparsable_file_is_an_error_never_a_smaller_count() {
    assert!(literals("fn (").is_err());
}

#[test]
fn sql_files_count_lines_outside_comments() {
    let sql = "-- orders live here\nCREATE TABLE orders (id int);\nSELECT * FROM orders; -- orders again\nSELECT 1;\n";
    assert_eq!(sql_mentions(sql, "orders"), 2);
    assert_eq!(sql_mentions(sql, "customers"), 0);
}

fn owned<'a>(pairs: &[(&str, &[&'a str])]) -> BTreeMap<String, BTreeSet<&'a str>> {
    pairs
        .iter()
        .map(|(n, o)| ((*n).to_string(), o.iter().copied().collect()))
        .collect()
}

fn used(name: &str, component: &str, file: &str, count: usize) -> Use {
    Use {
        name: name.into(),
        component: component.into(),
        file: file.into(),
        count,
        manifest: format!("{component}/Cargo.toml"),
    }
}

#[test]
fn one_low_finding_per_name_and_other_component_with_counts_and_files() {
    let (f, notes) = judge(
        &owned(&[("orders", &["orders-svc"])]),
        vec![
            used("orders", "billing", "billing/src/a.rs", 2),
            used("orders", "billing", "billing/migrations/1.sql", 1),
            used("orders", "web", "web/src/b.rs", 1),
        ],
        "p",
        vec![],
    );
    assert_eq!(f.len(), 2, "{f:?}");
    assert!(f
        .iter()
        .all(|x| x.severity == Severity::Low && x.confidence == Confidence::Low));
    let b = f
        .iter()
        .find(|x| x.subject.as_deref() == Some("billing:orders"))
        .unwrap();
    assert!(
        b.summary.contains("3 place(s) in 2 file(s)"),
        "{}",
        b.summary
    );
    assert!(b.summary.contains("`orders-svc`"));
    assert!(notes[0].contains("orders (orders-svc)"), "{notes:?}");
}

#[test]
fn nothing_found_is_no_finding() {
    let (f, _) = judge(&owned(&[("orders", &["orders-svc"])]), vec![], "p", vec![]);
    assert!(f.is_empty());
}

#[test]
fn a_name_two_components_own_is_an_info_finding() {
    let (f, _) = judge(&owned(&[("orders", &["a", "b"])]), vec![], "p", vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].subject.as_deref(), Some("orders:owners"));
}

#[test]
fn not_run_names_what_to_add() {
    assert!(missing_declaration(None)
        .unwrap()
        .contains("no [architecture] table"));
    let a: ArchitectureConfig = toml::from_str("[[component]]\nname = \"x\"\n").unwrap();
    assert!(missing_declaration(Some(&a)).unwrap().contains("owns_data"));
    let a: ArchitectureConfig =
        toml::from_str("[[component]]\nname = \"x\"\nowns_data = [\"orders\"]\n").unwrap();
    assert!(missing_declaration(Some(&a)).is_none());
}
