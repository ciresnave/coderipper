// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-010: a release workflow signs its artifacts or attests their build provenance.
//!
//! **What this judges:** the same release workflows as SUP-009 (a tracked GitHub Actions workflow that names a publisher such as
//! `cargo publish`, `npm publish`, `gh release` or `goreleaser`; a `--dry-run` line is not a publisher), read as text with comment
//! lines excluded. It signs or attests when the same file names, in any case, one of `cosign`, `sigstore`, `slsa`, `provenance`
//! (which covers `npm publish --provenance` and `actions/attest-build-provenance`), `attest`, `gpg` or `minisign`. One **Medium**
//! finding, at Low confidence, per release workflow with none of them.
//!
//! **Applies to:** a repository with at least one release workflow; otherwise `not applicable`.
//!
//! **What it cannot see:** a release made outside GitHub Actions, signing done in a reusable workflow or composite action the file
//! calls, a tool this list does not hold, whether the signature or attestation covers the published artifact, whether it verifies, and
//! the assurance level (a human choice). It does not read the published release, so it is a local check of the pipeline, not a
//! verification of the artifact.

use super::sup009::{is_workflow, publishing_step};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::SUP_010;

const SIGNING_WORDS: &[&str] = &[
    "cosign",
    "sigstore",
    "slsa",
    "provenance",
    "attest",
    "gpg",
    "minisign",
];

fn mentions_signing(text: &str) -> bool {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .any(|l| {
            let lower = l.to_ascii_lowercase();
            SIGNING_WORDS.iter().any(|w| lower.contains(w))
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
        if mentions_signing(&text) {
            continue;
        }
        findings.push(
            Finding::absence(
                ID,
                Severity::Medium,
                Confidence::Low,
                view.name(),
                format!("the release workflow {path} signs nothing and attests no build provenance"),
                "Consumers can only trust that a release came from the declared source and build if that claim is verifiable. Add a step that signs the artifacts or attests their provenance (for example `actions/attest-build-provenance`, `cosign sign-blob`, `slsa-framework/slsa-github-generator`, or `npm publish --provenance`). The check reads this one file as text; signing done in a reusable workflow or composite action it calls is not seen.",
                format!(
                    "{path} was read in full and its publishing step `{publisher}` was found at line {line}; it was searched for the words {}",
                    SIGNING_WORDS.join(", ")
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
    fn an_unsigned_release_workflow_is_one_finding_at_the_publishing_step() {
        let f = found(&[(".github/workflows/release.yml", RELEASE)]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(8));
        assert!(f[0].clone().validate().is_ok());
    }

    #[test]
    fn a_signing_or_provenance_step_in_the_same_file_satisfies_it_in_any_form() {
        for step in [
            "cosign sign-blob target/release/app",
            "uses: actions/attest-build-provenance@v2",
            "uses: slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v2",
            "npm publish --provenance",
            "gpg --detach-sign app.tar.gz",
        ] {
            let text = format!("{RELEASE}      - run: {step}\n");
            assert!(
                found(&[(".github/workflows/release.yml", &text)]).is_empty(),
                "{step}"
            );
        }
        // a comment that names a signing tool is not a step
        let text = format!("{RELEASE}      # TODO cosign\n");
        assert_eq!(found(&[(".github/workflows/release.yml", &text)]).len(), 1);
    }

    #[test]
    fn a_dry_run_and_a_workflow_that_does_not_publish_are_not_judged() {
        let dry = "jobs:\n  t:\n    steps:\n      - run: cargo publish --dry-run\n";
        let ci = "on: push\njobs:\n  t:\n    steps:\n      - run: cargo test\n";
        for (path, text) in [
            (".github/workflows/ci.yml", dry),
            (".github/workflows/ci.yml", ci),
            ("README.md", "x"),
        ] {
            assert!(matches!(
                judge(&RepoView::from_files(&[(path, text)])),
                Verdict::NotApplicable(_)
            ));
        }
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
