//! The coverage report (multi-language design section 6): computed from the catalog joined with what a module claims and has
//! proven, never asserted. A claim without a proof is a gap, a missing module is every rule not covered, and the report
//! says so in CodeRipper's own voice: an incomplete implementation, not a finding about the checked code.

use coderipper::catalog::Catalog;
use coderipper::coverage::{for_missing_module, for_module, RuleRun, RunOutcome};
use coderipper::module::{Capabilities, Hello, RuleClaim};

/// A catalog of rules `a`-`e` (the ids are `WSP-90n`): `a`, `b`, `c` apply to any language, `d` only to rust, `e` only to python.
fn catalog() -> Catalog {
    fn record(id: &str, languages: &str) -> String {
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
remediation = "What to do."
lifecycle = "experimental"

[rule.applicability]
languages = {languages}
scopes = ["first_party"]
"#
        )
    }
    let text = [
        record("WSP-901", "[\"any\"]"),
        record("WSP-902", "[\"any\"]"),
        record("WSP-903", "[\"any\"]"),
        record("WSP-904", "[\"rust\"]"),
        record("WSP-905", "[\"python\"]"),
    ]
    .concat();
    Catalog::parse(&[("wsp.toml", &text)]).expect("valid")
}

fn claim(id: &str, status: &str, proof: Option<&str>) -> RuleClaim {
    let mut c = RuleClaim::native(id);
    c.status = status.into();
    c.proof = proof.map(str::to_string);
    if status == "not-applicable" {
        c.reason = Some("no such construct".into());
    }
    c
}

fn hello(claims: Vec<RuleClaim>) -> Hello {
    Hello::new(
        "rust",
        "0.0.1",
        vec!["rust".into()],
        vec!["Cargo.toml".into()],
        Capabilities::default(),
        claims,
    )
}

#[test]
fn only_a_claim_with_a_proof_is_covered() {
    let h = hello(vec![
        claim("WSP-901", "implemented-native", Some("conformance/WSP-901")),
        claim("WSP-902", "implemented-native", None),
    ]);
    let c = for_module(&catalog(), &h, "rust", None);
    assert_eq!(c.covered, 1);
    assert_eq!(
        c.claimed_unproven, 1,
        "a claim with no proof is counted separately"
    );
    assert!(
        c.gaps.contains(&"WSP-902".to_string()),
        "and as a gap: {:?}",
        c.gaps
    );
    assert!(!c.gaps.contains(&"WSP-901".to_string()));
}

#[test]
fn the_total_is_the_rules_that_apply_to_the_language() {
    let c = for_module(&catalog(), &hello(vec![]), "rust", None);
    assert_eq!(
        c.rules_total, 4,
        "a, b, c and the rust-only d; not the python-only e"
    );
    let py = for_missing_module(&catalog(), "python", 3);
    assert_eq!(py.rules_total, 4, "a, b, c and the python-only e");
}

#[test]
fn a_rule_a_module_does_not_mention_is_not_covered_and_listed_as_a_gap() {
    let h = hello(vec![claim("WSP-901", "implemented-native", Some("p"))]);
    let c = for_module(&catalog(), &h, "rust", None);
    assert_eq!(c.not_covered, 3);
    for gap in ["WSP-902", "WSP-903", "WSP-904"] {
        assert!(c.gaps.contains(&gap.to_string()), "{gap} in {:?}", c.gaps);
    }
    // The four buckets add up to the total: nothing is dropped or counted twice.
    assert_eq!(
        c.covered + c.claimed_unproven + c.not_applicable + c.not_covered,
        c.rules_total
    );
}

#[test]
fn a_not_applicable_claim_is_neither_covered_nor_a_gap() {
    let h = hello(vec![claim("WSP-903", "not-applicable", None)]);
    let c = for_module(&catalog(), &h, "rust", None);
    assert_eq!(c.not_applicable, 1);
    assert!(!c.gaps.contains(&"WSP-903".to_string()));
}

