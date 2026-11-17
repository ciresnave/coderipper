// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-007: every flag of a flag registry has an owner and a sunset date.
//!
//! **What this judges (any language, from the tracked files alone):** a *flag registry* is a tracked file named `flags`,
//! `feature-flags`, `feature_flags`, `featureflags`, `feature-toggles`, `feature_toggles` or `toggles` with the extension `.yml`, `.yaml`,
//! `.json` or `.toml`, outside vendored directories, tests, fixtures and examples. Its entries are read: the items of the list or the map
//! under a top-level `flags`, `feature_flags`, `features` or `toggles` key, or else of the file's own top-level list or map (a map
//! entry that is not itself a map, `new_ui: true`, is a flag with nothing recorded). An entry is reported when it records no **owner**
//! (a non-empty `owner`, `owners`, `maintainer`, `team`, `responsible`, `contact` or `assignee`) or no **sunset** (a non-empty `sunset`,
//! `expires`, `expiry`, `expiration`, `expire`, `remove_by`, `removal_date`, `end_date`, `retire_by`, `delete_by` or `remove_after`); keys are
//! compared in lower case with `-` read as `_`. One **Low**-severity, Medium-confidence finding per flag, at the line that names it.
//!
//! **What it cannot see:** a flag declared in code or in a service (LaunchDarkly, Unleash) with no file here, a registry under another
//! name, whether the owner is a real person or the date is still in the future (that needs a clock and history), and whether a finished
//! rollout's flag was deleted. A registry file that does not parse is named in a note, never read as clean. A repository with no registry
//! file is not applicable.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use serde_json::Value;

const ID: &str = crate::rule_id::OPS_007;

const STEMS: &[&str] = &[
    "flags",
    "feature-flags",
    "feature_flags",
    "featureflags",
    "feature-toggles",
    "feature_toggles",
    "toggles",
];
const LISTS: &[&str] = &["flags", "feature_flags", "features", "toggles"];
const OWNER: &[&str] = &[
    "owner",
    "owners",
    "maintainer",
    "team",
    "responsible",
    "contact",
    "assignee",
];
const SUNSET: &[&str] = &[
    "sunset",
    "expires",
    "expiry",
    "expiration",
    "expire",
    "remove_by",
    "removal_date",
    "end_date",
    "retire_by",
    "delete_by",
    "remove_after",
];

fn skipped(path: &str) -> bool {
    path.split('/').any(|s| {
        matches!(
            s.to_lowercase().as_str(),
            "vendor"
                | "node_modules"
                | "target"
                | "third_party"
                | "tests"
                | "test"
                | "__tests__"
                | "fixtures"
                | "fixture"
                | "testdata"
                | "examples"
        )
    })
}

fn is_registry(path: &str) -> bool {
    if skipped(path) {
        return false;
    }
    let leaf = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let Some((stem, ext)) = leaf.rsplit_once('.') else {
        return false;
    };
    matches!(ext, "yml" | "yaml" | "json" | "toml") && STEMS.contains(&stem)
}

fn parse(path: &str, text: &str) -> Option<Value> {
    if path.to_lowercase().ends_with(".json") {
        serde_json::from_str(text).ok()
    } else if path.to_lowercase().ends_with(".toml") {
        toml::from_str(text).ok()
    } else {
        yaml_serde::from_str(text).ok()
    }
}

fn has(entry: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    entry.iter().any(|(k, v)| {
        keys.contains(&k.to_lowercase().replace('-', "_").as_str())
            && match v {
                Value::Null => false,
                Value::String(s) => !s.trim().is_empty(),
                Value::Array(a) => !a.is_empty(),
                _ => true,
            }
    })
}

