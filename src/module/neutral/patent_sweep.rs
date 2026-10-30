// SPDX-License-Identifier: MIT OR Apache-2.0
//! `patent-sweep-record`: a project that can be released has a patent sweep on record for the version line it releases.
//!
//! **What this judges (CireSnave, 2026-10-25, first-hand in the PM session: "Ensure a patent sweep has been done of every one of
//! our projects before release."):** the repository declares a version that could be published (the root `Cargo.toml`'s
//! `[package]` version, else `[workspace.package]` version, unless `publish = false`; or the root `package.json`'s `version`,
//! unless `"private": true`), and then:
//! - **Medium:** no tracked file `docs/PATENT-SWEEP.md`.
//! - **Medium:** the file is there, but no version in it is in the same **version line** as the manifest's: `0.9` for any `0.9.x`
//!   (before 1.0 the second number is the major, CireSnave 2026-09-24), `2` for any `2.x.y`. A sweep names the version it covered;
//!   a sweep of `0.8.3` does not cover `0.9.0`.
//! - **Medium:** the record lacks a heading for one of `ideas`, `queries`, `hits`: a file that exists is not a sweep, and an empty
//!   page must not pass for one.
//! - **Medium:** the hits table has a row whose verdict is `unread` (its claims could not be read): an unread hit is not clear, and
//!   counts as a finding until it is read and given a verdict.
//!
//! **What it cannot see:** whether the sweep was any good (it is a due-diligence read, not a legal opinion, and the record says
//! so); whether the project has been published (a project that never will be is opted out with an `[[allow]]` entry carrying the
//! reason, which stays visible, or by `publish = false` / `"private": true`); manifests below the root; `pyproject.toml`.
//! The process is `C:\Projects\PATENT-SWEEP-PROCESS.md`; the record's template is there too.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::PATENT_SWEEP_RECORD;

/// Where the record lives, relative to the repository root.
const RECORD: &str = "docs/PATENT-SWEEP.md";

/// `(manifest, version)` of the root manifest that could be published.
pub(super) fn releasable(view: &RepoView) -> Option<(&'static str, String)> {
    if let Some(text) = view.read("Cargo.toml") {
        if let Ok(doc) = text.parse::<toml::Table>() {
            let package = doc.get("package");
            let publish_off = package.and_then(|p| p.get("publish")).is_some_and(|p| {
                p.as_bool() == Some(false) || p.as_array().is_some_and(|a| a.is_empty())
            });
            let version = package
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    doc.get("workspace")
                        .and_then(|w| w.get("package"))
                        .and_then(|p| p.get("version"))
                        .and_then(|v| v.as_str())
                });
            if let (Some(version), false) = (version, publish_off) {
                return Some(("Cargo.toml", version.to_string()));
            }
        }
    }
    if let Some(text) = view.read("package.json") {
        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) {
            let private = doc.get("private").and_then(|p| p.as_bool()) == Some(true);
            if let (Some(version), false) = (doc.get("version").and_then(|v| v.as_str()), private) {
                return Some(("package.json", version.to_string()));
            }
        }
    }
    None
}

/// The version line of `version`: `0.9` for `0.9.37`, `2` for `2.1.0`; `None` when it does not start with `major.minor`.
pub(super) fn line_of(version: &str) -> Option<String> {
    let mut parts = version
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+']);
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.is_empty() || !major.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if major == "0" {
        minor
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| format!("0.{minor}"))
    } else {
        Some(major.to_string())
    }
}

/// The sections every sweep record has (the template in `PATENT-SWEEP-PROCESS.md`), each as a word a heading contains.
const SECTIONS: &[&str] = &["ideas", "queries", "hits"];

/// The sections of [`SECTIONS`] that no Markdown heading names.
fn missing_sections(text: &str) -> Vec<&'static str> {
    SECTIONS
        .iter()
        .copied()
        .filter(|word| {
            !text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with('#') && t.to_ascii_lowercase().contains(word)
            })
        })
        .collect()
}

/// The 1-based lines of table rows whose verdict cell (the last cell) is `unread`.
fn unread_rows(text: &str) -> Vec<u32> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('|'))
        .filter(|(_, l)| {
            l.trim()
                .trim_end_matches('|')
                .rsplit('|')
                .next()
                .is_some_and(|cell| {
                    cell.trim()
                        .trim_matches(['`', '*', '_'])
                        .eq_ignore_ascii_case("unread")
                })
        })
        .map(|(i, _)| i as u32 + 1)
        .collect()
}

