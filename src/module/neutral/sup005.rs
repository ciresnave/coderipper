// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-005: one package resolved to many versions in the lockfile.
//!
//! **What this judges (Rust, from `Cargo.lock`):** every tracked `Cargo.lock` outside vendored, fixture and test directories is parsed.
//! A package that the lockfile resolves to **more than three distinct versions** (four or more) is reported, with its versions: one
//! **Info** finding per lockfile, listing the packages and their versions, most versions first. The limit is **chosen from the corpus,
//! not tuned**: of 3,173 package entries in the nine corpus lockfiles, 2,952 resolve to one version, 191 to two, 16 to three, 11 to
//! four, one to five and one to six, so "more than three" reports 13 packages (0.4%) in 3 of the 9 repositories (the Windows
//! `windows-sys` family and `hashbrown` account for most of them). It is a constant here; a `[sup005] max_versions` setting is not
//! read, because settings do not reach the language-neutral module yet.
//!
//! **What it cannot see:** which of the versions can be unified by a compatible requirement (Cargo allows several semver-incompatible
//! versions on purpose), why a duplicate is there (a dependency pins an old one), and the cost of each. The finding names the
//! versions so a person judges. A lockfile that is not valid TOML is named in a note, never read as clean.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

const ID: &str = crate::rule_id::SUP_005;

/// Versions of one package at or below which nothing is reported. Chosen from the corpus (see the module docs).
const MAX_VERSIONS: usize = 3;

const SKIPPED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "target",
    "tests",
    "test",
    "testdata",
    "fixtures",
    "conformance",
];

fn is_lockfile(path: &str) -> bool {
    let mut parts = path.rsplit('/');
    parts.next() == Some("Cargo.lock") && !parts.any(|s| SKIPPED.contains(&s))
}

/// Package name to its distinct versions.
fn versions(text: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let doc: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in doc
        .get("package")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        if let (Some(n), Some(v)) = (
            p.get("name").and_then(|n| n.as_str()),
            p.get("version").and_then(|v| v.as_str()),
        ) {
            out.entry(n.to_string()).or_default().insert(v.to_string());
        }
    }
    Ok(out)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let locks: Vec<&String> = view.paths().iter().filter(|p| is_lockfile(p)).collect();
    if locks.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Cargo.lock".into());
    }
    let mut findings = Vec::new();
    let mut broken = Vec::new();
    for path in locks {
        let Some(text) = view.read(path) else {
            continue;
        };
        let by_name = match versions(&text) {
            Ok(v) => v,
            Err(_) => {
                broken.push(path.clone());
                continue;
            }
        };
        let mut many: Vec<(&String, &BTreeSet<String>)> = by_name
            .iter()
            .filter(|(_, v)| v.len() > MAX_VERSIONS)
            .collect();
        if many.is_empty() {
            continue;
        }
        many.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
        let list: Vec<String> = many
            .iter()
            .map(|(n, v)| {
                format!(
                    "{n} ({} versions: {})",
                    v.len(),
                    v.iter().cloned().collect::<Vec<_>>().join(", ")
                )
            })
            .collect();
        findings.push(
            Finding::new(
                ID,
                Severity::Info,
                Confidence::Medium,
                view.name(),
                format!(
                    "{path} resolves {} package(s) to more than {MAX_VERSIONS} versions each, most of all `{}` ({} versions)",
                    many.len(),
                    many[0].0,
                    many[0].1.len()
                ),
                format!(
                    "Several versions of one package mean several copies in the build and several places to patch a vulnerability. {} Cargo allows semver-incompatible versions side by side on purpose, so some of these cannot be unified; look for the ones a dependency update would remove (`cargo tree -d`). The limit of {MAX_VERSIONS} is chosen from the corpus, not tuned.",
                    list.join("; ")
                ),
            )
            .location(Location::new(path.as_str(), None))
            .subject(path.as_str()),
        );
    }
    if broken.is_empty() {
        Verdict::Findings(findings)
    } else {
        Verdict::Noted(
            findings,
            vec![format!(
                "SUP-005: {} lockfile(s) are not valid TOML and were not read ({})",
                broken.len(),
                broken.join(", ")
            )],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock(entries: &[(&str, &str)]) -> String {
        let mut s = String::from("version = 4\n");
        for (n, v) in entries {
            s += &format!("\n[[package]]\nname = \"{n}\"\nversion = \"{v}\"\n");
        }
        s
    }

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(w) => panic!("not applicable: {w}"),
        }
    }

    #[test]
    fn four_versions_of_one_package_are_reported_with_the_versions_and_three_are_not() {
        let four = lock(&[
            ("syn", "1.0.1"),
            ("syn", "2.0.1"),
            ("syn", "3.0.1"),
            ("syn", "4.0.1"),
            ("serde", "1.0.0"),
        ]);
        let f = found(&[("Cargo.lock", &four)]);
        assert_eq!(f.len(), 1);
        assert!(
            f[0].summary.contains("`syn` (4 versions)"),
            "{}",
            f[0].summary
        );
        assert!(
            f[0].detail.contains("1.0.1, 2.0.1, 3.0.1, 4.0.1"),
            "{}",
            f[0].detail
        );
        assert!(f[0].clone().validate().is_ok());
        let three = lock(&[("syn", "1.0.1"), ("syn", "2.0.1"), ("syn", "3.0.1")]);
        assert!(found(&[("Cargo.lock", &three)]).is_empty());
    }

    #[test]
    fn the_same_version_twice_is_one_version_and_each_lockfile_is_its_own_finding() {
        let dup = lock(&[
            ("a", "1.0.0"),
            ("a", "1.0.0"),
            ("a", "1.0.0"),
            ("a", "1.0.0"),
        ]);
        assert!(found(&[("Cargo.lock", &dup)]).is_empty());
        let many = lock(&[("a", "1"), ("a", "2"), ("a", "3"), ("a", "4")]);
        let f = found(&[
            ("Cargo.lock", &many),
            ("tools/Cargo.lock", &many),
            ("conformance/x/Cargo.lock", &many),
        ]);
        assert_eq!(f.len(), 2, "a fixture lockfile is not judged: {f:?}");
    }

    #[test]
    fn no_lockfile_does_not_apply_and_a_broken_one_is_noted() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
        match judge(&RepoView::from_files(&[("Cargo.lock", "[[package\n")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(notes[0].contains("Cargo.lock"), "{notes:?}");
            }
            _ => panic!("expected a note"),
        }
    }
}
