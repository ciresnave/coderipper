//! `licence`: a licence file, a manifest that agrees with it, an SPDX identifier in each source file, and a holder that is not a person.
//!
//! **What this judges (EXPECTATIONS 6.4 and 6.4a, from the repository alone):**
//! - **High, the one blocking finding:** no licence file at the repository root (`LICENSE*`, `LICENCE*`, `COPYING*`, `UNLICENSE*`, or a
//!   `LICENSES/` directory).
//! - **Medium, reported and never blocking** (the owner: these "are not gates that have to be verified before we proceed"): the root
//!   manifest's `license` names an identifier that no licence file is named for (`LICENSE-MIT` for `MIT`; one generic `LICENSE` or
//!   `COPYING` is accepted for any identifier, because its text is not read); source files with no `SPDX-License-Identifier` in
//!   their first lines; a header whose identifier is neither one of the project's nor recorded in a `NOTICE`, `THIRD-PARTY` file or
//!   `LICENSES/` entry (an origin record, 6.4a); a copyright line, in a licence file or a header, that names the owner as a person
//!   rather than the organisation.
//!
//! The per-file findings are one finding for the whole repository (its detail names the first few files and the count), because a
//! finding per file would bury a repository with hundreds of them.
//!
//! **What it cannot see:** the licence text itself (a `LICENSE` file that is not the licence the manifest names); whether a header's
//! identifier is the right one for a copied file (that is a person's check against the origin); the person list is built in (the
//! owner's names), so another owner's personal name is not found; markup and data files carry no header requirement and are not read;
//! the directories the other rules skip (`fixtures`, `vendor`, `node_modules`, ..) are skipped; manifests below the root are not read.

use super::{conflict_marker::SKIPPED, file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use toml::Value;

const ID: &str = crate::rule_id::LICENCE;

/// Source files that must carry a header: the extensions of code, not of data or documents.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "mjs", "cjs", "jsx", "ts", "tsx", "go", "java", "kt", "c", "h", "cc", "cpp",
    "hpp", "cs", "rb", "php", "swift", "sh", "ps1",
];

/// How many lines at the top of a file a header may sit in.
const HEADER_LINES: usize = 15;

/// The names of the owner as a person. The copyright holder is the organisation (6.4); these are the personal forms.
const PERSONAL: &[&str] = &["eric evans", "ciresnave"];

/// Files that are an origin record for a copied file's licence (6.4a).
const ORIGIN_PREFIXES: &[&str] = &["NOTICE", "THIRD-PARTY", "THIRD_PARTY", "THIRDPARTY"];

const NAMED_LICENCE_PREFIXES: &[&str] = &["LICENSE", "LICENCE", "COPYING", "UNLICENSE"];

fn is_source(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    if segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s) || *s == "target")
    {
        return false;
    }
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// The root licence files: a root file named for a licence, and the entries of a root `LICENSES/` directory.
fn licence_files(view: &RepoView) -> Vec<String> {
    view.paths()
        .iter()
        .filter(|p| {
            if let Some(rest) = p.strip_prefix("LICENSES/") {
                return !rest.is_empty();
            }
            !p.contains('/') && {
                let upper = p.to_ascii_uppercase();
                NAMED_LICENCE_PREFIXES.iter().any(|n| upper.starts_with(n))
            }
        })
        .cloned()
        .collect()
}

/// The SPDX identifiers an expression names (`MIT OR Apache-2.0` is two; `WITH` exceptions and operators are not identifiers).
fn expression_ids(expr: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut after_with = false;
    for token in expr.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '/') {
        let token = token.trim().trim_end_matches('+');
        if token.is_empty() {
            continue;
        }
        match token.to_ascii_uppercase().as_str() {
            "OR" | "AND" => after_with = false,
            "WITH" => after_with = true,
            _ if after_with => after_with = false,
            _ => ids.push(token.to_string()),
        }
    }
    ids
}

/// The licence expression the root manifest declares, and the manifest it came from.
fn declared(view: &RepoView) -> Option<(&'static str, String)> {
    if let Some(text) = view.read("Cargo.toml") {
        if let Ok(doc) = toml::from_str::<toml::Table>(&text) {
            let from_package = doc.get("package").and_then(|p| p.get("license"));
            let from_workspace = doc
                .get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("license"));
            if let Some(expr) = from_package.or(from_workspace).and_then(Value::as_str) {
                return Some(("Cargo.toml", expr.to_string()));
            }
        }
    }
    if let Some(text) = view.read("package.json") {
        if let Ok(serde_json::Value::Object(doc)) = serde_json::from_str::<serde_json::Value>(&text)
        {
            if let Some(expr) = doc.get("license").and_then(serde_json::Value::as_str) {
                return Some(("package.json", expr.to_string()));
            }
        }
    }
    if let Some(text) = view.read("pyproject.toml") {
        if let Ok(doc) = toml::from_str::<toml::Table>(&text) {
            let license = doc.get("project").and_then(|p| p.get("license"));
            let expr = license
                .and_then(Value::as_str)
                .or_else(|| license.and_then(|l| l.get("text")).and_then(Value::as_str));
            if let Some(expr) = expr {
                return Some(("pyproject.toml", expr.to_string()));
            }
        }
    }
    None
}

