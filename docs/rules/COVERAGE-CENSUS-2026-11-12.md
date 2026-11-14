# Coverage census: what each language has built, and what Rust still lacks

**Measured 2026-11-12 at `origin/main` 9da0f91 (0.43.59), a release build of that commit.** Written for the PM task "languages and Rust completeness"
(CireSnave 2026-11-12: "We commonly do TypeScript, JavaScript, C, C++, Slang, and CUDA builds. Are those all on CodeRipper's roadmap to add support for?" and "I doubt we have
checks for all rules for Rust under CodeRipper. Lets get all of those languages plus completing our checks for rules for Rust code on the roadmap if they aren't already.
The checks that require a model's judgement, those can wait until we have more of our ML stack ready").

## Method (so it can be repeated)

1. **Denominator:** every `[[rule]]` in `rules/*.toml` at that ref: **233 rules**. Each record names the languages it applies to (`[rule.applicability] languages`):
   183 `any`, 35 `rust, typescript, python`, 12 `rust`, 3 `typescript, python`. No record names C, C++, CUDA, Slang, HLSL, GLSL or JavaScript.
2. **Built, per language:** the binary's own coverage report, `coderipper fast --profile extended --coverage=full`, run on a one-file Rust repository and on a repository holding a
   `package.json` and a `.ts` file. The report counts a rule as covered only when a conformance fixture proves it, and lists the ids that are "not yet implemented"
   and those "implemented, not yet proven". **built = applicable - not yet implemented** (an unproven rule runs, so it is built).
3. **MECHANICAL or MODEL, per unbuilt Rust rule:** the "input needed" column of `docs/rules/REMAINING.md` (2026-11-07, the lane's per-rule judgment, not a measurement):
   `a model` is MODEL; every other input (source, history, network, a runtime) is MECHANICAL. All 128 unbuilt Rust ids have a row there (checked by script: 0 unclassified).

## The headline numbers

| Language family | Rules that apply | Built | Unbuilt | How it is checked today |
| :--- | ---: | ---: | ---: | :--- |
| **Rust** | 230 | 102 (100 covered + 2 implemented, not yet proven) | **128** | the built-in Rust module (`syn`, `cargo`, rustc diagnostics), the neutral module and 9 delegated tools |
| **TypeScript** | 221 | 32 | 189 | no module: only the language-neutral rules run (31) plus 1 more; "1 source file not checked" |
| **JavaScript** | not in the catalog (read as TypeScript: 221) | 0 language-aware | - | no roadmap row of its own; `src/coverage.rs` `LANGUAGES` lists `js`, `jsx`, `mjs`, `cjs` as TypeScript source, so a JavaScript project reads as "typescript: no module" |
| **C, C++, CUDA** | 183 (the `any` rules; no record names them) | 0 language-aware; 31 language-neutral | - | not in `src/coverage.rs` `LANGUAGES` (it knows only python and typescript), so a C, C++ or CUDA project is not even reported as an unchecked language |
| **Slang, HLSL, GLSL (shader code)** | 183 (the `any` rules) | 0 language-aware; 31 language-neutral | - | not in `LANGUAGES`; not reported as unchecked |
| **Language-neutral** (`languages = ["any"]`) | 183 | 31 | 152 | the neutral module: files read as text, git history, CI and repository metadata |

Python has the same shape as TypeScript (221 apply) and is outside this task. The 31 neutral rules are the same 31 for every language, so "32 of 221" for TypeScript is those 31 plus one rule
the TypeScript-applicable set adds.

**Unbuilt Rust rules by kind (128):** MODEL **23** (tag `needs-model`, parked until the ML stack is ready, not declined); MECHANICAL **105**, of which by what they need:

| Input needed (REMAINING.md) | Rules | Buildable now? |
| :--- | ---: | :--- |
| source only | 41 | yes, from the committed tree |
| a runtime | 34 | yes, but it runs project code (tests, benchmarks, probes): the self-hosted-runner rule applies |
| history | 15 | yes, from git history |
| source only (meta) | 9 | yes: rules about CodeRipper itself (GOV) |
| history + network | 3 | yes, with the forge and history |
| network | 2 | yes, with a registry or the forge |
| the commit's CI status | 1 | yes, with the forge |

## Per domain: built of applicable, three families

| Domain | Rust | TypeScript | Language-neutral |
| :--- | ---: | ---: | ---: |
| AIH | 0 / 9 | 0 / 9 | 0 / 9 |
| API | 5 / 12 | 3 / 12 | 3 / 12 |
| ARC | 11 / 12 | 0 / 12 | 0 / 8 |
| CON | 8 / 11 | 0 / 9 | 0 / 8 |
| COR | 4 / 10 | 0 / 9 | 0 / 5 |
| DOC | 3 / 10 | 3 / 10 | 3 / 10 |
| EFX | 3 / 7 | 1 / 7 | 1 / 7 |
| ERR | 4 / 12 | 0 / 11 | 0 / 3 |
| EVO | 0 / 9 | 0 / 9 | 0 / 9 |
| EXT | 4 / 9 | 0 / 10 | 0 / 4 |
| GOV | 2 / 11 | 1 / 10 | 1 / 10 |
| MOD | 10 / 11 | 0 / 10 | 0 / 2 |
| OPS | 5 / 12 | 2 / 10 | 2 / 10 |
| PRF | 2 / 8 | 0 / 8 | 0 / 8 |
| RDB | 4 / 14 | 1 / 13 | 1 / 13 |
| REL | 5 / 12 | 2 / 12 | 2 / 12 |
| SEC | 7 / 9 | 5 / 10 | 4 / 5 |
| SUP | 12 / 17 | 10 / 16 | 10 / 16 |
| TST | 4 / 13 | 0 / 13 | 0 / 13 |
| TYP | 3 / 8 | 0 / 8 | 0 / 8 |
| WSP | 6 / 14 | 4 / 13 | 4 / 11 |
| **all** | **102 / 230** | **32 / 221** | **31 / 183** |

## The 128 unbuilt Rust rules

Default-on policy (CireSnave, board 285): a built rule is default on and nothing is skipped; a rule still opt-in because it fired on too many non-defects is recorded in `src/module/neutral/mod.rs::opt_in_rule_ids`, not here.

### MODEL: `needs-model` (parked until the ML stack is ready; not declined): 23

| id | title | input needed |
| :--- | :--- | :--- |
| AIH-003 | New helpers do not repeat existing utilities | a model |
| AIH-004 | A change stays within the task it was given | a model |
| AIH-005 | Tests check the specification, not the code's own output | a model |
| AIH-006 | Generated code follows the local conventions | a model |
| AIH-008 | No abstractions or options nobody asked for | a model |
| API-012 | Unpromised behaviour not accidentally relied upon | a model |
| ARC-010 | Changes respect recorded architectural intent | a model |
| CON-009 | Cancellation safety is documented | a model |
| COR-005 | Requirements and doc claims traced to asserting tests | a model |
| DOC-006 | Docs and comments match current behaviour | a model |
| ERR-010 | Compound state changes say whether they are all-or-nothing | a model |
| EXT-010 | Abstraction cuts along the true dimension of variation | a model |
| MOD-007 | Stated responsibility compared with declared purpose | a model |
| RDB-009 | Names tell the truth about behaviour | a model |
| RDB-010 | Comments and docstrings match current behaviour | a model |
| RDB-011 | Terminology follows the project glossary | a model |
| RDB-012 | Additions match how the repository already works | a model |
| RDB-013 | Diff readability against a rubric | a model |
| SEC-010 | Model-assisted review of risky changes | a model |
| TST-008 | Test names agree with assertions | a model |
| TST-013 | Expected values not derived from the implementation | a model |
| TYP-008 | Invariants are written down and checked | a model |
| TYP-009 | State machines make transitions explicit | a model |

### MECHANICAL: buildable now: 105

| id | title | input needed |
| :--- | :--- | :--- |
| AIH-001 | One policy for humans, agents and CI | source only |
| AIH-002 | Referenced APIs and new packages really exist | network |
| AIH-007 | AI-assisted commits are marked in their trailers | history |
| AIH-010 | The tests ran, and passed, before the work was called done | the commit's CI status |
| API-002 | Public surface snapshot reviewed when changed | a runtime |
| API-003 | Version number matches kind of change | history |
| API-005 | Serialized formats tested for old and new readers | a runtime |
| API-008 | Public API exposes no third-party types | source only |
| API-010 | Doc samples build and execute | a runtime |
| API-011 | Command-line, config and exit-code surfaces under test | a runtime |
| CON-001 | No data races | a runtime |
| CON-007 | Deterministic release of resources | a runtime |
| COR-002 | Invariant types enforce invariants in code | source only |
| COR-004 | Reference-implementation algorithms get differential tests | a runtime |
| COR-008 | Critical protocols modelled and code traced to model | a runtime |
| COR-009 | Critical functions have proof harnesses in CI | a runtime |
| COR-010 | Idempotency and ordering claims tested with replay | a runtime |
| DOC-001 | Public items are documented | source only |
| DOC-002 | Documentation examples build and run | a runtime |
| DOC-003 | Components carry a purpose and an owner in the spec | source only |
| DOC-004 | Significant architectural changes come with a decision record | history |
| DOC-007 | Diagrams that have drifted from the dependency graph | source only |
| DOC-008 | Changes users can see get changelog entries | history |
| EFX-001 | Components declared pure avoid effectful APIs | source only |
| EFX-004 | Deterministic algorithms give identical results every run | a runtime |
| EFX-005 | Methods that both change state and answer queries | source only |
| EFX-006 | Functions claimed pure are pure | source only |
| ERR-002 | Catch-all handlers only at boundaries | source only |
| ERR-003 | No panics or unchecked indexing on input paths | source only |
| ERR-005 | Cross-boundary errors keep context and cause | source only |
| ERR-006 | Domain errors free of transport details | source only |
| ERR-007 | Resources released on error paths | source only |
| ERR-008 | Error branches covered by tests | a runtime |
| ERR-011 | Non-throwing parse results are checked | source only |
| EVO-001 | Hotspot ranking from churn, complexity and defects | history |
| EVO-002 | Hidden coupling found through co-change | history |
| EVO-003 | Divergent change inside a module | history |
| EVO-004 | Sole-owner risk in critical components | history |
| EVO-005 | Erosion trend of violations and waivers | history |
| EVO-006 | Stale rules and specs for untouched components | history |
| EVO-007 | Defect clustering by component | history + network |
| EVO-008 | Change-size distribution | history + network |
| EVO-009 | Per-function complexity trend | history |
| EXT-006 | Shared contract tests for every implementation | a runtime |
| EXT-007 | Speculative generality | source only |
| EXT-008 | Shotgun surgery when adding a variant | history |
| EXT-009 | Stable components edited for new variants | history |
| GOV-001 | Baseline findings and ratchet downward | source only (meta) |
| GOV-002 | Waivers with owner and expiry | source only (meta) |
| GOV-003 | Rule maturity stages driven by precision | source only (meta) |
| GOV-004 | Policy versioned, reviewed and tested | source only (meta) |
| GOV-005 | Classify code into analysis scopes | source only (meta) |
| GOV-006 | Every finding states severity, confidence, proof and a fix | source only (meta) |
| GOV-007 | Report per dimension, never one score | source only (meta) |
| GOV-008 | Layered gates from editor to runtime | source only (meta) |
| GOV-009 | Rules ship rationale and remediation guidance | source only (meta) |
| OPS-001 | One typed, validated configuration entry point | source only |
| OPS-002 | Config keys are documented, defaulted and used | source only |
| OPS-004 | Trace context follows outbound calls | a runtime |
| OPS-005 | Golden-signal metrics for endpoints and dependencies | a runtime |
| OPS-007 | Flags need an accountable owner and a sunset date | source only |
| OPS-008 | Liveness and readiness endpoints exist | a runtime |
| OPS-010 | Build and version information is exposed | source only |
| PRF-001 | Hot paths have budgets enforced by benchmarks | a runtime |
| PRF-002 | Statistically detected benchmark regressions | a runtime |
| PRF-004 | Hot loops tagged for speed allocate nothing | source only |
| PRF-006 | Performance exceptions documented with benchmark link | source only |
| PRF-007 | Critical query plans avoid full table scans | a runtime |
| PRF-008 | Tail latency tracked beside averages | a runtime |
| RDB-001 | Code matches the project formatter | source only |
| RDB-002 | Identifiers follow language naming conventions | source only |
| RDB-003 | Cognitive complexity stays within bounds | source only |
| RDB-005 | Duplicated knowledge across clone groups | source only |
| RDB-006 | No dead code, unused imports or unused private items | source only |
| REL-002 | Retries follow a safe policy | source only |
| REL-004 | Idempotent message handlers | a runtime |
| REL-006 | Graceful shutdown | a runtime |
| REL-008 | Fault-injection tests for declared failures | a runtime |
| REL-010 | Crash-recovery tests for stateful components | a runtime |
| REL-011 | Breakers and bulkheads where required | source only |
| REL-012 | SLO conformance checked at runtime | a runtime |
| SEC-001 | No unsanitised taint flow to dangerous sinks | source only |
| SUP-003 | Dependency licences satisfy policy | source only |
| SUP-006 | Declared dependencies that are never used | source only |
| SUP-007 | Justification record for each new dependency | history + network |
| SUP-012 | New package names screened for squatting | network |
| SUP-013 | Only audited versions in sensitive tiers | source only |
| TST-001 | Changed-code coverage meets threshold | a runtime |
| TST-002 | Mutation score on the diff meets threshold | a runtime |
| TST-005 | Intermittent tests are assigned and time-boxed | a runtime |
| TST-007 | Bug fixes ship a regression test | history |
| TST-010 | Cross-component interfaces have two-sided contract tests | a runtime |
| TST-011 | Test order and parallelism do not change results | a runtime |
| TST-012 | Test duration budgets with size classes | a runtime |
| TYP-001 | Public APIs wrap domain values in dedicated types | source only |
| TYP-004 | Outside data becomes domain types at the boundary | source only |
| TYP-007 | Boolean blindness in signatures | source only |
| WSP-002 | Cross-repository dependencies point the right way and do not loop | source only |
| WSP-003 | Internal library versions stay within the skew policy | source only |
| WSP-004 | Widely used libraries are tested against their dependents before release | a runtime |
| WSP-005 | Organization policy is inherited and exceptions are waived | source only |
| WSP-006 | Copied code across repositories | source only |
| WSP-007 | Interfaces between repositories have contract tests | a runtime |
| WSP-008 | Conventions do not drift between repositories | source only |
| WSP-009 | Repositories forced to release together | history |

## What the numbers do not say

- "Built" is "has a check with a claim", the report's own word; it is not "finds every instance". Many built rules are narrow proxies (REMAINING.md says which).
- The MODEL tag is the lane's judgment of 2026-11-07, applied to today's unbuilt set; CireSnave or the PM can retag a row. A rule tagged MECHANICAL with input `a runtime` may still need the self-hosted runner.
- The 2026-11-07 table counted 231 rules and 165 unbuilt for all languages (a different predicate: no check of any kind); this census counts Rust-applicable rules only (230), which is why it is 128.
- The neutral module reads some Rust only (it uses `syn` for a few rules); the language-neutral column counts a rule built for every language only as far as the catalog's `any` says. Whether each of the 31 truly reads C or CUDA is what the language-options note's "first rules per language" measures when built.

— lane: coderipper
