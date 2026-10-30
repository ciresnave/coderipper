# Which portfolio rules a CI job can check, and the one check set that does it

Written 2026-10-21 at the PM's task (CireSnave, 2026-10-21: *"We already had rules in place blocking relative path dependencies. If we don't already have CI checks blocking enforcing those rules, lets get them built and attached to all of our repositories. For that matter, any of our rules that can be checked in CI, should be."*). Read against `main@48a5b95` (0.8.6) of this repository, `C:\Projects\CLAUDE.md` and `C:\Projects\CIRESNAVE-EXPECTATIONS.md` as they stood on 2026-10-21.

**Method and its limits.** Every rule in the two files was classified from its text. Which CodeRipper rule covers it was read from `rules/*.toml` (`checked_today`) and from the check modules on `origin/main`. **Not done:** no rule was run against a repository to confirm what it finds today; the project-specific entries of `CIRESNAVE-EXPECTATIONS.md` section 5 (mlmf, synapse, OverMind, ThinkersJournal) were classified from their titles only; the KISS ledgers were not read, so "KISS ledger covers it" appears nowhere below. A catalog record with no `checked_today` line is a plan, not a check.

## 1. Classification

Classes: **CI** = a job can decide it from the repository (plus, where marked, the forge); **partly** = a job can check a stated slice; **no** = judgement, a process, or a person's act.

### Covered today by a CodeRipper rule or a pinned tool

