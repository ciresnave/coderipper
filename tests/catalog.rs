//! The rule catalog (multi-language design §4): records are data, parsed strictly, and the five built-in checks each have
//! one that agrees with what the check declares about itself.

use coderipper::catalog::{Catalog, CatalogError, Lifecycle};
use coderipper::check::{Network, Scope, Unit};

/// A complete, valid record in the file format, to mutate one field at a time.
fn record(id: &str) -> String {
    format!(
        r#"
[[rule]]
id = "{id}"
title = "A title"
domain = "WSP"
statement = "What the rule claims."
rationale = "Why it matters."
evidence_tiers = ["V0"]
evidence_strength = "C"
contested = false
scope = "project"
network = "local"
unit = "package"
executes_project_code = false
default_severity = "medium"
confidence_class = "high"
kb_refs = []
aliases = []
remediation = "What to do."
related = []
lifecycle = "advisory"

[rule.applicability]
languages = ["any"]
scopes = ["first_party"]
"#
    )
}

fn parse(files: &[(&str, String)]) -> Result<Catalog, CatalogError> {
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
    Catalog::parse(&borrowed)
}

#[test]
fn every_registered_check_has_a_record() {
    let catalog = Catalog::builtin();
    let registered = coderipper::registered_checks();
    assert_eq!(
        registered.len(),
        5,
        "positive control: the five built-in checks"
    );
    for check in registered {
        assert!(
            catalog.get(check.id()).is_some(),
            "no record for {}",
            check.id()
        );
    }
}

/// The triage list (our own data, one row per rule of the knowledge base) and the catalog must name the same rules, and
/// the catalog adds exactly the five original checks.
#[test]
fn the_catalog_has_every_triaged_rule_and_the_five_checks_and_nothing_else() {
    let tsv = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/superpowers/triage/rules.tsv"),
    )
    .expect("the triage list");
    let mut triaged: Vec<&str> = tsv
        .lines()
        .skip(1)
        .filter_map(|l| l.split('\t').next())
        .filter(|id| !id.is_empty())
        .collect();
    assert_eq!(
        triaged.len(),
        216,
        "positive control: the triage list has 216 rules"
    );
    triaged.extend(coderipper::registered_checks().iter().map(|c| c.id()));
    let mut in_catalog: Vec<&str> = Catalog::builtin().rules().iter().map(|r| r.id()).collect();
    triaged.sort_unstable();
    in_catalog.sort_unstable();
    assert_eq!(in_catalog, triaged);
}

#[test]
fn a_knowledge_base_id_sits_in_the_file_of_its_domain() {
    for rule in Catalog::builtin().rules() {
        if let Some((prefix, number)) = rule.id().split_once('-') {
            if prefix.len() == 3 && number.chars().all(|c| c.is_ascii_digit()) {
                assert_eq!(rule.domain(), prefix, "{}", rule.id());
            }
        }
    }
}

#[test]
fn every_new_rule_starts_experimental_with_no_overlap_refs() {
    for rule in Catalog::builtin().rules() {
        if rule
            .id()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            assert_eq!(rule.lifecycle(), Lifecycle::Experimental, "{}", rule.id());
            assert!(rule.kb_refs().is_empty(), "{}", rule.id());
        }
    }
}

#[test]
fn each_legacy_record_agrees_with_its_check() {
    let catalog = Catalog::builtin();
    for check in coderipper::registered_checks() {
        let rule = catalog
            .get(check.id())
            .unwrap_or_else(|| panic!("no record for {}", check.id()));
        assert_eq!(rule.scope(), check.scope(), "{}: scope", check.id());
        assert_eq!(rule.network(), check.network(), "{}: network", check.id());
        assert_eq!(rule.unit(), check.unit(), "{}: unit", check.id());
    }
}

