# CodeRipper roadmap

Where CodeRipper should go, in order, and who uses it. **Written 2026-10-14 against `main@1aebeb2` (0.6.14), at the PM's
request; the PM owns priorities, so this is the lane's proposal and the PM edits it.** Detail lives in the plans under
`docs/superpowers/plans/`; this file orders them and says why. It records no promise of a date.

## Where it stands (read from `main@1aebeb2`)

- **One engine, three faces.** The library, `coderipper` and `cargo coderipper` are built and published
  (crates.io lists 0.6.3, 0.6.9, 0.6.11 and 0.6.14 as not yanked, read 2026-10-14). The hosted service is **design only: nothing is
  built, and nothing is built until CireSnave gives a separate go** (`docs/superpowers/plans/2026-10-12-hosted-service.md`).
- **Rules that run.** Five native Rust checks (`reachability`, `unused-return-values`, `unused-parameters`, `version-consistency`,
  `ci-protection-presence`); five language-neutral native rules (SUP-001, SUP-011, DOC-005, DOC-009, WSP-001); six rules run by
  a pinned third-party tool through the delegation framework (SEC-002 gitleaks, SUP-002 and SEC-006 osv-scanner, DOC-010 lychee,
  SUP-008 zizmor, API-006 buf). Every rule except `ci-protection-presence` has a seeded defect and a clean twin under `conformance/` (it needs a forge: its pair is `tests/fixtures/forge/` run by `tests/ci_protection_fixture.rs` over a stub forge, so the built-in module does not claim it as `proof`).
- **Not built.** The TypeScript and Python modules (multi-language P4, P5), the remaining 96 unscheduled native rules, the
  and every hosted-service component (the configuration layers, designed in `2026-10-13-configuration-layers.md`, were built in 0.6.21).
- **The default ruleset is still the five classic checks.** CireSnave's ruling (board 144) is that the default becomes all rules,
  which is a breaking release; it is **not ready**: the 26-repository sweep of 2026-10-13 found WSP-001 on 23 repositories,
  DOC-009 on 21 and SUP-008 on 21, so those three mechanical rules must be triaged first.
- **Measured on the real fuel workspace (Task 7, 2026-10-15, `fuel@8cdb33dd` 0.19.5, CodeRipper 0.6.32, `--workspace`).** **45 members analysed = the plan's 43 publishable members + 2 non-publishable fixtures** (`fuel-model-fixture`, `fuel-model-umbrella-fixture`; the plan was written against `fuel@d1bfe127`, which listed 43). **Findings:** reachability 469 (all medium; 25 members; the cold and warm runs are byte-identical), unused-return-values 36 (28 low + 8 medium; 6 members), unused-parameters 0; every run exits 3. **Wall time per whole-workspace run:** reachability 1742 s cold and 915 s warm, unused-return-values 1992 s and unused-parameters 1334 s (both run after reachability, so against a warm cache); the cache is 73.5 GB. **The lever is an incremental cache, not parallelism** (PM's ruling). **Not collected:** per-member times (the runs were whole-workspace; the plan asked for them), per-member parity against `--workspace` (the PM ruled it unnecessary), and a run beside another lane's build. On Windows, `fuel-cuda-backend` and `fuel-metal-kernels` cannot build, so every run exits 3 for that environmental reason. **unused-parameters reported 0 findings and no positive control: the control's target was not built** (`fuel-capture-fixtures` has `required-features = ["capture"]` and is never compiled in a default build; no crate-wide `allow(unused)` hides it). **Defects found by this run, all fixed (0.6.33-0.6.35, proposed):** (1) when a control is absent because its target was not built, the output now says so ("target not built: required-features=[capture]"), fixed for unused-parameters in PR #83; (2) `--features`, `--all-features` and `--no-default-features` now reach the build step (PR #84); (3) CodeRipper reads the committed tree (a throwaway worktree), so uncommitted fixes show no change; `--help` and the run now say so (PR #85; 0.6.36 corrected it: the throwaway worktree is used by reachability, unused-return-values and unused-parameters only, `version-consistency` and API-006 read the working tree, and the note now counts untracked files too).

## Order of work

The order is: finish what users hit today, then what blocks the default, then reach, then the hosted service. Each item names what
would make it stop.

