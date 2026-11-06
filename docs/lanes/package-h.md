# Package H: repository-file checks (OPS-006, REL-005, SUP-005, SUP-009, SEC-007, REL-007, OPS-009)

Read [README](README.md) first, then the rows of these rules in [`docs/rules/REMAINING.md`](../rules/REMAINING.md). Language-neutral, source only, each a presence or shape check over tracked files (`git ls-files`,
never a disk walk). **Neutral-module checks** (register in `src/module/neutral/mod.rs`); copy `sup011.rs` (configuration files by name), `support_statement.rs` (a document and what it says), `ci_security.rs`
(workflow text) and, for SUP-005, the `Cargo.lock` read in `src/checks/dependency_freshness/mod.rs`. Read how an existing neutral check lays out `conformance/<RULE>/`. Each rule applies only where its kind of
file exists: a repository with no Kubernetes manifests is not told about probes.

**First, two mapping questions, answered by running the delegate, not by guessing** (the REMAINING.md rows say they were not run): **OPS-006** (secrets in configuration files) may already be what gitleaks reports
for SEC-002 (`delegate/gitleaks.rs`); **REL-005** (probes and resource limits in Kubernetes manifests) may be checkov checks under the SEC-009 delegate (`delegate/checkov.rs`). If a rule is covered, claim its id
from the delegate's `RULES` and add a conformance fixture; write no new detection. Run only tools this repository already pins (`coderipper tools`), on a fixture you wrote.

**SUP-005**: more than N distinct versions of one package in `Cargo.lock` (default N from the corpus, setting `[sup005] max_versions`; report the packages and their versions, not just a count). Info.
**SUP-009**: the release workflow (one that publishes, tags or uploads release assets) generates an SBOM (`cargo cyclonedx`, `cargo sbom`, `syft`, `cyclonedx`, `spdx` in a step). Info.
**SEC-007**: a threat model exists (a tracked file named `THREAT-MODEL*`, `threat-model*`, or `docs/**/threat*`). Presence only; the text is not judged and the finding says so. Info.
**REL-007**: a failure model per declared component exists (a document named `FAILURE-MODEL*` or `failure-modes*`, or a section so titled in an architecture document). Presence only. Info.
**OPS-009**: Prometheus alert rule files (`*.rules.yml`, `alerts*.yml` with `groups:` and `alert:`) in which an alert has no `runbook_url` annotation. Applies only where such files exist.

Most are absence findings: obey `Finding::validate` (an absence needs a positive control: name the file kinds searched and one present file as the control; see `support_statement.rs`). Default Info; none in
`classic`. Measure on the corpus; these should be quiet or not applicable on most of it. Patent read first: presence checks for documents and SBOM steps; expected hits are scanners and policy tools (OpenSSF
Scorecard checks several of these).

Done when: the checks (or delegate claims) are registered, fixtures, corpus numbers, patent rows, fragments, READY. Order: the two mapping questions, then SUP-005, SUP-009, SEC-007 and REL-007, then OPS-009.
