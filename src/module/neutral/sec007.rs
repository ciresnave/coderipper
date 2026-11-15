// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-007 (a threat model exists) and REL-007 (a failure model exists): two presence checks over the names of tracked files.
//!
//! **What this judges:** that the repository tracks a document of each kind, nothing about what it says (the finding says so).
//!
//! - `SEC-007`: a tracked file whose name starts with `threat-model`, `threat_model`, `threatmodel` or `threat-model` in any case
//!   (`THREAT-MODEL.md`, `docs/threat-model.md`), or any file under `docs/` or `doc/` whose name starts with `threat`.
//! - `REL-007`: a tracked file whose name starts with `failure-model`, `failure_model`, `failuremodel`, `failure-modes`,
//!   `failure_modes` or `failuremodes` in any case, or a Markdown heading that contains `failure model` or `failure mode` in an
//!   architecture document (`ARCHITECTURE.md`, `docs/ARCHITECTURE.md`, `docs/architecture*.md`, `docs/design*.md`).
//!
//! **Applies to:** a repository that tracks a project manifest (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `pom.xml`,
//! `build.gradle`, `build.gradle.kts`, `setup.py`, `composer.json`, `Gemfile`, a `.csproj`): a repository with no code has no system to
//! model, and is `not applicable`.
//!
//! **What it cannot see:** a threat model or a failure model kept outside the repository (a wiki, a design tool), under another name
//! (`security-design.md`, `risks.md`), or inside a document that is not the architecture document; whether a document that exists
//! is a model at all. One **Info** finding per rule, with a positive control (the files searched and a manifest that was found).

use super::{file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const THREAT: &str = crate::rule_id::SEC_007;
const FAILURE: &str = crate::rule_id::REL_007;

const THREAT_NAMES: &[&str] = &["threat-model", "threat_model", "threatmodel"];
const FAILURE_NAMES: &[&str] = &[
    "failure-model",
    "failure_model",
    "failuremodel",
    "failure-modes",
    "failure_modes",
    "failuremodes",
];

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
];

/// A tracked manifest of a project, if there is one: the file that shows the repository holds a system.
fn a_manifest(view: &RepoView) -> Option<String> {
    view.paths()
        .iter()
        .find(|p| {
            let name = file_name(p);
            MANIFESTS.contains(&name) || name.ends_with(".csproj")
        })
        .cloned()
}

fn starts_with_any(name: &str, prefixes: &[&str]) -> bool {
    let lower = name.to_ascii_lowercase();
    prefixes.iter().any(|p| lower.starts_with(p))
}

fn in_docs(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("docs/") || lower.starts_with("doc/") || lower.contains("/docs/")
}

fn has_threat_model(view: &RepoView) -> Option<String> {
    view.paths()
        .iter()
        .find(|p| {
            let name = file_name(p);
            starts_with_any(name, THREAT_NAMES)
                || (in_docs(p) && name.to_ascii_lowercase().starts_with("threat"))
        })
        .cloned()
}

fn is_architecture_doc(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = file_name(&lower).to_string();
    if !name.ends_with(".md") {
        return false;
    }
    name.starts_with("architecture")
        || (in_docs(path) && (name.starts_with("design") || name.starts_with("architecture")))
}

fn has_failure_model(view: &RepoView) -> Option<String> {
    if let Some(p) = view
        .paths()
        .iter()
        .find(|p| starts_with_any(file_name(p), FAILURE_NAMES))
    {
        return Some(p.clone());
    }
    for path in view.paths().iter().filter(|p| is_architecture_doc(p)) {
        let Some(text) = view.read(path) else {
            continue;
        };
        let titled = text.lines().any(|l| {
            let t = l.trim_start();
            if !t.starts_with('#') {
                return false;
            }
            let lower = t.to_ascii_lowercase();
            lower.contains("failure model") || lower.contains("failure mode")
        });
        if titled {
            return Some(path.clone());
        }
    }
    None
}

fn missing(
    id: &str,
    view: &RepoView,
    manifest: &str,
    what: &str,
    names: &str,
    advice: &str,
) -> Finding {
    Finding::absence(
        id,
        Severity::Info,
        Confidence::Low,
        view.name(),
        format!("the repository tracks no {what}"),
        format!(
            "{advice} This check reads names and headings only, so it never judges a document's content, and a {what} kept outside the \
             repository (a wiki, a design tool) is not seen."
        ),
        format!(
            "{} tracked file(s) were searched for {names}; the search reaches tracked files, as shown by the project manifest {manifest}",
            view.paths().len()
        ),
    )
    .location(Location::new(manifest, None))
    .subject(view.name())
}

