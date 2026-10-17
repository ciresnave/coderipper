# Q14 re-measurement, 2026-10-16 (the bar for the all-rules default)

**Report only for the portfolio's repositories; the one code change is the reachability text (0.6.42).** Produced by the CodeRipper lane at the PM's task (CireSnave 2026-10-16 on board 180 Q14: "Go with your recommendation" = no breaking release yet).

## What was measured

- **Tool:** `coderipper` 0.6.41 built from `main@f5defb8`; `coderipper --no-config check <RULE> --profile extended --deny none --message-format json --project <clone>`, one run per rule per repository.
- **Subject:** depth-1 clones of each repository's default branch from its remote on 2026-10-16: the same 27 repositories as the 2026-10-14 triage (every directory under `C:\Projects` with a `.git` directory and an `origin`, minus `cutlass` and the two `auth-framework-ghsa-*` forks). `waived` is 0 in every run.
- **Positive control:** the same queries find 176 SUP-008 findings in 10 repositories, 11 WSP-001, 3 DOC-009, 12 SUP-001 and 3119 lint-suppression, so none is an always-clean query; the other repositories exit 0 with 0 findings.

## The three rules, 2026-10-14 against 2026-10-16

| Rule | 2026-10-14 | 2026-10-16 | Left |
|---|---|---|---|
| WSP-001 | 24 repos | **11 repos** | `ThinkersJournal.com`, `amt`, `aocl`, `approvals-ciresnave`, `auto-discovery`, `candlelight`, `consent`, `fam`, `kiss-ref`, `mlmf`, `vulkane` |
| DOC-009 | 21 repos | **3 repos** | `HumboldtUnifiedKidTracker` (33 pages), `amt` (11), `mlmf` (17) |
| SUP-008 | 326 findings, 18 repos | **176 findings, 10 repos** | `HumboldtUnifiedKidTracker` 1, `ThinkersJournal.com` 10, `amt` 40, `aocl` 14, `auth-framework` 8 (service-container images), `auto-discovery` 56, `candlelight` 7, `fam` 4, `mlmf` 7, `vulkane` 29 |

**Every remaining finding is a true defect in the repository's own files; none is the rule being wrong.** The three rules are mechanical (one file, one block, one hash each) and low-noise, so this lane changed none of them. WSP-001 is the missing `CODEOWNERS` (the owner handle for a `ThinkersJournal/` repository is the PM's to name; `approvals-ciresnave` is the profile repository and takes the one-line file or the waiver "profile repository; no review flow to route"). DOC-009 is a `GLOSSARY.md` the owner has to write. SUP-008 is unpinned actions (148), jobs without `permissions:` (20) and 8 `postgres`/`redis` images; `synapse`, which had 3 such images, is now clean, so images can be pinned and the waiver the 2026-10-14 triage suggested is not needed.

## The other repository-level rules

| Rule | Findings | Where |
|---|---|---|
| SUP-011, DOC-005 | 0 | all 27 |
| SUP-001 | 12 | a CI `cargo` command without `--locked`: `agentlife`, `amt` 2, `auth-framework` 2, `baracuda`, `consent`, `fuel`, `lightbulb`, `synapse`, `unisql`; and `OverMind` (`probe/package.json` has no lockfile) |
| lint-suppression | 3119, 18 repos | `baracuda` 1632, `fuel` 860, `aocl` 291, `lightbulb` 77, `auth-framework` 74, `Unpopped` 35, `vulkane` 33, `amt` 21, `agentlife` 19, `HumboldtUnifiedKidTracker` 15, `KISS` 14, `mlmf` 14, `kiss-ref` 12, `auto-discovery` 9, `consent` 6, `synapse` 3, `SafeImage` 2, `unisql` 2 |

`baracuda`, `synapse` and `fuel` (unmeasured on 2026-10-13 because a per-member Rust run takes hours) are now measured for every repository-level rule: baracuda 1 SUP-001 + 1632 lint-suppression, synapse 1 + 3, fuel 1 + 860; the rest are 0. Of the 3119 lint-suppression findings, 1428 are `#[ignore]` without a reason (CireSnave's rule: a self-hosted runner, never `#[ignore]`) and 190 are `dead_code`; the rest are other lints. They are true by his rule and mostly mechanical, but 3119 is the largest part of the bar.

## Reachability, read for truth

Run on `agentlife` (12 findings), `auto-discovery` (29), `lightbulb` (137) and `mlmf` (74); `amt`, `consent`, `vulkane` and `candlelight` are workspace roots (exit 3: "pass `--project <member directory>`") and were not read. Sample: 30 findings of `auto-discovery` and `mlmf`, each read against its source and its `pub mod` chain from `src/lib.rs`.

- **28 of 30 (93%) are rooted in a `pub` item of a `pub` module of a published library**, with no consumer inside the repository: true as stated ("zero callers found within this crate"), and not dead code. They need the library's `[reachability] public_api` declaration (or removal if the API is unwanted). 26 are themselves `pub`; 2 are private helpers reached only from a `pub` function with no caller (`write_safetensors`, `extract_attention_weights`).
- **2 of 30 (7%) are real dead private code** (`copy_with_metadata`, and `load_model_by_format`, reached only from the private uncalled `convert_between_formats`).
- **0 of 30 wrongly reported.** (The 2026-10-13 sample was 28% wrong, all from sibling workspace members; 0.6.9 fixed that, and these two repositories are single packages, so this sample does not test it.)

So the reachability volume is mostly an undeclared-API count. The finding text did not say so; 0.6.42 does ("It is written `pub`: if it is this library's API, declare it under `[reachability] public_api` with a reason; if not, remove it or narrow it to `pub(crate)`"), planted both ways in `tests/reachability.rs`.

## The measured bar

All-rules by default with `--deny info` turns red every repository with any of: a missing `CODEOWNERS` (11 left), a missing glossary (3), an unpinned workflow (10), a CI cargo command without `--locked` (10), lint suppressions without a reason (3119 in 18), and reachability on libraries that have not declared a public API. **The bar is not met.** What would meet it, smallest first: WSP-001 (11 one-line files), SUP-001 (12 edits), DOC-009 (3 glossaries), SUP-008 (176 findings in 10 repositories), `public_api` declarations for published libraries, then the lint-suppression reasons (3119). The release decision returns to the board when these hold; this lane does not decide it.

Not done, and why: per-member reachability on `amt`, `consent`, `vulkane`, `candlelight`, `baracuda`, `synapse` and `fuel` (hours each), and the other Rust rules (`unused-parameters`, `unused-return-values`, `version-consistency`) over the whole portfolio. Their counts are not in the bar above.
