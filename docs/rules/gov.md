# Keeping the checker honest: baselines, waivers and gates

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Human review alone is slow, uneven from reviewer to reviewer, and cannot keep pace with a growing codebase, so quality drifts. Standards that exist only in people's heads, or only in a wiki, decay. The remedy has two parts: move checks earlier, so that feedback arrives while the author is still editing, and write the standards down as versioned, reviewable policy that machines enforce, so the same rule applies to everyone every time.

The GOV rules differ from the other domains. They do not describe properties of the code under analysis; they describe how CodeRipper itself should behave: how it remembers old findings, handles exceptions, matures its rules, and presents results. They are all class T (features of the tool) and have no mechanical check of their own.

## How to apply it

### Living with existing problems

- **Baselines and ratchets (GOV-001).** When a tool arrives on an existing codebase, snapshot today's findings. From then on the gate rejects anything not in the snapshot, and the snapshot may only shrink. Keep one count per component. The hard part is identity: a finding needs a fingerprint that survives edits to the surrounding lines, or every refactor will make old findings look new. The approach suits complexity, forbidden dependencies, security issues, duplicated code, `unsafe` usage and gaps in testing.
- **Waivers (GOV-002).** A justified exception is a record: which rule, which target, why, who owns it, when it was made and when it expires. An expired waiver stops working and the finding returns. A waiver is a human decision, which is why this rule has a V5 component. Trend the waiver count and average age: a growing pile signals a rule that does not fit or a team under pressure.

### Managing the rules themselves

- **Policy as code (GOV-004).** Rules and configuration are versioned, reviewed and tested. Each rule should have fixtures that must trigger it and fixtures that must not; testing the rules, even by mutating them, catches rules that no longer detect anything. A policy change should prompt an analysis of how it affects existing baselines.
- **Rule lifecycle (GOV-003).** New rules start experimental, graduate to advisory and then to blocking only once measured precision justifies it, and are demoted if precision degrades. This applies to every tier and is essential for model-assisted checks.
- **Scopes (GOV-005).** Classify code as first-party, generated, vendored, third-party, test, benchmark, example, experimental or deprecated, and give each class its own rules. Generated code is not simply excluded: boundary-crossing checks and vulnerability checks keep running on it, while readability checks do not.

### Presenting results

- **Complete findings (GOV-006).** Each finding states severity (how bad if true), confidence (how likely true), evidence, and a remediation.
- **No composite score (GOV-007).** Report each dimension separately and gate by severity, so that a grave authorization flaw can never be cancelled out by pleasant formatting. Dimensions correspond to the domain codes; dashboards show a trend per dimension.
- **Engagement (GOV-009).** Every rule should explain itself, show examples, and point to fix guidance; developers should be able to suggest changes. Adoption is a human problem.

### Where checks run

- **Layered gates (GOV-008).** Editor and pre-commit: fast linters, formatters and exact checks. CI: the broad deterministic and test-based set, scoped to what changed. After that, at deployment and runtime: policy-as-code applied to manifests and infrastructure definitions, plus fitness functions tied to service-level objectives. Existing engines such as Open Policy Agent, Kyverno, Conftest and Checkov already cover the last layer, so CodeRipper should call them. Keep false positives low at every layer: users lose faith in a noisy tool faster than they do in one that occasionally misses something.

## Background

The architectural form of automated enforcement is the fitness function: an automated, objective check on some property of a design, run on a trigger or continuously, and either narrow in scope or system-wide. The GOV rules apply the same idea to the checker's own conduct.

## Measures

Track, per dimension rather than as one number: new findings versus baselined findings, the baseline count over time, the number of live waivers and their average age, how many rules sit at each maturity stage, and the measured precision of each rule that is advisory or blocking.

## Cautions

- A blended cleanliness number invites gaming and hides risk; the source is firm on avoiding it.
- Without a way to record exceptions, teams turn the tool off, so waivers are a feature, not a leak.
- Thresholds used in gates are defaults to be tuned per project (see the verification model page).

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| GOV-001 | Baseline findings and ratchet downward | T | no check yet: feature of CodeRipper, not a check (deferred). Fingerprinting across edits is the hard part |
| GOV-002 | Waivers with owner and expiry | T | no check yet: feature of CodeRipper, not a check (deferred). The waiver content is a human decision |
| GOV-003 | Rule maturity stages driven by precision | T | no check yet: feature of CodeRipper, not a check (deferred) |
| GOV-004 | Policy versioned, reviewed and tested | T | no check yet: feature of CodeRipper, not a check (deferred). Testing rules means running them on fixtures |
| GOV-005 | Classify code into analysis scopes | T | no check yet: feature of CodeRipper, not a check (deferred) |
| GOV-006 | Every finding states severity, confidence, proof and a fix | T | no check yet: feature of CodeRipper, not a check (deferred) |
| GOV-007 | Report per dimension, never one score | T | no check yet: feature of CodeRipper, not a check (deferred) |
| GOV-008 | Layered gates from editor to runtime | T | no check yet: feature of CodeRipper, not a check (deferred). Runtime objective gates need telemetry |
| GOV-009 | Rules ship rationale and remediation guidance | T | no check yet: feature of CodeRipper, not a check (deferred) |

## Further reading

- Building Evolutionary Architectures, Neal Ford, Rebecca Parsons and Patrick Kua (fitness functions).
- Documentation for Open Policy Agent (Rego), Kyverno, Conftest and Checkov.
