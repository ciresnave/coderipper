# Triage of Pip's rules review (2026-11-07)

Pip's review (relayed by the PM, `C:\Projects\.lane-state\pip-rules-review-2026-11-07.md`) is treated as claims to verify. Every claim below was read at
`origin/main` c715b60 (rules/*.toml, the check's source, `CHANGELOG.md`), and carries a verdict: **AGREE**, **DISAGREE** or **PARTLY**, with the file and line.
`SCHEDULED` means a row in [`docs/rules/REMAINING.md`](rules/REMAINING.md) (section E, below the existing table), default on and low value meaning scheduled,
never declined (CireSnave, 2026-10-25). **RULING** marks what only CireSnave can settle. Nothing here was run against third-party code: the counts come from a
throwaway program (not committed) that prints the catalog joined to `BuiltinCheck::ALL`, `NeutralModule::describe()` and `delegated_rule_ids()`.

## 0. The count Pip got wrong, and the real one

| Quantity (c715b60) | Pip | Measured |
| :--- | ---: | ---: |
| Catalog records | 231 | **231** (217 numbered, 14 slug-named): agree |
| Records with `checked_today` | 36 | 37 before this PR; **66 after** |
| Rule ids with an implemented check | "36 checks" | **66** (52 numbered, 14 slug-named): 26 built-in, 29 neutral-module (8 opt-in), 11 delegated; no id is on two routes |
| Distinct detectors | - | 63 (REL-003 is CON-004's reading, SEC-006 is SUP-002's OSV groups narrowed, OPS-006 is SEC-002's gitleaks scan narrowed) |
| Fixture-proven (built-in + neutral) | - | 53 of 66; the 11 delegated are proven only while their tool is installed |
| ARC implemented | 0 of 12 | **10 of 12** (ARC-001 to 009 and 011; missing ARC-010 model-assisted, ARC-012) |
| MOD implemented | 0 of 11 | **10 of 11** (MOD-001 to 006, 008, 009, 010 and the slug `reachability`; missing MOD-007) |
| Domains with 0 implemented | 8 (79 rules) | **6** (AIH 8, COR 10, EVO 9, PRF 8, TST 13, TYP 8 = 56 rules) |

**Verdict: PARTLY.** The 231, the lifecycle split (226 experimental, 5 advisory, 0 blocking), the severity split (low 97, medium 84, info 25, high 24, critical 1),
`evidence_strength` C 196, `kb_refs` empty 225, 15 contested, 217 + 14 are all correct. "36 checks wearing a 231-rule coat" is stale: the records for 29 implemented
rules (the whole ARC and MOD families, `reachability`, `unused-*`, `version-consistency`, `ci-protection-presence`, OPS-011, API-001, API-007, SEC-002, SEC-009) simply lacked
`checked_today`, which this PR fills (`rules/*.toml`). The real figure is 66 of 231 (29%): 52 of 217 numbered rules and all 14 slug-named ones.
`docs/rules/REMAINING.md` was stale the same way (48 built, 183 rows); it is brought to 66 built and 165 rows in this PR.

## A. Missing rules, ranked (Pip)

All ten are **SCHEDULED** as new rows (REMAINING.md section E) with a proposed id; none exists in the catalog, so none is "declined" or "wrong". Ids are proposals
and become records when a rule is built.

1. **Tests weakened to make a change pass: AGREE, SCHEDULED (AIH-009).** `AIH-005` (rules/aih.toml:100) and `TST-013` (rules/tst.toml:292) are about deriving expectations from code, not weakening. Checkable from the diff against the merge-base: a removed `#[test]`, removed or loosened assertion, a newly added `#[ignore]` (the last is half-covered today by `lint-suppression`, rules/gov.toml:220).
2. **Run the tests before claiming done: AGREE, SCHEDULED (AIH-010).** No record says it (grep of titles and statements: only TST-011 mentions running). Not decidable from a repository; it is a hook or CI-status rule: "the commit's checks ran green". RULING: is a harness rule (an agent hook) in scope for CodeRipper at all, or only the CI-status reading?
3. **SEC-003 excludes Rust; no authentication rule: AGREE.** `languages = ["typescript","python"]` (rules/sec.toml:52); `SEC-010` (model-assisted review) is the only mention of authentication. SCHEDULED: Rust applicability for SEC-003 and SEC-008 (a gap is honest, flipping the field claims no check) plus SEC-011 (authentication on entry points), Rust route inventory for axum, actix-web, rocket, warp as the first reading.
4. **`!Send` across await: AGREE, SCHEDULED (CON-012).** `CON-002` is guards across await (rules/concurrency.toml:28). A public `async fn` whose future is not `Send` can be read from the signature and the types it holds across awaits; clippy's `future_not_send` is nursery, so a native reading is justified.
5. **Cargo feature additivity: AGREE, SCHEDULED (SUP-013).** Readable from `Cargo.toml` alone: features that remove (`no-*`, `disable-*`), mutually exclusive pairs (`compile_error!` under two features), default features that cannot be turned off.
6. **`Debug` on secret-holding types: AGREE, SCHEDULED (SEC-011a/ERR-009 Rust half).** `derive(Debug)` or `impl Debug` on a type with a field named like a secret (password, token, secret, api_key, private_key) and no `secrecy`-style wrapper. SEC-004 is hard-coded keys, not this.
7. **`build.rs` / proc-macro hygiene: AGREE, SCHEDULED (SUP-014).** `SUP-007` mentions build scripts only inside a dependency's justification (rules/sup.toml:151). Readable: network calls, writes outside `OUT_DIR`, no `rerun-if-changed`, `unsafe`.
8. **Arithmetic safety: AGREE, SCHEDULED (COR-011).** `COR-007` is float equality (rules/cor.toml:148); `TYP-005` is money in floats (rules/typ.toml:76). Cheapest first step: whether `[lints]` enables clippy's `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss` in numeric crates, then native reading of `as` narrowing casts.
9. **Send/Sync soundness: PARTLY, SCHEDULED (COR-012).** `SEC-005` already reports an `unsafe impl` (so `unsafe impl Send/Sync`) with no `SAFETY` comment (`src/module/neutral/sec005.rs`), so the inventory half exists; the soundness half (a field that is a raw pointer, `Cell`, `Rc`) does not.
10. **API-003 and 0.x: AGREE, fixed in this PR.** The statement read "major for breaking, minor for additions" with no 0.x clause (rules/api.toml:52). It now says that before 1.0 the second number is the major (CireSnave, 2026-09-17, `C:\Projects\CLAUDE.md` section 9). API-003 has no check yet.

Not on the list but in Pip's "first" list: **agent changes need human review: AGREE, SCHEDULED (AIH-011).** `SEC-010` covers a model reviewer with a human confirming findings; nothing says an agent-authored change needs a human. Not checkable from a tree; checkable as branch protection (required reviews), which `ci-protection-presence` reads (rules/wsp.toml:29).

## B. Rules wrong in an important way (Pip)

1. **SEC-003 and SEC-008 exclude Rust: AGREE.** rules/sec.toml:52 and :176 list typescript and python only; SEC-008 has the partial Rust sibling ERR-009 (rules/err.toml:226, all three languages, no check). SCHEDULED as A3. RULING: none needed (a Rust gap is the honest state).
2. **API-009 flags every public type: PARTLY.** True of the reading (rules/api.toml:199: every `pub` enum with variants and every `pub` struct whose named fields are all `pub`, no `#[non_exhaustive]`, no doc word closed/sealed/fixed/exhaustive), and `non_exhaustive` on an all-pub struct does break downstream construction. But it is Low, Medium confidence and opt-in (`opt_in_rule_ids()`, `src/module/neutral/mod.rs`), so it runs only when named. SCHEDULED: drop all-pub-field structs from the default reading (a struct built by literal is closed by design); keep enums. The EXT-001 tension is real and unresolved. RULING: is "enum-only, opt-in" the intended end state?
3. **ERR-004 alias-blind, anyhow a defect: PARTLY.** Alias-blind: AGREE, the record says so (rules/err.toml:104 "Not judged: a type alias that hides the error"); SCHEDULED: resolve a `type Error = ...` in the same file. "anyhow as a blanket ban": DISAGREE: it applies to the `pub fn`s of a library (not binaries, not private functions), the statement is "specific error types a caller can branch on", the rule is contested and opt-in.
4. **lint-suppression ratchet counts reasoned allows: AGREE.** `src/module/neutral/lint_suppression.rs` lines 9-15: the `allow` and `ignore` counts are "reasons or not", compared with the merge-base; a rise is Medium at the first file whose count rose. One new reasoned `#[allow(.., reason = "..")]` is flagged like an unreasoned one. This is GOV-001's reading (rules/gov.toml:4 "new code may add none") and is deliberate, but it contradicts the rule's own title ("says why"). RULING: ratchet total suppressions (GOV-001 as written) or only unreasoned ones (Pip, the title)? Until ruled, no change.
5. **API-006 and SUP-008 ignore the project's own waivers: PARTLY.** True: rules/api.toml:126 "The repository's own buf.yaml (its breaking ignore lists ...) is not honoured"; rules/sup.toml:175 "zizmor.yml and its zizmor: ignore comments are not honoured". "Contradicting GOV-002" is wrong: GOV-002 (rules/gov.toml:28) defines a waiver as a record with owner, date and expiry, which a tool-native ignore does not carry; the allowlist is the waiver route. Documented choice, not a bug. RULING (optional): honour tool-native ignores as a waiver source with a `needs-reason` note?
6. **ARC-005 flags every leaf, no threshold: DISAGREE.** `src/checks/architecture/stability.rs:9-11`: an edge A -> B is reported only when I(B) - I(A) reaches a margin (default 0.3, `[arc005] min_margin`); a leaf is the unstable end and has no edge to judge; "a component with no edges has no instability". Severity is Info, confidence Low, "the crate-level reading almost never fires" (stability.rs:25-27). It is not a duplicate of ARC-003 (volatile names in policy components) or ARC-004 (infrastructure types in signatures).
7. **CON-001 vacuous for safe Rust: AGREE.** rules/concurrency.toml:4: "ruled out ... because the language guarantees it for safe code or because a race detector has exercised the concurrent paths"; no check exists and none is possible for safe Rust. SCHEDULED: reword to the cases that exist in Rust (unsafe `Send/Sync` impls, `static mut`, FFI): fold into COR-006 and the Send/Sync row (A9), keep the record for C/C++-style languages. RULING: none.
8. **version-consistency wrong default, contradictory rating: DISAGREE.** The default encodes CireSnave's rule "I always want all crates within a project to use the same version number ..." (`C:\Projects\CLAUDE.md` section 9, 2026-09-17; the exception for a version tracking another project is in the same quote and in `docs/rules/wsp.md`). High severity with advisory lifecycle is two axes: severity is how bad if true (GOV-006, rules/gov.toml), lifecycle is gating maturity (GOV-003); rules/wsp.toml:4 and :29 both say high + advisory on purpose.
9. **API-012 recommends varying incidental behaviour: AGREE.** rules/api.toml:273: Low severity, Low confidence, a design essay; no check is feasible. Not removed (a catalog record is the owner's knowledge base). RULING: keep as Info guidance with `lifecycle` unchanged and mark "not checkable" in REMAINING.md, or retire the record?
10. **EFX-003's three files is chosen, not measured: AGREE.** `checked_today` says "(the three is chosen, not measured)" (rules/efx.toml:52). SCHEDULED: measure 2, 3, 5 on the nine corpus repos and pick by the false-positive reading (the rule is opt-in because the portfolio run found too many). Pip's EFX-003/OPS-001 duplicate claim: see C.

## C. Coherence (Pip)

**Duplicates, claim by claim** (a merge would be an alias, as TYP-003 -> MOD-010 in 0.41.1; the catalog keeps one record that answers to both ids):

| Pair | Verdict | Two lines |
| :--- | :--- | :--- |
| EFX-003 / OPS-001 | DISAGREE | EFX-003 is "environment reads are centralized" (checkable now); OPS-001 (rules/ops.toml) is "one typed, validated configuration entry point", wider (files, typed parse). Related, not the same. |
| DOC-001 / MOD-001 / COR-001 | DISAGREE | DOC-001 documents public items; MOD-001 is a small, documented public surface; COR-001 is what the docs must state (errors, panics, safety). Three different checks; MOD-001 reuses DOC-001's reading. |
| COR-002 / TYP-008 / MOD-010 | PARTLY | MOD-010 (private fields + validating constructor) is the checkable half of COR-002 (enforced in code); TYP-008 adds "written down". Candidate: COR-002 alias of MOD-010 (RULING, as for TYP-003). |
| DOC-006 / RDB-010 | AGREE | Same statement in two words (docs and comments match current behaviour). SCHEDULED alias RDB-010 -> DOC-006. |
| DOC-002 / API-010 | AGREE | Same statement (doc samples compile and run); API-010 adds "where practical". SCHEDULED alias API-010 -> DOC-002. |
| EVO-005 / ARC-009 | PARTLY | ARC-009 (built, compares cycles and surface against one base) is a fraction of EVO-005's trend per component; keep EVO-005 for the trend, point it at ARC-009 in `related`. |
| EVO-003 / MOD-006 | AGREE | Same reading (divergent change); MOD-006 is built. SCHEDULED alias EVO-003 -> MOD-006. |
| TST-013 / AIH-005 | PARTLY | Same reading; AIH-005 adds "alongside generated code". SCHEDULED alias AIH-005 -> TST-013 (RULING: the knowledge base lists the agent face on purpose?). |
| AIH-006 / RDB-012 | PARTLY | Near-identical (match local conventions); AIH-006 is Info, RDB-012 Low. Alias candidate. |
| AIH-008 / EXT-007 | DISAGREE | AIH-008 is "nobody asked for it" (agent scope), EXT-007 is "speculative generality" (unused abstractions, checkable). Different evidence. |
| ARC-002 / MOD-002 | DISAGREE | Cycles between components (ARC-002) vs modules (MOD-002); two units, two built checks. |
| CON-007 / ERR-007 | PARTLY | Resource release on error paths overlaps; CON-007 adds tasks and tests. One record would do; alias candidate. |
| SEC-008 / ERR-009 | AGREE | Same statement; ERR-009 covers Rust, SEC-008 does not. SCHEDULED alias SEC-008 -> ERR-009 (fixes B1 for free). |
| WSP-007 / TST-010 | AGREE | Same statement (two-sided contract tests across repositories). SCHEDULED alias TST-010 -> WSP-007. |
| ARC-008 / MOD-008 / EXT-007 | DISAGREE | Pointless layers (ARC-008) / pass-through wrapper (MOD-008) / unused abstractions (EXT-007): three readings; ARC-008 reports MOD-008's shape plus others. All three are contested on purpose. |

**RULING needed (one policy, not nine):** CireSnave accepted TYP-003 -> MOD-010 as an alias; do the six "AGREE" pairs (DOC-006/RDB-010, DOC-002/API-010, EVO-003/MOD-006, SEC-008/ERR-009, WSP-007/TST-010, TST-013/AIH-005) merge the same way, or does the knowledge base keep both ids? Pip's "~200 rules" is not supported: 6 AGREE + 5 PARTLY pairs is 11 records.

**Double-reporting by design: AGREE it is a real design choice.** REL-003 pairs with CON-004 (`src/module/neutral/concurrency.rs`), SEC-006 with SUP-002 (OSV), OPS-006 with SEC-002 (gitleaks narrowed to config files): one site yields two findings under two ids. SCHEDULED: report one finding with both ids (as `aliases`), or fold REL-003 into CON-004 as TYP-003 was. RULING with the policy above.

**Severity versus mission: PARTLY.** ARC has 6 low, 2 medium, 4 info (rules/arc.toml; Pip said 8/2/2), no high: AGREE it is below the mission's weight; ARC-002 and ARC-001 are Low. The reason is GOV-003/GOV-006: precision is unmeasured, and ARC findings are V1 evidence. RULING: raise ARC-002 (cycle) to Medium once its precision is measured on the corpus, not before. **High severity at low confidence (CON-006, ERR-009, SEC-003, SUP-012): AGREE**, those four are the exact set (rules/concurrency.toml:130, err.toml:226, sec.toml:52, sup.toml:273); GOV-006 asks for a stated confidence, which they have, but a high/low pair never gates. SCHEDULED: re-grade each when its check is built (CON-006 is built: measure first).

**Naming split: PARTLY.** Pip's "9 of 14 slugs have checks vs 27 of 217" is stale: **14 of 14** slugs and 52 of 217 numbered. Renaming slugs to ids is a RULING (they are public finding ids and allowlist keys: a rename breaks every `.coderipper.toml`).

**"Dangling alias" TYP-003: DISAGREE.** `aliases = ["TYP-003"]` on MOD-010 is how the catalog resolves TYP-003 (0.41.1; `coderipper check TYP-003` works); the record `TYP-003` does not exist on purpose (the two were one rule).

**"Half-finished rules with no `checked_today`" (ERR-001, ARC-012, TYP-006, EXT-002): DISAGREE.** ERR-001 has one (built, #255). ARC-012, EXT-002 and TYP-006 (#257, unmerged) have no checker on main, so there is nothing to describe; the rule that a checker's PR carries its `checked_today` holds, and #257 does.

## D. What changed in this PR

- `checked_today` for the 29 implemented rules that lacked it (29: arc.toml 10, mod.toml 10 with `reachability`, api 2, err 1, rdb 1, ops 1, sec 2, wsp 2). Each states what the code does today, its severity and confidence, when it is "not run", and "Not judged". Where the code departs from the record's `statement`, the departures are listed in section E of this file.
- `docs/rules/REMAINING.md`: 66 built, 165 rows, plus the scheduled rows of section E.
- `rules/api.toml`: API-003's statement gets the 0.x clause (A10).
- Not changed (needs a ruling): lint-suppression ratchet, aliases, severities, slug names, API-009/ERR-004 readings.

## E. Where the records and the code differ (found while writing `checked_today`)

These are for the next planning pass; none was fixed here.

- **ARC-001:** statement says an allowed-edges list; the code judges layer direction, with `allow` edges and waivers as exceptions. **ARC-002:** statement says loops inside one component are warnings; the code reports none. **ARC-003:** statement says links to volatile components; the code counts identifier tokens, namespaces and feature `cfg` attributes. **ARC-004:** statement says type-resolved facts; the code matches paths through `use`. **ARC-007:** statement says reads or writes; the code matches text and cannot tell them apart. **ARC-008:** two of four statement items (data-mapper chains, long call paths before I/O) are not implemented. **ARC-009:** statement says history; the code compares cycles and surface against one base. **ARC-011:** statement says services and call fan-out; the code reads module pairs in one crate.
- **MOD-002:** "at every level of structure the project declares": only modules (the declared-component level is ARC-002). **MOD-005:** statement says the same codebase and growth; the code compares a pooled corpus of nine repositories and does not measure growth. **MOD-001, 004, 005, 008:** `default_severity` low, but the code emits Info for most findings. **MOD-008:** statement says "or type"; the code reads functions and methods. **MOD-010:** `//!` header says derive bypass is Low; the code emits Info (docs/rules/mod.md agrees with the code). **MOD-006:** records say languages `any`; it reads known code extensions and committed history. **MOD-009:** header says `&self or self`, docs say only `self`.
- **SEC-002:** the record says `default_severity` critical and confidence high; the code emits High, Medium confidence (Low for `generic-api-key`), and scans commits, not uncommitted edits. **OPS-011:** statement says the binary prints its name and crate version; the probe checks that the manifest version appears. **SEC-009:** statement and docs name ARM, Bicep and Serverless; the 37 named checks are AWS, GCP, Kubernetes and Dockerfile ids only. **API-001:** the module doc says a baseline equal to HEAD compares the crate with itself; the tool is pointed at the working tree, so uncommitted changes can be compared. **ci-protection-presence:** `audit_ruleset_rules` hardcodes a positive-control string that need not be true when classic protection satisfied the check. **unused-return-values, unused-parameters:** `cfg(test)` items are not skipped and tests, examples and benches count as call sites (ERR-001 skips them).
- **Pip's severity claim for SEC-002 ("critical 1")** is a record fact (rules/sec.toml:28), and the code does not reach it: see above. RULING: raise the code to Critical or lower the record to High.
