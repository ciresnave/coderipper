// SPDX-License-Identifier: MIT OR Apache-2.0
//! `version-and-changelog`: a branch that changes the project changes its version and says so in the changelog.
//!
//! **What this judges (CireSnave, 2026-09-17: "If a change is made and pushed, the version should change in some way. If it is a
//! breaking change, the major version should change."; PM ruling 2026-10-21: the check is on pull requests into the default branch),
//! from the tree at `HEAD` against the tree the branch started from:**
//! - **Medium:** the version in the root `Cargo.toml` (`[package]`, else `[workspace.package]`) or `package.json` is the one the base
//!   had.
//! - **Medium:** a version that did change (or a manifest the base did not have) has no entry in the root `CHANGELOG*` / `CHANGES*`
//!   file: a heading line, or a line that begins with the version, naming it as a whole token (`0.8.1` is not an entry for `0.8.12`,
//!   and `1.0.0-rc.1` is not one for `1.0.0`). No changelog file at all is the same finding.
//!
//! **The base** is the merge-base of `HEAD` with `origin/HEAD`, else `origin/main`, else `origin/master`. The rule does not apply
//! (a coverage gap, never "clean") when none of them exists, when the history needed for the merge-base is not there (a shallow
//! clone: fetch with `fetch-depth: 0`), or when `HEAD` is the base (a run on the default branch compares nothing).
//!
//! **What it cannot see:** the `version-deferred` label (a forge fact; the PM's gate allocates the number and re-checks at merge, so a
//! deferred bump is waived with an `[[allow]]` entry or read as advisory); whether a breaking change moved the right component;
//! manifests below the root; `pyproject.toml`; lock files (never a version's home); a changelog entry that names the version but
//! says nothing.

use super::{Base, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::VERSION_AND_CHANGELOG;

const MANIFESTS: &[&str] = &["Cargo.toml", "package.json"];

/// The version a manifest declares: Cargo's `[package] version`, else `[workspace.package] version`; npm's `version`.
fn version_in(path: &str, text: &str) -> Option<String> {
    if path == "Cargo.toml" {
        let doc: toml::Table = text.parse().ok()?;
        let own = doc
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str());
        let shared = || {
            doc.get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
        };
        return own.or_else(shared).map(str::to_string);
    }
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("version")?.as_str().map(str::to_string)
}

/// The 1-based line that declares `version` in a manifest, or 1.
fn version_line(path: &str, text: &str, version: &str) -> u32 {
    let key = if path == "Cargo.toml" {
        "version"
    } else {
        "\"version\""
    };
    text.lines()
        .position(|l| l.trim_start().starts_with(key) && l.contains(version))
        .map_or(1, |i| i as u32 + 1)
}

/// Whether `line` names `version` as a whole token: not inside a longer number (`10.8.12`, `0.8.120`, `0.8.12.1`) and not the
/// release part of a pre-release or build version (`0.8.12-rc.1`).
fn names(line: &str, version: &str) -> bool {
    let mut from = 0;
    while let Some(at) = line[from..].find(version) {
        let start = from + at;
        let end = start + version.len();
        from = end;
        let before = line[..start].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_digit() || c == '.') {
            continue;
        }
        let mut after = line[end..].chars();
        let glued = match after.next() {
            Some(c) if c.is_ascii_digit() => true,
            Some('.') => after.next().is_some_and(|d| d.is_ascii_digit()),
            Some('-' | '+') => after.next().is_some_and(|d| d.is_ascii_alphanumeric()),
            _ => false,
        };
        if !glued {
            return true;
        }
    }
    false
}

/// The 1-based line of the changelog entry for `version`: a Markdown heading naming it, or a line that begins with it
/// (`0.8.12`, `v0.8.12`, `[0.8.12]`).
fn entry_line(text: &str, version: &str) -> Option<u32> {
    text.lines()
        .position(|l| {
            let t = l.trim_start();
            let heading = t.starts_with('#')
                || t.trim_start_matches(['[', 'v', 'V', '*'])
                    .starts_with(version);
            heading && names(l, version)
        })
        .map(|i| i as u32 + 1)
}