/// Whether `text` names a version in `line`: a whole `0.9` or `0.9.x` token for the line `0.9`; `2.x` or `2.x.y` for the line `2`.
fn names_line(text: &str, line: &str) -> bool {
    let bytes = text.as_bytes();
    let wanted = if line.contains('.') {
        line.to_string()
    } else {
        format!("{line}.")
    };
    let mut from = 0;
    while let Some(at) = text[from..].find(&wanted) {
        let start = from + at;
        let end = start + wanted.len();
        from = start + 1;
        // not inside a longer number (`10.9`, `1.0.9`) ...
        if start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            continue;
        }
        let next = bytes.get(end).copied();
        let ok = if line.contains('.') {
            // `0.9` then not another digit (`0.90` is another line): the end, a dot, or any other character
            !next.is_some_and(|c| c.is_ascii_digit())
        } else {
            // `2.` then a digit: `2.1`, `2.1.0`
            next.is_some_and(|c| c.is_ascii_digit())
        };
        if ok {
            return true;
        }
    }
    false
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let Some((manifest, version)) = releasable(view) else {
        return Verdict::NotApplicable(
            "no root Cargo.toml or package.json declares a version that could be published".into(),
        );
    };
    let project = view.name();
    let Some(line) = line_of(&version) else {
        return Verdict::NotApplicable(format!(
            "{manifest} declares the version {version}, which has no major.minor to name a version line"
        ));
    };
    let advice = format!(
        "A file that exists is not a sweep, and no file is none. Do the patent sweep (C:\\Projects\\PATENT-SWEEP-PROCESS.md): list the project's ideas, search patent sources, read the closest \
         hits claim by claim, and record it in {RECORD} naming the version it covers ({version}, version line {line}). It is a \
         due-diligence read, not a legal opinion. A project that will never be released opts out with an [[allow]] entry and a reason."
    );
    if !view.has(RECORD) {
        return Verdict::Findings(vec![Finding::absence(
            ID,
            Severity::Medium,
            Confidence::High,
            project,
            format!("{manifest} declares version {version}, and the repository has no {RECORD}"),
            advice,
            format!(
                "the search covered the {} tracked files for {RECORD}",
                view.paths().len()
            ),
        )
        .location(Location::new(RECORD, None))
        .subject("patent-sweep-record")]);
    }
    let Some(text) = view.read(RECORD) else {
        return Verdict::Findings(Vec::new());
    };
    let mut found = Vec::new();
    if !names_line(&text, &line) {
        found.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                format!(
                    "{RECORD} does not name a version in the line of {manifest} version {version} ({line})"
                ),
                advice.clone(),
                format!(
                    "{RECORD} was read whole, and no version token in it is in the version line {line}"
                ),
            )
            .location(Location::new(RECORD, Some(1)))
            .subject("patent-sweep-record:version"),
        );
    }
    let missing = missing_sections(&text);
    if !missing.is_empty() {
        found.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                format!(
                    "{RECORD} is not a patent sweep: it has no heading for {}",
                    missing.join(", ")
                ),
                format!(
                    "A file that exists is not a sweep. A sweep records the ideas swept, the queries run (source, query, date) and the \
                     hits read claim by claim, each with a verdict. {advice}"
                ),
                format!(
                    "{RECORD} was read whole, and no Markdown heading names {}",
                    missing.join(", ")
                ),
            )
            .location(Location::new(RECORD, Some(1)))
            .subject("patent-sweep-record:sections"),
        );
    }
    let unread = unread_rows(&text);
    if let Some(first) = unread.first() {
        found.push(
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                format!("{RECORD} has {} hit(s) with the verdict unread", unread.len()),
                format!(
                    "An unread hit is not clear: its claims were not read, so nothing is known about the overlap. Read each (the \
                     independent claims first), then give it `clear`, `design-around` or `needs-review`. {advice}"
                ),
            )
            .location(Location::new(RECORD, Some(*first)))
            .subject("patent-sweep-record:unread"),
        );
    }
    Verdict::Findings(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Verdict {
        judge(&RepoView::from_files(files))
    }

    fn findings(v: Verdict) -> Vec<Finding> {
        match v {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    const CARGO: &str = "[package]\nname = \"a\"\nversion = \"0.9.37\"\n";
    /// The headings a sweep record has.
    const BODY: &str = "## Ideas swept\n## Queries\n## Hits\n";

    #[test]
    fn a_releasable_project_with_no_record_is_a_finding_at_the_record() {
        let f = findings(run(&[("Cargo.toml", CARGO)]));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].check_id, "patent-sweep-record");
        assert_eq!(f[0].location.as_ref().unwrap().file, RECORD);
        assert!(
            f[0].summary.contains("no docs/PATENT-SWEEP.md"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn a_record_that_names_the_version_line_is_clean() {
        for head in [
            "# Patent sweep: a 0.9.37 @ abc\n",
            "# Patent sweep: a v0.9.0 @ abc\n",
            "Covers the 0.9 line.\n",
        ] {
            let text = format!("{head}{BODY}");
            assert!(
                findings(run(&[("Cargo.toml", CARGO), (RECORD, &text)])).is_empty(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_page_that_is_not_a_sweep_is_a_finding_even_when_it_names_the_version() {
        let f = findings(run(&[
            ("Cargo.toml", CARGO),
            (RECORD, "# Patent sweep 0.9.37\ntodo\n"),
        ]));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].summary.contains("is not a patent sweep"),
            "{}",
            f[0].summary
        );
        assert!(f[0].detail.contains("A file that exists is not a sweep"));
    }

    #[test]
    fn an_unread_hit_is_a_finding_at_its_row_and_a_read_one_is_not() {
        let head =
            format!("# Patent sweep 0.9.37\n{BODY}| # | patent | verdict |\n|---|---|---|\n");
        let unread = format!(
            "{head}| 1 | US1 | clear |\n| 2 | US2 | `unread` |\n| 3 | US3 | **Unread** |\n"
        );
        let f = findings(run(&[("Cargo.toml", CARGO), (RECORD, &unread)]));
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].summary.contains("2 hit(s) with the verdict unread"),
            "{}",
            f[0].summary
        );
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(8));
        let read = format!("{head}| 1 | US1 | clear |\n| 2 | US2 | design-around |\n");
        assert!(findings(run(&[("Cargo.toml", CARGO), (RECORD, &read)])).is_empty());
    }

    #[test]
    fn a_record_of_another_line_does_not_cover_this_one() {
        for head in [
            "# Patent sweep: a 0.8.3 @ abc\n",
            "Version 0.90.1 and 10.9.0 and 1.0.9\n",
            "no version at all\n",
        ] {
            let text = format!("{head}{BODY}");
            let f = findings(run(&[("Cargo.toml", CARGO), (RECORD, &text)]));
            assert_eq!(f.len(), 1, "{text:?}");
            assert!(
                f[0].summary.contains("does not name a version"),
                "{}",
                f[0].summary
            );
        }
    }

    #[test]
    fn after_one_point_oh_the_major_is_the_line() {
        let cargo = "[package]\nname = \"a\"\nversion = \"2.4.1\"\n";
        let with = |v: &str| {
            findings(run(&[
                ("Cargo.toml", cargo),
                (RECORD, &format!("swept {v}\n{BODY}")),
            ]))
        };
        assert!(with("2.0.0").is_empty());
        assert_eq!(with("1.9.0").len(), 1);
        assert_eq!(with("12.0.0").len(), 1);
    }

    #[test]
    fn a_project_that_cannot_be_released_is_not_judged() {
        for files in [
            vec![(
                "Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\npublish = false\n",
            )],
            vec![(
                "Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\npublish = []\n",
            )],
            vec![(
                "package.json",
                "{\"name\":\"a\",\"version\":\"1.0.0\",\"private\":true}",
            )],
            vec![("README.md", "no manifest\n")],
        ] {
            assert!(
                matches!(run(&files), Verdict::NotApplicable(_)),
                "{files:?}"
            );
        }
    }

    #[test]
    fn package_json_and_a_workspace_version_are_read() {
        let npm = "{\"name\":\"a\",\"version\":\"1.2.3\"}";
        assert_eq!(findings(run(&[("package.json", npm)])).len(), 1);
        let record = format!("swept 1.0.0\n{BODY}");
        assert!(findings(run(&[("package.json", npm), (RECORD, &record)])).is_empty());
        let ws = "[workspace]\nmembers = [\"a\"]\n[workspace.package]\nversion = \"0.4.0\"\n";
        assert_eq!(findings(run(&[("Cargo.toml", ws)])).len(), 1);
    }
}