fn not_applicable() -> Verdict {
    Verdict::NotApplicable(
        "the repository tracks no project manifest (Cargo.toml, package.json, pyproject.toml, go.mod ...), so there is no system to model"
            .into(),
    )
}

pub(super) fn judge_threat(view: &RepoView) -> Verdict {
    let Some(manifest) = a_manifest(view) else {
        return not_applicable();
    };
    if has_threat_model(view).is_some() {
        return Verdict::Findings(Vec::new());
    }
    Verdict::Findings(vec![missing(
        THREAT,
        view,
        &manifest,
        "threat model",
        "a file named THREAT-MODEL*, threat-model*, threat_model* or threatmodel*, and a file named threat* under docs/",
        "Write down what the system protects, from whom, and what is out of scope (a short STRIDE table is enough to start), and keep it in the repository next to the code it describes.",
    )])
}

pub(super) fn judge_failure(view: &RepoView) -> Verdict {
    let Some(manifest) = a_manifest(view) else {
        return not_applicable();
    };
    if has_failure_model(view).is_some() {
        return Verdict::Findings(Vec::new());
    }
    Verdict::Findings(vec![missing(
        FAILURE,
        view,
        &manifest,
        "failure model",
        "a file named FAILURE-MODEL*, failure-modes* (and the underscore forms), and a heading about failure modes or a failure model in ARCHITECTURE.md, docs/architecture*.md or docs/design*.md",
        "For each component, write down how it can fail, what the caller sees when it does, and what recovers it.",
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: (&str, &str) = ("Cargo.toml", "[package]\nname=\"p\"\n");

    fn threat(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge_threat(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            other => panic!(
                "unexpected: {:?}",
                matches!(other, Verdict::NotApplicable(_))
            ),
        }
    }

    fn failure(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge_failure(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            other => panic!(
                "unexpected: {:?}",
                matches!(other, Verdict::NotApplicable(_))
            ),
        }
    }

    #[test]
    fn a_repository_without_a_threat_model_is_told_and_one_with_it_is_not() {
        let f = threat(&[M, ("src/lib.rs", "")]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check_id, "SEC-007");
        assert!(f[0].clone().validate().is_ok());
        assert!(threat(&[M, ("THREAT-MODEL.md", "x")]).is_empty());
        assert!(threat(&[M, ("docs/threat-model.md", "x")]).is_empty());
        assert!(threat(&[M, ("docs/Threats.md", "x")]).is_empty());
        // a file merely named threat outside docs/ is not a threat model
        assert_eq!(threat(&[M, ("src/threat.rs", "")]).len(), 1);
    }

    #[test]
    fn a_failure_model_is_a_file_or_a_titled_section_of_the_architecture_document() {
        assert_eq!(failure(&[M]).len(), 1);
        assert!(failure(&[M, ("FAILURE-MODEL.md", "x")]).is_empty());
        assert!(failure(&[M, ("docs/failure_modes.md", "x")]).is_empty());
        assert!(failure(&[M, ("ARCHITECTURE.md", "# A\n\n## Failure modes\n\nx\n")]).is_empty());
        assert!(failure(&[
            M,
            ("docs/design.md", "# D\n## Failure model per component\n")
        ])
        .is_empty());
        // prose that mentions failure modes is not a section title, and another document is not the architecture document
        assert_eq!(
            failure(&[
                M,
                ("ARCHITECTURE.md", "# A\nWe discuss failure modes here.\n")
            ])
            .len(),
            1
        );
        assert_eq!(failure(&[M, ("README.md", "## Failure modes\n")]).len(), 1);
    }

    #[test]
    fn a_repository_with_no_manifest_does_not_apply() {
        assert!(matches!(
            judge_threat(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge_failure(&RepoView::from_files(&[("notes.txt", "x")])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn the_control_names_the_manifest_that_shows_the_search_reaches_files() {
        let f = threat(&[M]);
        assert!(f[0]
            .positive_control
            .as_deref()
            .unwrap()
            .contains("Cargo.toml"));
    }
}