#[test]
fn the_expected_attributes_are_what_the_checks_have_today() {
    // Positive control for the loop above: it would pass vacuously if both sides were read through the same wrong lens.
    let catalog = Catalog::builtin();
    let ci = catalog.get("ci-protection-presence").expect("record");
    assert_eq!(
        (ci.scope(), ci.network(), ci.unit()),
        (Scope::Project, Network::NetworkRequired, Unit::Repository)
    );
    let reach = catalog.get("reachability").expect("record");
    assert_eq!(
        (reach.scope(), reach.network(), reach.unit()),
        (Scope::Project, Network::LocalOnly, Unit::Package)
    );
}

#[test]
fn the_legacy_ids_keep_their_kb_refs() {
    let catalog = Catalog::builtin();
    let expected: [(&str, &[&str]); 5] = [
        ("reachability", &["RDB-006", "MOD-001"]),
        ("unused-parameters", &["RDB-006"]),
        ("unused-return-values", &["ERR-001", "ERR-011"]),
        ("version-consistency", &["WSP-003"]),
        ("ci-protection-presence", &[]),
    ];
    for (id, refs) in expected {
        assert_eq!(catalog.get(id).expect("record").kb_refs(), refs, "{id}");
    }
}

#[test]
fn the_legacy_records_state_what_each_check_emits_and_does() {
    use coderipper::finding::{Confidence, Severity};
    let catalog = Catalog::builtin();
    // (id, executes project code, default severity, confidence class), read from the checks' own findings.
    let expected = [
        ("reachability", true, Severity::Medium, Confidence::Medium),
        ("unused-parameters", true, Severity::Low, Confidence::High),
        (
            "unused-return-values",
            true,
            Severity::Medium,
            Confidence::Medium,
        ),
        (
            "version-consistency",
            false,
            Severity::High,
            Confidence::High,
        ),
        (
            "ci-protection-presence",
            false,
            Severity::High,
            Confidence::High,
        ),
    ];
    for (id, runs_code, severity, confidence) in expected {
        let rule = catalog.get(id).expect("record");
        assert_eq!(rule.executes_project_code(), runs_code, "{id}");
        assert_eq!(rule.default_severity(), severity, "{id}");
        assert_eq!(rule.confidence_class(), confidence, "{id}");
    }
}

#[test]
fn a_valid_record_parses() {
    let catalog = parse(&[("wsp.toml", record("WSP-900"))]).expect("valid");
    let rule = catalog.get("WSP-900").expect("present");
    assert_eq!(rule.lifecycle(), Lifecycle::Advisory);
    assert_eq!(rule.domain(), "WSP");
}

#[test]
fn an_unknown_field_is_rejected_and_names_the_file() {
    let text =
        record("WSP-900").replace("contested = false", "contested = false\nfavourite = true");
    let err = parse(&[("wsp.toml", text)]).expect_err("unknown field");
    let message = err.to_string();
    assert!(message.contains("wsp.toml"), "{message}");
    assert!(message.contains("favourite"), "{message}");
}

#[test]
fn a_missing_required_field_is_rejected() {
    let text = record("WSP-900").replace("statement = \"What the rule claims.\"\n", "");
    let err = parse(&[("wsp.toml", text)]).expect_err("missing statement");
    assert!(err.to_string().contains("statement"), "{err}");
}

#[test]
fn a_duplicate_id_across_files_is_rejected() {
    let err = parse(&[("a.toml", record("WSP-900")), ("b.toml", record("WSP-900"))])
        .expect_err("duplicate");
    let message = err.to_string();
    assert!(
        message.contains("WSP-900") && message.contains("a.toml") && message.contains("b.toml"),
        "{message}"
    );
}

#[test]
fn an_alias_that_collides_with_an_id_is_rejected() {
    let second = record("WSP-901").replace("aliases = []", "aliases = [\"WSP-900\"]");
    let err = parse(&[("a.toml", record("WSP-900")), ("b.toml", second)]).expect_err("collision");
    assert!(err.to_string().contains("WSP-900"), "{err}");
}