#[test]
fn a_claim_for_a_rule_outside_the_catalog_is_not_counted() {
    let h = hello(vec![claim("NOPE-1", "implemented-native", Some("p"))]);
    let c = for_module(&catalog(), &h, "rust", None);
    assert_eq!(c.covered, 0);
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(c.problems[0].contains("NOPE-1"));
}

#[test]
fn a_language_with_no_module_has_every_rule_not_covered_and_says_why() {
    let c = for_missing_module(&catalog(), "typescript", 14);
    assert_eq!(c.module, None);
    assert_eq!(c.covered, 0);
    assert_eq!(c.not_covered, c.rules_total);
    assert_eq!(c.source_files, Some(14));
    assert!(
        c.note
            .as_deref()
            .unwrap()
            .contains("no module for typescript"),
        "{:?}",
        c.note
    );
}

#[test]
fn this_runs_outcomes_are_counted_by_kind() {
    let h = hello(vec![claim("WSP-901", "implemented-native", Some("p"))]);
    let run = vec![
        RuleRun::new("a", RunOutcome::Clean),
        RuleRun::new("b", RunOutcome::Clean),
        RuleRun::new("c", RunOutcome::Findings(3)),
        RuleRun::new("d", RunOutcome::Skipped),
        RuleRun::new("e", RunOutcome::CouldNotRun),
    ];
    let c = for_module(&catalog(), &h, "rust", Some(&run));
    let counts = c.run.expect("a run was given");
    assert_eq!(
        (
            counts.clean,
            counts.findings,
            counts.skipped,
            counts.could_not_run
        ),
        (2, 1, 1, 1),
        "findings counts rules with findings, not findings"
    );
}

#[test]
fn the_json_line_has_the_documented_shape() {
    let h = hello(vec![claim("WSP-901", "implemented-native", Some("p"))]);
    let run = vec![RuleRun::new("a", RunOutcome::Clean)];
    let value = for_module(&catalog(), &h, "rust", Some(&run)).to_json();
    assert_eq!(value["reason"], "coderipper-coverage");
    assert_eq!(value["language"], "rust");
    for key in [
        "rules_total",
        "covered",
        "claimed_unproven",
        "not_applicable",
        "not_covered",
        "gaps",
        "gaps_truncated",
        "run",
    ] {
        assert!(value.get(key).is_some(), "missing {key}: {value}");
    }
    assert_eq!(value["run"]["clean"], 1);
    assert_eq!(value["gaps_truncated"], false);
}

#[test]
fn a_long_gap_list_is_cut_and_says_so() {
    let c = for_missing_module(&catalog(), "python", 1);
    let value = c.to_json_with_gap_limit(2);
    assert_eq!(value["gaps"].as_array().unwrap().len(), 2);
    assert_eq!(value["gaps_truncated"], true);
    assert_eq!(
        value["not_covered"], 4,
        "the counts are never cut, only the list"
    );
}

#[test]
fn the_human_line_calls_a_gap_not_yet_implemented_never_a_defect() {
    let h = hello(vec![claim("WSP-901", "implemented-native", Some("p"))]);
    let line = for_module(&catalog(), &h, "rust", None).human_line();
    assert!(line.contains("1 of 4 covered"), "{line}");
    assert!(line.contains("not yet implemented"), "{line}");
    for banned in ["fail", "defect", "violation"] {
        assert!(!line.contains(banned), "{line}");
    }
}

#[test]
fn the_builtin_rust_module_claims_its_five_rules_and_proves_four() {
    let checks = coderipper::registered_checks();
    let module = coderipper::module::RustModule::new(&checks);
    let hello = coderipper::module::Module::describe(&module).unwrap();
    let c = for_module(Catalog::builtin(), &hello, "rust", None);
    assert_eq!(c.covered, 4, "four claims carry a proof");
    assert_eq!(
        c.claimed_unproven, 1,
        "ci-protection-presence has no proof yet"
    );
    assert!(c.gaps.contains(&"ci-protection-presence".to_string()));
    assert!(c.problems.is_empty(), "{:?}", c.problems);
}

