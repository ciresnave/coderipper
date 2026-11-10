// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-009: every Prometheus alert carries a runbook link.
//!
//! **What this judges:** each tracked Prometheus rule file (a file named `*.rules.yml`, `*.rules.yaml`, `alerts*.yml`,
//! `alerts*.yaml`, `alert_rules*.yml` or `alert_rules*.yaml`, outside vendored directories and test directories) is parsed as YAML.
//! Under `groups[].rules[]`, each rule that has an `alert` key is an alert; it needs a non-empty `annotations.runbook_url`. One **Info**
//! finding per alert without one, at the line of its `alert:` key. A file that holds recording rules only (`record:`) has no alerts and
//! is not judged.
//!
//! **Applies to:** a repository with at least one such file that holds an alert; otherwise `not applicable`.
//!
//! **What it cannot see:** alert rules in a Helm template, a Jsonnet or `PrometheusRule` custom resource (a Kubernetes manifest is not
//! read), a runbook linked from `description` or `summary` text instead of the `runbook_url` annotation, a rule file under another
//! name, and whether the link works. A file that is not valid YAML is named in a note, never read as clean.

use super::{file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::OPS_009;

const SKIPPED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "tests",
    "test",
    "testdata",
    "fixtures",
];

fn is_rule_file(path: &str) -> bool {
    if path.split('/').rev().skip(1).any(|s| SKIPPED.contains(&s)) {
        return false;
    }
    let name = file_name(path).to_ascii_lowercase();
    let yaml = name.ends_with(".yml") || name.ends_with(".yaml");
    yaml && (name.contains(".rules.")
        || name.starts_with("alerts")
        || name.starts_with("alert_rules"))
}

struct Alert {
    name: String,
    has_runbook: bool,
}

fn alerts_of(text: &str) -> Result<Vec<Alert>, String> {
    let doc: yaml_serde::Value = yaml_serde::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let Some(groups) = doc.get("groups").and_then(|g| g.as_sequence()) else {
        return Ok(out);
    };
    for group in groups {
        let Some(rules) = group.get("rules").and_then(|r| r.as_sequence()) else {
            continue;
        };
        for rule in rules {
            let Some(name) = rule.get("alert").and_then(|a| a.as_str()) else {
                continue;
            };
            let has_runbook = rule
                .get("annotations")
                .and_then(|a| a.get("runbook_url"))
                .and_then(|u| u.as_str())
                .is_some_and(|u| !u.trim().is_empty());
            out.push(Alert {
                name: name.to_string(),
                has_runbook,
            });
        }
    }
    Ok(out)
}

/// The 1-based line of `alert: NAME` (quoted or not) in `text`, or the first line.
fn line_of(text: &str, name: &str) -> u32 {
    text.lines()
        .position(|l| {
            let t = l.trim_start().trim_start_matches("- ").trim_start();
            t.strip_prefix("alert:")
                .is_some_and(|rest| rest.trim().trim_matches(|c| c == '"' || c == '\'') == name)
        })
        .map_or(1, |i| u32::try_from(i + 1).unwrap_or(1))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut findings = Vec::new();
    let (mut files, mut alerts, mut with_link) = (0usize, 0usize, 0usize);
    let mut broken: Vec<String> = Vec::new();
    for path in view.paths().iter().filter(|p| is_rule_file(p)) {
        let Some(text) = view.read(path) else {
            continue;
        };
        let found = match alerts_of(&text) {
            Ok(a) => a,
            Err(_) => {
                broken.push(path.clone());
                continue;
            }
        };
        if found.is_empty() {
            continue;
        }
        files += 1;
        for alert in found {
            alerts += 1;
            if alert.has_runbook {
                with_link += 1;
                continue;
            }
            let line = line_of(&text, &alert.name);
            findings.push((path.clone(), line, alert.name));
        }
    }
    if files == 0 && broken.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Prometheus rule file (*.rules.yml, alerts*.yml) that holds an alert".into(),
        );
    }
    let out: Vec<Finding> = findings
        .into_iter()
        .map(|(path, line, name)| {
            Finding::absence(
                ID,
                Severity::Info,
                Confidence::Medium,
                view.name(),
                format!("the alert `{name}` has no `runbook_url` annotation"),
                "An alert that pages someone should say what to do. Add `annotations: runbook_url: <link>` to the rule, pointing at the steps for this alert. A runbook named only in `description` or `summary` text is not seen.",
                format!(
                    "{alerts} alert(s) in {files} rule file(s) were read; {with_link} of them carry a runbook_url, so the annotation is read where it exists"
                ),
            )
            .location(Location::new(path.as_str(), Some(line)))
            .subject(name)
        })
        .collect();
    if broken.is_empty() {
        Verdict::Findings(out)
    } else {
        Verdict::Noted(
            out,
            vec![format!(
                "OPS-009: {} rule file(s) are not valid YAML and were not read ({})",
                broken.len(),
                broken.join(", ")
            )],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = "groups:\n  - name: app\n    rules:\n      - alert: HighErrors\n        expr: rate(errors[5m]) > 1\n        annotations:\n          summary: x\n      - alert: Down\n        expr: up == 0\n        annotations:\n          runbook_url: https://wiki/down\n      - record: job:up:sum\n        expr: sum(up)\n";

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(w) => panic!("not applicable: {w}"),
        }
    }

    #[test]
    fn an_alert_without_a_runbook_url_is_a_finding_at_its_line_and_one_with_it_is_not() {
        let f = found(&[("monitoring/app.rules.yml", RULES)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("HighErrors"));
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
        assert!(f[0].clone().validate().is_ok());
        assert!(f[0]
            .positive_control
            .as_deref()
            .unwrap()
            .contains("2 alert(s) in 1 rule file(s)"));
    }

    #[test]
    fn an_empty_runbook_url_does_not_count() {
        let text = RULES.replace("https://wiki/down", "\"\"");
        assert_eq!(found(&[("alerts.yml", &text)]).len(), 2);
    }

    #[test]
    fn recording_rules_only_and_other_files_are_not_judged() {
        let record = "groups:\n  - name: r\n    rules:\n      - record: a\n        expr: b\n";
        assert!(matches!(
            judge(&RepoView::from_files(&[
                ("x.rules.yml", record),
                ("docker-compose.yml", "services: {}\n")
            ])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[("tests/alerts.yml", RULES)])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn a_file_that_is_not_yaml_is_noted_not_clean() {
        let view = RepoView::from_files(&[("a.rules.yml", "groups: [\n"), ("b.rules.yml", RULES)]);
        match judge(&view) {
            Verdict::Noted(f, notes) => {
                assert_eq!(f.len(), 1);
                assert!(notes[0].contains("a.rules.yml"), "{notes:?}");
            }
            _ => panic!("expected a note"),
        }
    }

    #[test]
    fn a_quoted_alert_name_is_found_at_its_line() {
        let text = "groups:\n- name: g\n  rules:\n  - alert: \"Quoted\"\n    expr: up\n";
        let f = found(&[("a.rules.yaml", text)]);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
    }
}