#[test]
fn an_alias_resolves_to_the_canonical_id() {
    let text = record("version-consistency").replace("aliases = []", "aliases = [\"WSP-003\"]");
    let catalog = parse(&[("wsp.toml", text)]).expect("valid");
    assert_eq!(catalog.canonical_id("WSP-003"), Some("version-consistency"));
    assert_eq!(
        catalog.canonical_id("version-consistency"),
        Some("version-consistency")
    );
    assert_eq!(catalog.canonical_id("nonexistent"), None);
}

#[test]
fn an_empty_statement_is_rejected() {
    let text = record("WSP-900").replace("What the rule claims.", "  ");
    let err = parse(&[("wsp.toml", text)]).expect_err("blank statement");
    assert!(err.to_string().contains("statement"), "{err}");
}

#[test]
fn a_rule_id_must_look_like_an_id() {
    let err = parse(&[("wsp.toml", record("Has Spaces"))]).expect_err("bad id");
    assert!(err.to_string().contains("Has Spaces"), "{err}");
}

#[test]
fn a_kb_ref_must_look_like_a_knowledge_base_id() {
    let text = record("WSP-900").replace("kb_refs = []", "kb_refs = [\"not-an-id\"]");
    let err = parse(&[("wsp.toml", text)]).expect_err("bad kb ref");
    assert!(err.to_string().contains("not-an-id"), "{err}");
}

#[test]
fn rules_come_back_in_file_then_record_order() {
    let catalog =
        parse(&[("a.toml", record("WSP-902")), ("b.toml", record("WSP-901"))]).expect("valid");
    let ids: Vec<&str> = catalog.rules().iter().map(|r| r.id()).collect();
    assert_eq!(ids, ["WSP-902", "WSP-901"]);
}

#[test]
fn an_alias_equal_to_the_rules_own_id_is_an_error_not_a_panic() {
    let text = record("WSP-900").replace("aliases = []", "aliases = [\"WSP-900\"]");
    assert!(parse(&[("a.toml", text)]).is_err());
}

#[test]
fn an_alias_listed_twice_is_an_error_not_a_panic() {
    let text = record("WSP-900").replace("aliases = []", "aliases = [\"a-b\", \"a-b\"]");
    assert!(parse(&[("a.toml", text)]).is_err());
}

#[test]
fn a_related_rule_must_exist() {
    let text = record("WSP-900").replace("related = []", "related = [\"WSP-999\"]");
    let err = parse(&[("a.toml", text)]).expect_err("dangling related");
    assert!(
        err.to_string().contains("WSP-999") && err.to_string().contains("a.toml"),
        "{err}"
    );
}

#[test]
fn a_related_rule_in_another_file_resolves() {
    let text = record("WSP-900").replace("related = []", "related = [\"WSP-901\"]");
    assert!(parse(&[("a.toml", text), ("b.toml", record("WSP-901"))]).is_ok());
}

#[test]
fn a_rule_cannot_be_related_to_itself() {
    let text = record("WSP-900").replace("related = []", "related = [\"WSP-900\"]");
    assert!(parse(&[("a.toml", text)]).is_err());
}

#[test]
fn a_blank_list_entry_is_rejected() {
    let text = record("WSP-900").replace("languages = [\"any\"]", "languages = [\" \"]");
    let err = parse(&[("a.toml", text)]).expect_err("blank language");
    assert!(err.to_string().contains("languages"), "{err}");
}

#[test]
fn kb_refs_must_be_written_even_when_empty() {
    let text = record("WSP-900").replace(
        "kb_refs = []
",
        "",
    );
    let err = parse(&[("a.toml", text)]).expect_err("kb_refs omitted");
    assert!(err.to_string().contains("kb_refs"), "{err}");
}

#[test]
fn an_unknown_applicability_field_is_rejected() {
    let text = record("WSP-900").replace(
        "scopes = [\"first_party\"]",
        "scopes = [\"first_party\"]
flavour = \"x\"",
    );
    let err = parse(&[("a.toml", text)]).expect_err("unknown field");
    assert!(err.to_string().contains("flavour"), "{err}");
}