| Rule (source) | Class | Covered by | Gap |
|---|---|---|---|
| One version per project (CLAUDE.md 9; EXPECTATIONS 2.2, 6.6a) | CI | `version-consistency`, with `[[tracks]]` for the one cross-project exception | Cargo only; an npm SDK paired with a crate (6.6a) is not read |
| CI and branch protection on every repo (6.5) | CI (network) | `ci-protection-presence` | Reads `required_status_checks`, and since 0.9.27 the settings audit (`enforce_admins` off, force-push and deletion blocked, squash-only, delete-branch-on-merge); forks are skipped. Private repos hide `protection` from a token without push access (an error, not clean) |
| `Cargo.lock` and `--locked` in CI | CI | SUP-001 | Cargo and npm only |
| CI actions pinned to SHAs, least-privilege permissions | CI | SUP-008 (zizmor) | none for the two named properties |
| CODEOWNERS | CI | WSP-001 | none |
| Secrets scan | CI | SEC-002 (gitleaks) | none |
| Dependency advisories (2.1's neighbour; 6.15 consumer side) | CI | SUP-002, SEC-006 (osv-scanner); `cargo deny check` (advisories, licences, git sources) in this repository only since 0.8.6 | `deny.toml` exists here; the 2026-10-21 PM list says 16 repos have none |
| Doc links resolve (6.1 "links onward") | CI | DOC-010 (lychee) | links only, not "covers all features" |
| `#[ignore]` / `allow(...)` carries a reason (5.1g) | partly | `lint-suppression` (extended profile) | reason, and since 0.8.17 a count ratchet against the merge-base (a branch adds none); no owner, no detector |
| Dead code, unused parameters and return values (5.1g "natural detector") | CI | `reachability`, `unused-parameters`, `unused-return-values` | an `allow(dead_code)` hides what they would report |
| No merge-conflict markers | CI | `conflict-marker` | none |
| Git dependencies denied (6.8, second half) | CI | `cargo deny` `[sources] unknown-git = "deny"`, where a repo has a `deny.toml` | only repos with a `deny.toml`; npm `git+` and `file:` dependencies not read |

### Not covered, and CI-checkable (the build list)

| Rule (source) | Class | What to check | Needs |
|---|---|---|---|
| **No `path =` dependency outside the repository** (6.8; CLAUDE.md 9 "Sources") | CI | Cargo: `cargo metadata` dependencies whose source is a path outside the workspace root; `[patch]`/`[replace]` and `.cargo/config.toml` paths the same way; in-workspace path deps must carry a `version`. npm: `file:` and `link:` dependencies leaving the package root. | new native rule (catalog home: SUP-004, "approved sources", planned). `cargo deny` does not see this: its `sources` table reads registries and git, and its `wildcards` ban reports a path dependency with no version, not one that leaves the repository |
| **No `include_*!("../...")`, no build script or test finding a sibling by a hardcoded relative path** (6.8, the baracuda `ozimmu-sys` case) | partly | `include_str!`/`include_bytes!`/`include!`/`#[path]` whose literal climbs out of the crate; `build.rs` and tests that join `".."` onto a manifest dir. A heuristic: a climb that stays inside the repository is fine | same rule as the line above, a second signal |
| No new `git =` dependency, with a named blocker for each existing one (6.8) | CI | the git dependencies found, each needing a waiver naming the blocker and owner (5.1f) | same rule; the deny.toml rollout covers the plain denial |
| **Licence file(s) present; SPDX identifier in each source file; manifest `license` agrees; holder is the organisation, not a person** (6.4) | CI | a file with no `SPDX-License-Identifier` in its first lines; a header whose identifier differs from the project's without an origin record (6.4a); `license` field vs the licence files; a personal name in a copyright line | new native rule or a delegated `reuse lint` through the delegation framework (REUSE checks files and headers, not the holder). Exceptions are data: KISS text is CC0, a third-party file keeps its origin's licence |
| **Version changed on every pushed change, with a CHANGELOG entry** (6.6, 6.6 extended 2026-09-25) | CI (PR) | the head's version differs from the base's, and `CHANGELOG.md` has a heading for the head's version; a breaking diff needs a second-number bump pre-1.0 (the second half only partly: a semver diff tool judges the API) | new rule (DOC-008 covers only the changelog half and is a plan). Must read a base ref, which no rule does today. **See decision 1 below** |
| **Newest dependency versions at all times** (CLAUDE.md 9; 2.1) | CI (scheduled) | the lockfile, not the manifest (2.1): `cargo update --dry-run` or the registry index lists every locked package whose newest version differs; a non-empty list is a finding unless a waiver carries the reason (2.1: a rule that cannot be met needs its reason). Done in 0.9.28 as the `dependency-freshness` check (cargo only; the scheduled workflow that runs it is not written yet) | new rule, run on a schedule, not per PR: a PR would turn red the day any dependency published |
| **Self-hosted job under `pull_request` carries the same-repo condition and the environment gate; private repos run on the self-hosted runner** (5.1h, "the lint becomes") | CI | workflow parse of `runs-on`, trigger, `if`, `environment`; repository visibility from the API | new rule or a zizmor audit (zizmor has a `self-hosted-runner` audit; check its persona and output before relying on it) Done in 0.8.18 as two `ci-security` findings (own-rule route, not zizmor); the environment half is waived on a private Free-plan repository. |
| `deny.toml` present and `cargo deny check` runs (CLAUDE.md 9 dependencies) | CI | file present; a workflow step that runs it | new rule; the list of repos without one exists (sent to the PM 2026-10-21) |
| Every branch gets checks on PRs (6.5 "all branches") | CI | a workflow triggered by `pull_request` with no `branches:` filter that leaves the default branch out | new workflow lint Done in 0.8.18 in `ci-security` (main and master assumed as the default). |
| Settings audit: `enforce_admins=false`, force-push and deletion blocked, squash-only, delete-branch-on-merge, forks excluded (6.5, CLAUDE.md 3 and 4) | CI (network, admin token) | extend `ci-protection-presence` to read those fields | extension. Needs a token that can read protection on private repos, so it cannot run with `GITHUB_TOKEN`. Done in 0.9.27: one Medium finding per differing setting (`enforce_admins` must be OFF, per 6.5); a fork is skipped; a branch under rulesets only gets the repository-level audit; reading needs admin, and a reply without the fields is an error |
| `#[ignore]` / `allow(dead_code)` counted, no new ones, owner and detector named (5.1g) | CI | per-repo count ratchet against the merge-base tree (GOV-001; the base is the committed number, so nothing to maintain); owner/detector per entry is not judged (an attribute has no field for them) | extension of `lint-suppression`, done in 0.8.17 |
| Waivers rare, each with reason and owner, counted in the report (5.1f) | CI | count `[[allow]]` entries per repo, reason and owner present, ratchet | extension; GOV-002 is a plan |
| README exists and has the five parts (6.1) | partly | the file exists; headings for description, features, examples, links; links resolve (DOC-010). "Comprehensive" and "covers all features" are judgement | new rule, existence and shape only |
| Library has CONTRIBUTING and doc comments on every function (6.3) | partly | `CONTRIBUTING.md` present for a library; `missing_docs` at deny level for a library crate (compile-time, 6.16, already free under `cargo clippy -D warnings`); DOC-001 is a plan | new rule: the lint level is set |
| Tests exist; coverage not falling (6.2) | partly | a test target per crate; coverage threshold (TST-001 is a plan; `coverage.yml` exists here). That every failure edge fails is judgement | not worth a rule beyond existence |
| Shared building blocks: no private persistence or auth stack (6.14, 6.17) | partly | `cargo deny` `[bans]` on direct backend crates outside PersistAnt (a per-project list, so a ban list the PersistAnt lane owns) | data, not code; the list is a decision for the PersistAnt and auth-framework lanes |
| Provenance: third-party files traced, licence and NOTICE kept (6.4a) | partly | a file whose header names an origin has that licence file and a `NOTICE`/`THIRD-PARTY` entry. That the copy is the origin's original is a person's check | folds into the licence rule |

### Not CI-checkable

Process and persons: Serena rules, `gh auth switch`, peer IDs and holds, message formats, restart and HANDOFF discipline (HANDOFF size is checkable in the PM's own repository only), Opus and model pinning (a launcher setting), cost routing, "don't wait for a TASK", merge authority and publish permission (6.10, 6.11), claiming crate names (6.12), backports (6.13), RustSec reporting (6.15), vault naming (6.12 second), compile-time-where-applicable (6.16: a design stance; one slice is free, `missing_docs`/`deny(warnings)`), FAM scope, the PersistAnt contract (6.17: a design review). Project-specific entries (mlmf `Option<T>`, synapse and OverMind Rust-before-complete, ThinkersJournal cache purges and `APP.live`) belong to those repositories' own tests.

## 2. One check set

**Shape: a `portfolio` profile in CodeRipper plus one reusable workflow in this repository.** The profile fixes the rule list, so a repository cannot drift by editing its own workflow; the workflow fixes how it is installed and run.

- **Profile `portfolio`** (a third value beside `classic` and `extended`): the per-PR rules above that need no network and no admin token: sources (new), licence (new), version-and-changelog (new), `version-consistency`, SUP-001, SUP-008, WSP-001, SEC-002, `lint-suppression` (with the count ratchet), `conflict-marker`, the self-hosted lint (new), workflow-triggers lint (new). Repository-specific facts live in the repository's `.coderipper.toml` (`[[tracks]]`, `[[allow]]` with reason and owner), which already exists and is already audited for staleness.
- **Reusable workflow** `ciresnave/coderipper/.github/workflows/portfolio.yml`, called as `uses: ciresnave/coderipper/.github/workflows/portfolio.yml@<40-hex SHA>`. It installs a pinned CodeRipper release from crates.io (`cargo install coderipper --version X --locked`, a version the caller cannot override), then runs `coderipper sweep --profile portfolio --deny medium --project .`. Inputs: `runs-on` (default `ubuntu-latest`; a private repository passes the self-hosted label). The caller's job holds `permissions: contents: read` only, and secrets are not passed.
- **Disk and failure reasons (0.14.1):** on a GitHub-hosted Linux runner the workflow first removes the toolchains the image ships and a build does not use, and fails by name (`disk: only N GB free ...`) if `MIN_FREE_GB` is not met, again before the sweep. A check that "failed to run" and a tool that could not be installed (gitleaks for SEC-002) are `::error::` annotations naming the cause, and the job is red even if the tool's own exit code was 0. Origin: synapse#290, run 41007729285.
- **Scheduled workflow** (same file, `schedule` plus `workflow_dispatch`): the checks that change without a push: dependency freshness, advisories (SUP-002, SEC-006), link rot (DOC-010).
- **Settings audit** is not a repository workflow: it needs a token that reads branch protection on every repository, so it runs from the PM's sweep (`ci-protection-presence` extended), not from `GITHUB_TOKEN`. The portfolio sweep already runs it.
- **Private repositories** call the same workflow with `runs-on` set to the self-hosted label, under the 5.1h private-repository conditions (push to main, schedule, dispatch, same-repo PRs). This repository is public (PM, 2026-10-21), so a private caller may use the workflow.
- **Rollout without turning every repository red on one day:** each rule starts report-only per repository (GOV-001 baseline, count ratchets down), the PM turns a check required once it is green on the default branch, which is how 6.5 already says to add checks.

## 3. Rulings (PM, 2026-10-21) and the blocking table

1. **Version and changelog (6.6).** A check on PRs into the default branch: the package manifest version (`Cargo.toml`, `package.json`; never a lock file) differs from the base's, and `CHANGELOG.md` has an entry for it. The `version-deferred` label is accepted only on a PR whose number the PM allocates at gate time; the PM's gate re-checks at merge.
2. **6.1-6.4 do not block a PR.** CireSnave, 2026-09-10, verbatim: *"My expectations are not \*right this second\* requirements. They are not gates that have to be verified before we proceed. However, they are things that need to remain on a to-do list until they are completed in every project. They are "before a project is truly complete, these will all be true"."* So the README, tests, library-docs and per-file SPDX classes run visibly but are **non-blocking**, and become requirements at the release line (a 1.0, a "production-ready" README, an announcement). **The unambiguous mechanical violations block**, because they break a rule he stated as a prohibition, not a to-do (6.8, 6.5, CLAUDE.md 9).
3. **Exceptions are data**, in the repository's `.coderipper.toml`, each with `reason` and `owner` (5.1f), counted in the report.
4. **Forks are excluded (6.5)**; the profile needs a per-repository "fork" switch fed from the PM's roll-out list.
5. coderipper is **public**, so private repositories can call its reusable workflow.

| Rule | Blocking on a PR | Non-blocking (visible, required at the release line) |
|---|---|---|
| Path dependency outside the repository; `git =` dependency without a waiver; `include_*!("../")` or `#[path]` leaving the crate | **blocking** | |
| `deny.toml` missing, or `cargo deny check` not run in CI | **blocking** | |
| Licence file(s) missing | **blocking** | |
| Actions not pinned to SHAs; broad token permissions (SUP-008) | **blocking** | |
| Lockfile / `--locked` (SUP-001); secrets (SEC-002); conflict markers | **blocking** | |
| Version changed vs base + CHANGELOG entry | **blocking** (`version-deferred` as above) | |
| One version per project (`version-consistency`) | **blocking** | |
| New `#[ignore]` / `allow(dead_code)` without reason (5.1g) | **blocking** (the reason check) | owner/detector, count ratchet |
| Self-hosted job on a PR trigger without the same-repo condition (5.1h) | **blocking** | |
| CODEOWNERS (WSP-001) | **blocking** (a file check) | |
| SPDX identifier in every source file; holder is the organisation; provenance notices | | non-blocking |
| README shape; CONTRIBUTING; `missing_docs`; tests exist; coverage | | non-blocking |
| Newest dependencies (scheduled); advisories; link rot | | non-blocking per PR; scheduled job opens the finding |
| Settings audit (admin bypass, force-push, squash-only) | run in the PM's sweep | |

Rows not in the PM's list (version-consistency, ignore reason, self-hosted, CODEOWNERS) are this lane's proposal; the PM edits.

## 4. Build order (each its own PR with its own version)

1. Sources rule: path outside the repository, `include_*!("../")` and hardcoded sibling paths, new `git =` (the rule CireSnave named first).
2. Licence rule: files, SPDX per source file, manifest agreement, holder.
3. Version-and-changelog rule (needs a base ref in the host).
4. `portfolio` profile and the reusable workflow, called from this repository first, pinned by SHA.
5. `lint-suppression` ratchet; self-hosted and trigger lints; settings-audit extension.
6. Dependency freshness (scheduled).