/// Whether a licence file is named for the identifier (`LICENSE-MIT`, `LICENSES/Apache-2.0.txt`), or is one generic file that cannot be
/// told apart by name.
fn file_covers(file: &str, id: &str) -> bool {
    let upper = file_name(file).to_ascii_uppercase();
    let id = id.to_ascii_uppercase();
    // the family: letters before the first hyphen or digit (`APACHE` for `APACHE-2.0`)
    let family: String = id.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    upper.contains(&id) || (!family.is_empty() && upper.contains(&family))
}

fn is_generic(file: &str) -> bool {
    let upper = file_name(file).to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    NAMED_LICENCE_PREFIXES.contains(&stem) && !file.starts_with("LICENSES/")
}

/// The identifiers and copyright lines in the first lines of a file's text.
fn header(text: &str) -> (Vec<String>, Vec<String>) {
    let (mut ids, mut holders) = (Vec::new(), Vec::new());
    for line in text.lines().take(HEADER_LINES) {
        if let Some((_, expr)) = line.split_once("SPDX-License-Identifier:") {
            ids.extend(expression_ids(expr.trim_end_matches("*/").trim()));
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("copyright") {
            holders.push(line.trim().to_string());
        }
    }
    (ids, holders)
}

fn personal(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    PERSONAL.iter().any(|n| lower.contains(n))
}

fn detail_list(paths: &[String]) -> String {
    let shown: Vec<&str> = paths.iter().take(5).map(String::as_str).collect();
    let more = paths.len().saturating_sub(shown.len());
    if more == 0 {
        shown.join(", ")
    } else {
        format!("{}, and {more} more", shown.join(", "))
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let project = view.name();
    let files = licence_files(view);
    if files.is_empty() {
        return Verdict::Findings(vec![Finding::absence(
            ID,
            Severity::High,
            Confidence::High,
            project,
            "the repository has no licence file",
            "Add the licence text at the root: LICENSE-MIT and LICENSE-APACHE for `MIT OR Apache-2.0`, or one LICENSE. Without it a consumer \
             has no grant to rely on (EXPECTATIONS 6.4).",
            format!(
                "the search covered root files named LICENSE*, LICENCE*, COPYING*, UNLICENSE* and a LICENSES/ directory among {} tracked files",
                view.paths().len()
            ),
        )
        .location(Location::new("LICENSE", None))
        .subject("licence file")]);
    }

    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let declared = declared(view);
    let mut project_ids: Vec<String> = Vec::new();

    if let Some((manifest, expr)) = &declared {
        project_ids = expression_ids(expr);
        let generic = files.iter().any(|f| is_generic(f));
        for id in &project_ids {
            if !generic && !files.iter().any(|f| file_covers(f, id)) {
                findings.push(
                    Finding::absence(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        project,
                        format!("{manifest} declares `{expr}` but no licence file is named for {id}"),
                        format!(
                            "The licence files are {}. Add LICENSE-{} (or LICENSES/{id}.txt), or correct the manifest's licence field.",
                            detail_list(&files),
                            id.to_ascii_uppercase().split('-').next().unwrap_or(id)
                        ),
                        format!(
                            "{manifest} was read and names {id}; the {} root licence file(s) searched are {}",
                            files.len(),
                            detail_list(&files)
                        ),
                    )
                    .location(Location::new(*manifest, None))
                    .subject(id.clone()),
                );
            }
        }
    } else {
        notes.push(
            "no root Cargo.toml, package.json or pyproject.toml declares a licence: the manifest and the headers are not compared with one"
                .to_string(),
        );
    }
    // the licence files also say which identifiers are the project's own
    for f in &files {
        if let Some(rest) = f.strip_prefix("LICENSES/") {
            let id = rest.rsplit_once('.').map_or(rest, |(stem, _)| stem);
            project_ids.push(id.to_string());
        }
    }
    let has_origin_record = view.paths().iter().any(|p| {
        let upper = file_name(p).to_ascii_uppercase();
        !p.contains("/") && ORIGIN_PREFIXES.iter().any(|o| upper.starts_with(o))
    });

    // holders named in the licence files
    for f in &files {
        if let Some(text) = view.read(f) {
            if let Some(line) = text
                .lines()
                .find(|l| l.to_ascii_lowercase().contains("copyright") && personal(l))
            {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::High,
                        project,
                        format!("{f} names a person as the copyright holder"),
                        format!(
                            "The holder is the organisation, not a person (EXPECTATIONS 6.4). The line reads: {}",
                            line.trim()
                        ),
                    )
                    .location(Location::new(f.clone(), None))
                    .subject("copyright holder"),
                );
            }
        }
    }

    // headers
    let sources: Vec<String> = view
        .paths()
        .iter()
        .filter(|p| is_source(p))
        .cloned()
        .collect();
    if sources.is_empty() {
        return Verdict::Noted(findings, notes);
    }
    let with_spdx: Option<Vec<String>> = view.paths_matching("SPDX-License-Identifier");
    let mut missing = Vec::new();
    let mut foreign: Vec<(String, String)> = Vec::new();
    let mut personal_headers = Vec::new();
    for path in &sources {
        let has_header = match &with_spdx {
            Some(list) => list.iter().any(|p| p == path),
            None => view.read(path).is_some_and(|t| !header(&t).0.is_empty()),
        };
        if !has_header {
            missing.push(path.clone());
            // a file with no identifier may still name a person in a copyright line
            if let Some(text) = view.read(path) {
                if header(&text).1.iter().any(|l| personal(l)) {
                    personal_headers.push(path.clone());
                }
            }
            continue;
        }
        let Some(text) = view.read(path) else {
            continue;
        };
        let (ids, holders) = header(&text);
        if holders.iter().any(|l| personal(l)) {
            personal_headers.push(path.clone());
        }
        if !project_ids.is_empty() && !has_origin_record {
            if let Some(other) = ids
                .iter()
                .find(|id| !project_ids.iter().any(|p| p.eq_ignore_ascii_case(id)))
            {
                foreign.push((path.clone(), other.clone()));
            }
        }
    }

    if !missing.is_empty() {
        findings.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                format!(
                    "{} of {} source files carry no SPDX-License-Identifier in their first {HEADER_LINES} lines",
                    missing.len(),
                    sources.len()
                ),
                format!(
                    "Add `SPDX-License-Identifier: <expression>` as a comment at the top of each source file (EXPECTATIONS 6.4); the manifest's \
                     licence field is not enough for a consumer who copies one file. Files: {}",
                    detail_list(&missing)
                ),
                format!(
                    "{} of the {} source files read do carry a header, so the search finds one where it exists",
                    sources.len() - missing.len(),
                    sources.len()
                ),
            )
            .location(Location::new(missing[0].clone(), Some(1)))
            .subject("SPDX-License-Identifier"),
        );
    }
    if !foreign.is_empty() {
        let paths: Vec<String> = foreign
            .iter()
            .map(|(p, id)| format!("{p} ({id})"))
            .collect();
        findings.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::Medium,
                project,
                format!(
                    "{} source files declare a licence other than the project's ({}) with no origin record",
                    foreign.len(),
                    project_ids.join(", ")
                ),
                format!(
                    "A file under another licence is third-party: record its origin and keep its licence and notice in a NOTICE or THIRD-PARTY \
                     file (EXPECTATIONS 6.4a), or correct the header. Files: {}",
                    detail_list(&paths)
                ),
                "the root was searched for NOTICE*, THIRD-PARTY* and THIRD_PARTY* files and a LICENSES/ entry for the identifier",
            )
            .location(Location::new(foreign[0].0.clone(), Some(1)))
            .subject("licence mismatch"),
        );
    }
    if !personal_headers.is_empty() {
        findings.push(
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::High,
                project,
                format!("{} source files name a person in a copyright line", personal_headers.len()),
                format!(
                    "The copyright holder is the organisation, not a person (EXPECTATIONS 6.4). Files: {}",
                    detail_list(&personal_headers)
                ),
            )
            .location(Location::new(personal_headers[0].clone(), Some(1)))
            .subject("copyright holder"),
        );
    }
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f
                .into_iter()
                .map(|f| f.validate().unwrap_or_else(|e| panic!("{e}")))
                .collect(),
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    const CARGO: &str =
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense = \"MIT OR Apache-2.0\"\n";

    fn subjects(found: &[Finding]) -> Vec<&str> {
        found
            .iter()
            .map(|f| f.subject.as_deref().unwrap_or(""))
            .collect()
    }

    #[test]
    fn a_repository_with_no_licence_file_is_a_high_finding() {
        let found = run(&[
            ("Cargo.toml", CARGO),
            (
                "src/lib.rs",
                "// SPDX-License-Identifier: MIT OR Apache-2.0\n",
            ),
        ]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(found[0].subject.as_deref(), Some("licence file"));
        assert!(found[0].clone().validate().is_ok());
    }

    #[test]
    fn a_complete_repository_is_clean() {
        let found = run(&[
            ("Cargo.toml", CARGO),
            (
                "LICENSE-MIT",
                "MIT License\n\nCopyright (c) 2026 Thinker's Journal\n",
            ),
            ("LICENSE-APACHE", "Apache License\n"),
            (
                "src/lib.rs",
                "// SPDX-License-Identifier: MIT OR Apache-2.0\nfn a() {}\n",
            ),
            ("README.md", "x"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn any_licence_file_is_found_at_the_root_only() {
        for name in [
            "LICENSE",
            "LICENSE.md",
            "LICENCE.txt",
            "COPYING",
            "UNLICENSE",
            "LICENSES/MIT.txt",
        ] {
            let found = run(&[(name, "text")]);
            assert!(
                found.iter().all(|f| f.severity != Severity::High),
                "{name}: {found:?}"
            );
        }
        let found = run(&[("docs/LICENSE", "text")]);
        assert_eq!(found[0].severity, Severity::High);
    }

    #[test]
    fn a_manifest_licence_with_no_file_for_it_is_medium() {
        let found = run(&[("Cargo.toml", CARGO), ("LICENSE-MIT", "MIT")]);
        assert_eq!(subjects(&found), vec!["Apache-2.0"]);
        assert_eq!(found[0].severity, Severity::Medium);
        // one generic file cannot be told apart by name, so it is accepted
        assert!(run(&[("Cargo.toml", CARGO), ("LICENSE", "text")]).is_empty());
        // a LICENSES/ entry is named for its identifier
        assert!(run(&[
            ("Cargo.toml", CARGO),
            ("LICENSES/MIT.txt", "t"),
            ("LICENSES/Apache-2.0.txt", "t")
        ])
        .is_empty());
    }

    #[test]
    fn package_json_and_pyproject_declare_a_licence_too() {
        let found = run(&[
            ("package.json", "{\"license\":\"ISC\"}"),
            ("LICENSE-MIT", "t"),
        ]);
        assert_eq!(subjects(&found), vec!["ISC"]);
        let found = run(&[
            (
                "pyproject.toml",
                "[project]\nlicense = {text = \"BSD-3-Clause\"}\n",
            ),
            ("LICENSE-MIT", "t"),
        ]);
        assert_eq!(subjects(&found), vec!["BSD-3-Clause"]);
    }

    #[test]
    fn source_files_without_a_header_are_one_finding_naming_the_count() {
        let found = run(&[
            ("LICENSE", "t"),
            ("a.rs", "fn a() {}\n"),
            ("b.py", "print(1)\n"),
            ("c.rs", "// SPDX-License-Identifier: MIT\n"),
            ("notes.md", "not source"),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].summary.starts_with("2 of 3 source files"),
            "{}",
            found[0].summary
        );
        assert_eq!(found[0].severity, Severity::Medium);
        assert_eq!(found[0].location.as_ref().unwrap().file, "a.rs");
    }

    #[test]
    fn a_header_below_the_first_lines_does_not_count_in_the_fallback_but_fixtures_and_target_are_skipped(
    ) {
        let found = run(&[
            ("LICENSE", "t"),
            ("tests/fixtures/a.rs", "fn a() {}\n"),
            ("target/x.rs", "fn a() {}\n"),
            ("vendor/y.js", "x"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_header_with_another_licence_needs_an_origin_record() {
        let files = [
            ("Cargo.toml", CARGO),
            ("LICENSE", "t"),
            ("src/a.rs", "// SPDX-License-Identifier: GPL-3.0-only\n"),
        ];
        let found = run(&files);
        assert_eq!(subjects(&found), vec!["licence mismatch"]);
        let mut with_notice = files.to_vec();
        with_notice.push(("NOTICE", "a.rs is from upstream, GPL-3.0-only"));
        assert!(run(&with_notice).is_empty());
    }

    #[test]
    fn a_person_as_holder_is_found_in_the_licence_and_in_a_header() {
        let found = run(&[
            ("LICENSE-MIT", "MIT\n\nCopyright (c) 2026 Eric Evans\n"),
            (
                "a.rs",
                "// SPDX-License-Identifier: MIT\n// Copyright 2026 CireSnave\n",
            ),
        ]);
        assert_eq!(
            subjects(&found),
            vec!["copyright holder", "copyright holder"]
        );
        assert!(found.iter().all(|f| f.severity == Severity::Medium));
    }

    #[test]
    fn expressions_split_into_identifiers() {
        assert_eq!(
            expression_ids("MIT OR Apache-2.0"),
            vec!["MIT", "Apache-2.0"]
        );
        assert_eq!(
            expression_ids("(MIT AND BSD-3-Clause)"),
            vec!["MIT", "BSD-3-Clause"]
        );
        assert_eq!(
            expression_ids("GPL-2.0+ WITH Classpath-exception-2.0"),
            vec!["GPL-2.0"]
        );
    }
}