| # | Item | Why now | Size | Blocked by |
|---|---|---|---|---|
| 1 | **DONE in 0.6.16 (proposed): proc-macro crates under `cfg_attr`** (the remainder of issue 54). 0.6.7 kept `#[proc_macro*]` entry points `pub`, but an attribute written inside `cfg_attr(..., proc_macro_derive(..))` was not recognised and that crate still exited 3. | A crate that exits 3 cannot go clean under "all rules"; issue 54 found it in 1 of the 23 repositories measured on 2026-10-12. | small | nothing |
| 2 | **Backport of item 1 and of the 0.6.7 fix.** Published 0.6.9, 0.6.11 and 0.6.14 already carry 0.6.7; only 0.6.3 lacks it. There are no release branches and the line is pre-1.0 (users take the newest 0.6.x), so no backport branch is cut: the answer is "publish the item 1 fix in the next 0.6.x". | The portfolio rule is to backport when it is not a lot of work. | none | the PM's `cargo publish` |
| 3 | **Triage WSP-001, DOC-009 and SUP-008 across the portfolio** (report-only for this lane: the per-repository fixes belong to the lanes that own those repositories). Produce one list per rule: true defect, rule too strict, waiver with a reason. **Lists produced 2026-10-14 (27 repositories at their default branch): `docs/portfolio-triage/2026-10-14-wsp001-doc009-sup008.md`; the per-repository fixes are the other lanes' to make.** | It gates the breaking release (item 4). | medium, mostly other lanes | the other lanes |
| 4 | **All rules by default (breaking, 0.7.0).** | CireSnave's ruling; the 2026-10-13 sweep says not ready. | small once 3 is done | item 3 |
| 5 | **DONE in 0.6.21 (published): the configuration layers** (system file, user file, restricted repository file, environment, flags; `--config`, `--no-config`; #69). | The hosted service and a site-wide policy both need a stronger-than-repository layer, and today a repository can only waive what it chooses. | medium | nothing |
| 6 | **Known reachability limits.** (a) an intended public API of a library with no in-repo consumer is still reported (**DONE in 0.6.23, proposed: `[reachability] public_api` declares the surface with a reason per entry, printed as declared public; no source signal is guessed**); (b) `[patch]` and `[replace]` dependencies are not seen by the dependents rescue (**DONE in 0.6.22, proposed: a patch whose `path` is a member's directory counts as a path dependency; git, out-of-workspace and `.cargo/config.toml` patches are still not seen**). | These are the two shapes the sweep still reads as false positives; each waived finding costs a reviewer. | medium | nothing |
| 7 | **Small defects found and not filed:** the gitleaks wrapper "did not read the history" on a Windows path longer than 260 characters (fixed in 0.6.17, proposed: `core.longpaths=true` through git's environment configuration); `--redact` on `conformance` (fixed in 0.6.18, proposed); and run `cargo coderipper --deny medium` in this repository's own CI (done in 0.6.19, proposed). | Cheap, and each is a wrong or missing message. | small | nothing |
| 8 | **TypeScript module (multi-language P4), then Python (P5), then the install policy (P6).** The estimates are in the programme plan: 9-22 and 8-21 lane-days, both unmeasured. | The portfolio's websites and tools are TypeScript; Rust-only coverage leaves them as gaps. | large | items 3-5 (so a module is not built on a default that is about to change) |
| 9 | **Hosted service.** Result document and intake interface first (design), then the build. | CireSnave's stated plan for ThinkersJournal.com; the design is complete enough to price. | large | **CireSnave's separate go** |

Item 1 is done; items 2 and 7 are this lane's to start now. Item 3 needs the PM to route the other lanes. Item 9 waits and is not worked.

## Who consumes it, and who should

**Read 2026-10-14 from the git index of every repository under `C:\Projects` that has a `.git` entry, searching tracked
`.github/` files, `Cargo.toml` files and shell scripts for the word "coderipper"** (positive control: the same query finds the two
repositories named below). The search names a word, so it can miss a consumer that calls the binary through a path or a
different name.

- **Today, nothing in the portfolio runs CodeRipper in its own CI.** Two repositories mention the word: `synapse` (a comment in
  `ci_cd.yml` about a coverage pilot agreed on 2026-10-03, not a use) and `ThinkersFoundation` (a CODEOWNERS comment that records
  a WSP-001 finding CodeRipper made). **This repository's own three workflows (`ci.yml`, `coverage.yml`, `live-tools.yml`) do not run the
  binary on the repository either**, so CodeRipper is not its own first consumer; item 7 fixes that.
- **Who should, in order:**
  1. **Every Rust repository in the portfolio**, as a CI step: `cargo coderipper --deny medium`, with each repository's
     `.coderipper.toml` holding reasoned waivers. This is the face CireSnave asked for first ("runnable alongside cargo clippy").
     It needs item 4 first for the full ruleset, but `fast` is usable now.
  2. **The PM's portfolio sweep** (`coderipper sweep --workspace`), which is how the 26-repository numbers above were produced;
     it should become a scheduled job whose output is the board's evidence, not a manual run.
  3. **Synapse**, as a consumer of the schema and workflow rules (API-006, SUP-008), so it does not grow its own copy. Whether it
     wants that is a question for the Synapse lane, not a decision made here.
  4. **The hosted service on ThinkersJournal.com**, for open-source projects, once CireSnave gives the go.
- **What a consumer needs from us:** a stable `Finding` JSON shape and exit codes (promised since 0.3.0), a crates.io version to
  depend on (never a path or a git dependency), and, for the multi-repository case, item 5.

## What would change this order

- A published release is found to report a live item as dead: that outranks everything, because it makes consumers distrust the rest.
- CireSnave's go on the hosted service: item 9 moves up and items 5 and 6 are prerequisites of it.
- The sweep shows a mechanical rule is mostly noise: it is demoted out of the default, not waived repository by repository.