#[test]
fn a_rule_claimed_twice_is_counted_once_and_reported_as_a_problem() {
    // Once by its id and once by an alias would otherwise make the buckets disagree with the total.
    let text = "
[[rule]]
id = \"WSP-901\"
title = \"A title\"
domain = \"WSP\"
statement = \"What the rule claims.\"
rationale = \"Why it matters.\"
evidence_tiers = [\"V0\"]
evidence_strength = \"C\"
contested = false
scope = \"project\"
network = \"local\"
unit = \"package\"
executes_project_code = false
default_severity = \"medium\"
confidence_class = \"high\"
kb_refs = []
aliases = [\"old-name\"]
remediation = \"What to do.\"
lifecycle = \"experimental\"

[rule.applicability]
languages = [\"any\"]
scopes = [\"first_party\"]
";
    let catalog = Catalog::parse(&[("wsp.toml", text)]).unwrap();
    let h = hello(vec![
        claim("WSP-901", "implemented-native", Some("p")),
        claim("old-name", "implemented-native", Some("p")),
    ]);
    let c = for_module(&catalog, &h, "rust", None);
    assert_eq!(c.covered, 1, "one rule, counted once");
    assert_eq!(
        c.covered + c.claimed_unproven + c.not_applicable + c.not_covered,
        c.rules_total
    );
    assert!(
        c.problems.iter().any(|p| p.contains("twice")),
        "{:?}",
        c.problems
    );
}

#[test]
fn a_claim_by_alias_counts_for_the_canonical_rule() {
    let text = "
[[rule]]
id = \"WSP-901\"
title = \"A title\"
domain = \"WSP\"
statement = \"What the rule claims.\"
rationale = \"Why it matters.\"
evidence_tiers = [\"V0\"]
evidence_strength = \"C\"
contested = false
scope = \"project\"
network = \"local\"
unit = \"package\"
executes_project_code = false
default_severity = \"medium\"
confidence_class = \"high\"
kb_refs = []
aliases = [\"old-name\"]
remediation = \"What to do.\"
lifecycle = \"experimental\"

[rule.applicability]
languages = [\"any\"]
scopes = [\"first_party\"]
";
    let catalog = Catalog::parse(&[("wsp.toml", text)]).unwrap();
    let h = hello(vec![claim("old-name", "implemented-native", Some("p"))]);
    let c = for_module(&catalog, &h, "rust", None);
    assert_eq!((c.covered, c.not_covered), (1, 0));
}

#[test]
fn outcomes_for_the_same_rule_combine_so_the_audit_is_never_overstated() {
    use RunOutcome::{Clean, CouldNotRun, Findings, Skipped};
    // A rule that could not run anywhere is incomplete, whatever else happened.
    assert_eq!(Clean.combine(CouldNotRun), CouldNotRun);
    assert_eq!(CouldNotRun.combine(Findings(2)), CouldNotRun);
    // Findings add up and beat clean.
    assert_eq!(Findings(1).combine(Findings(2)), Findings(3));
    assert_eq!(Clean.combine(Findings(2)), Findings(2));
    // It ran somewhere clean: clean, even if it did not apply elsewhere.
    assert_eq!(Skipped.combine(Clean), Clean);
    assert_eq!(Skipped.combine(Skipped), Skipped);
}

#[test]
fn merging_runs_keeps_one_entry_per_rule() {
    use coderipper::coverage::merge_runs;
    let mut all = vec![RuleRun::new("a", RunOutcome::Clean)];
    merge_runs(
        &mut all,
        vec![
            RuleRun::new("a", RunOutcome::Findings(1)),
            RuleRun::new("b", RunOutcome::Skipped),
        ],
    );
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].outcome, RunOutcome::Findings(1));
    assert_eq!(all[1].rule, "b");
}

#[test]
fn one_source_file_is_singular() {
    let line = for_missing_module(&catalog(), "python", 1).human_line();
    assert!(line.contains("1 source file not checked"), "{line}");
    let line = for_missing_module(&catalog(), "python", 2).human_line();
    assert!(line.contains("2 source files not checked"), "{line}");
}