/// `(name, entry)` of each flag the file records; an entry that is not a map records nothing.
fn entries(root: &Value) -> Vec<(String, serde_json::Map<String, Value>)> {
    let body = match root {
        Value::Object(m) => m
            .iter()
            .find(|(k, v)| LISTS.contains(&k.to_lowercase().as_str()) && !v.is_null())
            .map_or(root, |(_, v)| v),
        _ => root,
    };
    let mut out = Vec::new();
    match body {
        Value::Array(a) => {
            for (i, item) in a.iter().enumerate() {
                if let Value::Object(m) = item {
                    let name = ["name", "key", "id", "flag"]
                        .iter()
                        .find_map(|k| m.get(*k).and_then(Value::as_str))
                        .map_or_else(|| format!("#{}", i + 1), str::to_string);
                    out.push((name, m.clone()));
                }
            }
        }
        Value::Object(m) => {
            for (name, v) in m {
                let entry = match v {
                    Value::Object(e) => e.clone(),
                    _ => serde_json::Map::new(),
                };
                out.push((name.clone(), entry));
            }
        }
        _ => {}
    }
    out
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view.paths().iter().filter(|p| is_registry(p)).collect();
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no flag registry file (flags, feature-flags, feature_flags, feature-toggles or toggles, as .yml, .yaml, .json or .toml)".into(),
        );
    }
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let mut read = 0usize;
    for path in &files {
        let parsed = view
            .read(path)
            .and_then(|t| parse(path, &t).map(|v| (t, v)));
        let Some((text, root)) = parsed else {
            notes.push(format!(
                "{ID} not read: {path} could not be read or parsed as a flag registry, so its flags are not judged"
            ));
            continue;
        };
        read += 1;
        for (name, entry) in entries(&root) {
            let no_owner = !has(&entry, OWNER);
            let no_sunset = !has(&entry, SUNSET);
            if !(no_owner || no_sunset) {
                continue;
            }
            let line = text
                .lines()
                .position(|l| l.contains(name.as_str()))
                .map_or(1, |i| i + 1);
            let missing = match (no_owner, no_sunset) {
                (true, true) => "an owner and a sunset date",
                (true, false) => "an owner",
                _ => "a sunset date",
            };
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!("flag `{name}` in {path} records no {missing}"),
                    "A flag nobody owns and nothing retires multiplies code paths and ends up as accidental configuration. Record who is responsible for it and the date it is to be removed, and delete it with its dead branch when the rollout is done.",
                )
                .location(Location::new(path.as_str(), Some(u32::try_from(line).unwrap_or(1))))
                .subject(format!("{path}:{name}"))
                .positive_control(format!(
                    "{read} flag registry file(s) were parsed with the same reader that looks for an owner key and a sunset key in each entry"
                )),
            );
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    if notes.is_empty() {
        Verdict::Findings(findings)
    } else {
        Verdict::Noted(findings, notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &str, text: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[(path, text)])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_flag_without_an_owner_or_a_sunset_is_reported_for_what_it_lacks() {
        let yaml = "flags:
  - name: new_checkout
    owner: payments
    expires: 2027-01-31
  - name: dark_mode
    owner: web
  - name: beta_api
    expires: 2027-03-01
  - name: old_thing
";
        let f = found("flags.yaml", yaml);
        let subjects: Vec<&str> = f.iter().map(|x| x.subject.as_deref().unwrap()).collect();
        assert_eq!(
            subjects,
            [
                "flags.yaml:dark_mode",
                "flags.yaml:beta_api",
                "flags.yaml:old_thing"
            ]
        );
        assert!(f[0].summary.contains("a sunset date"));
        assert!(f[1].summary.contains("an owner"));
        assert!(f[2].summary.contains("an owner and a sunset date"));
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(5));
    }

    #[test]
    fn a_map_of_flags_json_and_toml_are_read_and_a_bare_boolean_records_nothing() {
        let json = r#"{"new_ui": true, "kept": {"owner": "a", "sunset": "2027-01-01"}}"#;
        let f = found("config/feature-flags.json", json);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].subject.as_deref().unwrap().ends_with(":new_ui"));
        let toml = "[x]\nowner = \"a\"\nexpires = \"2027-01-01\"\n[y]\nteam = \"b\"\n";
        let f = found("feature_flags.toml", toml);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].subject.as_deref().unwrap().ends_with(":y"));
    }

    #[test]
    fn no_registry_is_not_applicable_and_an_unparsable_one_is_noted() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("src/lib.rs", "")])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[("flags.json", "{ not json")])),
            Verdict::Noted(_, _)
        ));
        // a registry in a fixture directory is not read
        assert!(matches!(
            judge(&RepoView::from_files(&[(
                "tests/fixtures/flags.yml",
                "a: 1\n"
            )])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("flags.yml", "- name: x\n") {
            f.validate().unwrap();
        }
    }
}