/// The root changelog file: the first tracked root file named `CHANGELOG*` or `CHANGES*`, in any case.
fn changelog(view: &RepoView) -> Option<String> {
    view.paths()
        .iter()
        .find(|p| {
            let upper = p.to_ascii_uppercase();
            !p.contains('/') && (upper.starts_with("CHANGELOG") || upper.starts_with("CHANGES"))
        })
        .cloned()
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let base = match view.base() {
        Base::Tree(base) => base,
        Base::SameAsHead(why) | Base::Gap(why) => return Verdict::NotApplicable(why),
    };
    let project = view.name();
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let mut judged = 0usize;
    let log = changelog(view);
    let log_text = log.as_deref().and_then(|p| view.read(p));
    for path in MANIFESTS {
        let Some(text) = view.read(path) else {
            continue;
        };
        let Some(now) = version_in(path, &text) else {
            continue;
        };
        judged += 1;
        let line = version_line(path, &text, &now);
        let then = base.read(path).and_then(|t| version_in(path, &t));
        if then.as_deref() == Some(now.as_str()) {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::High,
                    project,
                    format!(
                        "{path} still has version {now}, the version of {}",
                        base.how()
                    ),
                    "A change that is pushed changes the version: a patch bump for a fix, a minor for a feature, and for a breaking change the \
                     major (pre-1.0, the second number) (EXPECTATIONS 6.6). The PM may allocate the number at gate time: say so on the pull \
                     request.",
                )
                .location(Location::new(*path, Some(line)))
                .subject(format!("{path}:version")),
            );
            continue;
        }
        if log_text
            .as_deref()
            .and_then(|t| entry_line(t, &now))
            .is_some()
        {
            continue;
        }
        let covered = match &log {
            Some(file) => format!(
                "{file} was read whole, and no heading line or line beginning with the version names {now} as a whole token"
            ),
            None => format!(
                "the {} tracked files include no root file named CHANGELOG* or CHANGES*",
                view.paths().len()
            ),
        };
        let place = log.clone().unwrap_or_else(|| "CHANGELOG.md".to_string());
        findings.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                match &log {
                    Some(file) => format!("{file} has no entry for {path} version {now}"),
                    None => format!("the repository has no changelog for {path} version {now}"),
                },
                format!(
                    "Add a heading for {now} to {place} that says what changed (CHANGELOG.md, `## {now} - <date>`); the version is the \
                     reader's key into it."
                ),
                covered,
            )
            .location(Location::new(place, if log.is_some() { Some(1) } else { None }))
            .subject(format!("{path}:changelog")),
        );
    }
    if judged == 0 {
        return Verdict::NotApplicable(
            "no root Cargo.toml or package.json declares a version to compare".into(),
        );
    }
    notes.push(format!(
        "{judged} manifest version(s) compared with {}",
        base.how()
    ));
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::neutral::RepoView;

    const CARGO_OLD: &str = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n";
    const CARGO_NEW: &str = "[package]\nname = \"a\"\nversion = \"0.2.0\"\n";
    const LOG: &str = "# Changelog\n\n## 0.2.0 - 2026-10-22\n\nthings\n\n## 0.1.0\n";

    fn run(now: &[(&str, &str)], then: Option<&[(&str, &str)]>) -> Verdict {
        judge(&RepoView::from_files_with_base(now, then))
    }

    fn findings(v: Verdict) -> Vec<Finding> {
        match v {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    #[test]
    fn a_bumped_version_with_an_entry_is_clean() {
        let v = run(
            &[("Cargo.toml", CARGO_NEW), ("CHANGELOG.md", LOG)],
            Some(&[("Cargo.toml", CARGO_OLD)]),
        );
        assert!(findings(v).is_empty());
    }

    #[test]
    fn the_same_version_as_the_base_is_reported_at_its_line() {
        let v = findings(run(
            &[("Cargo.toml", CARGO_OLD), ("CHANGELOG.md", LOG)],
            Some(&[("Cargo.toml", CARGO_OLD)]),
        ));
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].location.as_ref().and_then(|l| l.line), Some(3));
        assert!(v[0].summary.contains("still has version 0.1.0"));
    }

    #[test]
    fn a_new_version_with_no_entry_is_reported_and_a_missing_file_is_the_same_finding() {
        let with_file = findings(run(
            &[
                ("Cargo.toml", CARGO_NEW),
                ("CHANGELOG.md", "# Changelog\n\n## 0.1.0\n"),
            ],
            Some(&[("Cargo.toml", CARGO_OLD)]),
        ));
        assert_eq!(with_file.len(), 1, "{with_file:?}");
        assert!(with_file[0].summary.contains("has no entry for"));
        let no_file = findings(run(
            &[("Cargo.toml", CARGO_NEW)],
            Some(&[("Cargo.toml", CARGO_OLD)]),
        ));
        assert_eq!(no_file.len(), 1, "{no_file:?}");
        assert!(no_file[0].summary.contains("has no changelog"));
    }

    #[test]
    fn an_entry_must_name_the_whole_version() {
        for (log, ok) in [
            ("## 0.2.0\n", true),
            ("## [0.2.0] - 2026-10-22\n", true),
            ("v0.2.0 (proposed)\n", true),
            ("## 0.2.0-rc.1\n", false),
            ("## 10.2.0\n", false),
            ("## 0.2.01\n", false),
            ("## 0.2.0.1\n", false),
            ("prose that mentions 0.2.0 in passing\n", false),
            ("## Unreleased\n", false),
        ] {
            assert_eq!(entry_line(log, "0.2.0").is_some(), ok, "{log:?}");
        }
    }

    #[test]
    fn package_json_and_a_workspace_version_are_read() {
        assert_eq!(
            version_in("package.json", "{\"name\":\"a\",\"version\":\"1.2.3\"}").as_deref(),
            Some("1.2.3")
        );
        let ws = "[workspace]\nmembers = [\"a\"]\n[workspace.package]\nversion = \"0.9.0\"\n";
        assert_eq!(version_in("Cargo.toml", ws).as_deref(), Some("0.9.0"));
        assert_eq!(
            version_in("Cargo.toml", "[workspace]\nmembers = [\"a\"]\n"),
            None
        );
    }

    #[test]
    fn a_manifest_the_base_did_not_have_still_needs_an_entry() {
        let v = findings(run(
            &[("Cargo.toml", CARGO_NEW), ("CHANGELOG.md", "# Changelog\n")],
            Some(&[("README.md", "x")]),
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn no_base_is_a_gap_and_a_repository_without_a_version_is_not_judged() {
        assert!(matches!(
            run(&[("Cargo.toml", CARGO_NEW)], None),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            run(&[("README.md", "x")], Some(&[("README.md", "y")])),
            Verdict::NotApplicable(_)
        ));
    }
}
