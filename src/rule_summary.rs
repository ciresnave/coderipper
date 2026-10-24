// SPDX-License-Identifier: MIT OR Apache-2.0
//! `--summary`: per-rule counts for one run, so a portfolio sweep can be tracked from one number per rule instead of a pile of lines.
//!
//! A row counts, for one rule id, the findings the run reported, the findings an `[[allow]]` entry waived (`waived`), and the findings
//! a `[reachability] public_api` or `public_api_crates` declaration set aside (`declared_public`). The last two are the **opt-outs**: the
//! default-on policy says waivers must be visible, and `opt_outs` is their sum. Only rules with at least one of the three appear; a rule
//! that ran clean has no row (the coverage report says what ran).
//!
//! The JSON field names are stable: `reason`, `project`, `findings`, `waived`, `declared_public`, `opt_outs`, `errors`, and per rule
//! `rule`, `findings`, `waived`, `declared_public`, `severities`.

use crate::RunResult;
use std::collections::BTreeMap;

/// One rule's counts in one run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RuleRow {
    pub rule: String,
    pub findings: usize,
    pub waived: usize,
    pub declared_public: usize,
    /// Reported findings by severity, lower-case (`medium`: 3).
    pub severities: BTreeMap<String, usize>,
}

/// The rows of `result`, sorted by rule id.
pub(crate) fn rows(result: &RunResult) -> Vec<RuleRow> {
    let mut by_rule: BTreeMap<&str, RuleRow> = BTreeMap::new();
    for f in &result.findings {
        let r = by_rule
            .entry(f.check_id.as_str())
            .or_insert_with(|| RuleRow {
                rule: f.check_id.clone(),
                ..RuleRow::default()
            });
        r.findings += 1;
        *r.severities
            .entry(format!("{:?}", f.severity).to_lowercase())
            .or_insert(0) += 1;
    }
    for w in &result.waived {
        let r = by_rule
            .entry(w.finding.check_id.as_str())
            .or_insert_with(|| RuleRow {
                rule: w.finding.check_id.clone(),
                ..RuleRow::default()
            });
        if w.declared_public {
            r.declared_public += 1;
        } else {
            r.waived += 1;
        }
    }
    by_rule.into_values().collect()
}

/// The `coderipper-rule-summary` line.
pub(crate) fn to_json(project: &str, rows: &[RuleRow], errors: usize) -> serde_json::Value {
    let findings: usize = rows.iter().map(|r| r.findings).sum();
    let waived: usize = rows.iter().map(|r| r.waived).sum();
    let declared_public: usize = rows.iter().map(|r| r.declared_public).sum();
    serde_json::json!({
        "reason": "coderipper-rule-summary",
        "project": project,
        "findings": findings,
        "waived": waived,
        "declared_public": declared_public,
        "opt_outs": waived + declared_public,
        "errors": errors,
        "rules": rows.iter().map(|r| serde_json::json!({
            "rule": r.rule,
            "findings": r.findings,
            "waived": r.waived,
            "declared_public": r.declared_public,
            "severities": r.severities,
        })).collect::<Vec<_>>(),
    })
}

/// The human block: one line of totals, then one line per rule.
pub(crate) fn print_human(project: &str, rows: &[RuleRow], errors: usize) {
    let findings: usize = rows.iter().map(|r| r.findings).sum();
    let waived: usize = rows.iter().map(|r| r.waived).sum();
    let declared: usize = rows.iter().map(|r| r.declared_public).sum();
    println!(
        "summary {project}: {findings} finding(s), {} opt-out(s) ({waived} waived, {declared} declared public), {errors} error(s)",
        waived + declared
    );
    for r in rows {
        let severities = r
            .severities
            .iter()
            .map(|(s, n)| format!("{s} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:<24} findings {:>4}  waived {:>3}  declared-public {:>4}  {severities}",
            r.rule, r.findings, r.waived, r.declared_public
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Claim, Confidence, Finding, Severity};

    fn finding(rule: &str, severity: Severity) -> Finding {
        Finding {
            check_id: rule.into(),
            claim: Claim::Presence,
            severity,
            confidence: Confidence::High,
            project: "p".into(),
            location: None,
            subject: None,
            summary: "s".into(),
            detail: "d".into(),
            positive_control: None,
            member: None,
        }
    }

    fn waived(rule: &str, declared_public: bool) -> crate::finding::Waived {
        crate::finding::Waived {
            finding: finding(rule, Severity::Medium),
            reason: "r".into(),
            source: None,
            declared_public,
        }
    }

    #[test]
    fn counts_findings_waivers_and_declarations_per_rule_in_rule_order() {
        let mut result = RunResult::new(
            vec![
                finding("WSP-001", Severity::Medium),
                finding("reachability", Severity::Medium),
                finding("reachability", Severity::Low),
            ],
            Vec::new(),
        );
        result.waived = vec![
            waived("reachability", true),
            waived("reachability", true),
            waived("SUP-004", false),
        ];
        let rows = rows(&result);
        let ids: Vec<&str> = rows.iter().map(|r| r.rule.as_str()).collect();
        assert_eq!(ids, vec!["SUP-004", "WSP-001", "reachability"]);
        let reach = &rows[2];
        assert_eq!(
            (reach.findings, reach.waived, reach.declared_public),
            (2, 0, 2)
        );
        assert_eq!(reach.severities["medium"], 1);
        assert_eq!(reach.severities["low"], 1);
        let json = to_json("p", &rows, 0);
        assert_eq!(json["reason"], "coderipper-rule-summary");
        assert_eq!(json["findings"], 3);
        assert_eq!(json["opt_outs"], 3);
        assert_eq!(json["rules"][0]["rule"], "SUP-004");
    }

    #[test]
    fn a_clean_run_has_no_rows_and_zero_totals() {
        let rows = rows(&RunResult::new(Vec::new(), Vec::new()));
        assert!(rows.is_empty());
        let json = to_json("p", &rows, 0);
        assert_eq!(json["findings"], 0);
        assert_eq!(json["opt_outs"], 0);
        assert_eq!(json["rules"], serde_json::json!([]));
    }
}
