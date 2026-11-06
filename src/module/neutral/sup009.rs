// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-009: a release workflow generates a software bill of materials.
//!
//! **What this judges:** each tracked GitHub Actions workflow (`.github/workflows/*.yml` or `.yaml`) that **publishes or tags a
//! release** is read as text, comment lines excluded. A workflow is a release workflow when it names one of: `cargo publish`,
//! `cargo release`, `npm publish`, `twine upload`, `gh release`, `pypa/gh-action-pypi-publish`, `softprops/action-gh-release`,
//! `actions/create-release`, `goreleaser`, `docker push`. (A name this list does not hold is not guessed.) It generates an SBOM when the
//! same file names, in any case, one of `cyclonedx`, `cargo sbom`, `cargo-sbom`, `syft`, `spdx` or `sbom` (which covers
//! `anchore/sbom-action`). One **Info** finding per release workflow with none of them.
//!
//! **Applies to:** a repository with at least one release workflow; otherwise `not applicable`.
//!
//! **What it cannot see:** a release made outside GitHub Actions (another CI service, a script run by hand), an SBOM generated in a
//! reusable workflow or composite action the file calls, a workflow that publishes through a name not in the list, and whether the
//! SBOM is attached to the release or only produced. The finding is at the line of the first publishing step.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::SUP_009;

const PUBLISHERS: &[&str] = &[
    "cargo publish",
    "cargo release",
    "npm publish",
    "twine upload",
    "gh release",
    "pypa/gh-action-pypi-publish",
    "softprops/action-gh-release",
    "actions/create-release",
    "goreleaser",
    "docker push",
];

const SBOM_WORDS: &[&str] = &[
    "cyclonedx",
    "cargo sbom",
    "cargo-sbom",
    "syft",
    "spdx",
    "sbom",
];

fn is_workflow(path: &str) -> bool {
    path.starts_with(".github/workflows/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
}

/// The first line that names a publisher, and which one (comments excluded).
fn publishing_step(text: &str) -> Option<(u32, &'static str)> {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with('#') {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if let Some(p) = PUBLISHERS.iter().find(|p| lower.contains(*p)) {
            return Some((u32::try_from(i + 1).unwrap_or(u32::MAX), p));
        }
    }
    None
}

fn mentions_sbom(text: &str) -> bool {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .any(|l| {
            let lower = l.to_ascii_lowercase();
            SBOM_WORDS.iter().any(|w| lower.contains(w))
        })
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut release = 0usize;
    let mut findings = Vec::new();
    for path in view.paths().iter().filter(|p| is_workflow(p)) {
        let Some(text) = view.read(path) else {
            continue;
        };
        let Some((line, publisher)) = publishing_step(&text) else {
            continue;
        };
        release += 1;
        if mentions_sbom(&text) {
            continue;
        }
        findings.push(
            Finding::absence(
                ID,
                Severity::Info,
                Confidence::Medium,
                view.name(),
                format!("the release workflow {path} generates no software bill of materials"),
                "Consumers and auditors ask what a release contains. Add a step that produces a CycloneDX or SPDX document for the released artifact (for example `cargo cyclonedx`, `cargo sbom` or `anchore/sbom-action`) and attach it to the release. The check reads this one file as text; an SBOM made in a reusable workflow or composite action it calls is not seen.",
                format!(
                    "{path} was read in full and its publishing step `{publisher}` was found at line {line}; it was searched for the words {}",
                    SBOM_WORDS.join(", ")
                ),
            )
            .location(Location::new(path.as_str(), Some(line)))
            .subject(path.as_str()),
        );
    }
    if release == 0 {
        return Verdict::NotApplicable(
            "the repository tracks no GitHub Actions workflow that publishes or tags a release"
                .into(),
        );
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            _ => panic!("not applicable"),
        }
    }

    const RELEASE: &str = "name: release\non:\n  push:\n    tags: ['v*']\njobs:\n  publish:\n    steps:\n      - run: cargo publish\n";

    #[test]
    fn a_release_workflow_without_an_sbom_step_is_one_finding_at_the_publishing_step() {
        let f = found(&[(".github/workflows/release.yml", RELEASE)]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(8));
        assert!(f[0].clone().validate().is_ok());
    }

    #[test]
    fn an_sbom_step_in_the_same_file_satisfies_it_in_any_form() {
        for step in [
            "cargo cyclonedx",
            "uses: anchore/sbom-action@v0",
            "syft . -o spdx-json",
            "cargo sbom",
        ] {
            let text = format!("{RELEASE}      - run: {step}\n");
            assert!(
                found(&[(".github/workflows/release.yml", &text)]).is_empty(),
                "{step}"
            );
        }
        // a comment that names an SBOM tool is not a step
        let text = format!("{RELEASE}      # TODO cargo cyclonedx\n");
        assert_eq!(found(&[(".github/workflows/release.yml", &text)]).len(), 1);
    }

    #[test]
    fn a_workflow_that_does_not_publish_is_not_judged_and_none_at_all_does_not_apply() {
        let ci = "on: push\njobs:\n  t:\n    steps:\n      - run: cargo test\n";
        assert!(matches!(
            judge(&RepoView::from_files(&[(".github/workflows/ci.yml", ci)])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
        // positive control: beside a release workflow the ci workflow adds no finding
        let f = found(&[
            (".github/workflows/ci.yml", ci),
            (".github/workflows/release.yaml", RELEASE),
        ]);
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0].subject.as_deref(),
            Some(".github/workflows/release.yaml")
        );
    }
}
