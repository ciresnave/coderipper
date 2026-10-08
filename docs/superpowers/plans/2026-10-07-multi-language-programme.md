# Multi-language programme — phased plan, estimates, open questions

**Status: DRAFT for PM approval. Documentation only.** Companion to `docs/superpowers/specs/2026-10-07-multi-language-design.md`
(the design) and `docs/superpowers/triage/` (the rule classification). Written 2026-10-07 by the CodeRipper lane.

Labels: **MEASURED** = checked in this session at the ref named; **ASSUMED** = reasoned, not checked; **ESTIMATE** = a forecast, with
its basis. Effort is in *lane-days* (one lane working a long day, the unit PR #31 took about 1.5 of, **ASSUMED** from this
session's own history: ten planned commits plus six audit-fix commits plus a CI loop). No estimate here has been tested.

## 0. How this plan is ordered

The order follows the owner's guidance (long-term target quoted in the design, §0): language-neutral and cross-codebase rules
first (they need no parser, and two such checks already exist); then the parser-based core. Each phase ends with something a
person can run and a test that would fail if the phase's claim were false (red first, as in PR #31). Every phase is its own
PR set with its own version bump (the portfolio rule: a change set carries its number), starting with the first *code* PR,
which also covers this docs PR (the PM's ruling, 2026-10-07).

## 1. Phases

### P0 — this design, the triage, this plan (docs only)

- **Delivers:** the design revision; the rule triage (216 rules, one coarse class each, per-language cells for the first-batch
  domains); this plan; the open questions for the owner.
- **Proves:** that the design is coherent against the real rule list (every rule gets exactly one status) and that the first batch
  is bounded. **Does not prove** anything runs.
- **Done when:** the PM approves, and an independent audit of the claims (MEASURED vs ASSUMED) has been answered.

### P1 — the module protocol, and the Rust module behind it (no behaviour change)

- **Delivers:** a `Module` trait mirroring the protocol; an `ExternalModule` adapter (child process, JSON lines, timeouts, kill of
  the process tree, the "no verdict is an error" rule); the five existing checks moved behind `RustModule`; a fake external
  module used only by tests (crash, hang, no summary, a missing rule result, a garbage line, a too-large output).
- **Proves:** the refactor changed nothing: every existing golden/CLI/exit-code test passes **unedited**; the JSON output of a
  fixed fixture is byte-identical before and after; each failure mode of the fake module ends as an error and exit 3, never a
  clean run.
- **Estimate (ESTIMATE):** 2-3 lane-days. Basis: PR #31's shaping work, the closest comparable, plus the process-management
  surface (Windows job objects) that is new. Risk: the Windows process-tree kill.

### P2 — the rule catalog and the coverage report

- **Delivers:** the catalog loader and record format; catalog records for the five legacy rules and the first-batch rules;
  `coderipper conformance`; the coverage report (human and the `coderipper-coverage` JSON line); `claimed-unproven` counted as a
  gap.
- **Proves:** coverage is computed from the catalog joined with each module's `describe`, not asserted; a module claiming a rule
  without a passing fixture shows as a gap; a covered rule that could not run exits 3; a known gap does not.
- **Estimate (ESTIMATE):** 2-3 lane-days.

### P3 — the first language-neutral batch (the neutral module)

- **Delivers:** the first batch named in `docs/superpowers/triage/README.md`: seven delegations through the neutral module (gitleaks,
  osv-scanner for two rules, lychee, zizmor, checkov, buf) and five small native checks (SUP-001, SUP-011, DOC-005, DOC-009, WSP-001),
  each with a seeded-defect fixture. The two checks that exist (`version-consistency`, `ci-protection-presence`) are re-expressed as
  catalog rules, behaviour unchanged. The other neutral natives (8) wait in the native backlog (below).
- **Proves:** delegation works end to end for a real tool (findings mapped to rule IDs through a mapping record) and a missing
  tool is a reported error. **Dependency to flag (see Q5):** this phase needs a minimal tool cache (pinned version, checksum, an
  explicit `--install-tools`); recommending that slice be pulled forward from P6 into P3 rather than delegating to
  tools the developer installed by hand.
- **Estimate (ESTIMATE):** 4-7 lane-days: seven tool mappings at about 0.1-0.3 lane-day each, five small natives at about 0.3-1 each,
  and the catalog records for them. The basis is the five existing checks, each of which was its own plan and PR over about a day
  (MEASURED from the dates in `git log`: 2026-10-01 and 2026-10-02), which sets the 0.3-1 range; the mapping figure is a guess.

### P4 — the TypeScript module (first external module)

- **Delivers:** `coderipper-module-typescript`, an external executable speaking the protocol; detection of `package.json` /
  `tsconfig.json`; the parser-based first-batch rules delegated to the tools the triage names; conformance fixtures.
- **Proves:** a module in a separate process, built and versioned separately from the host, works through the protocol; polyglot
  repositories run the right module per project root; "no module for typescript" shows as gaps, not silence.
- **Scope, as the triage measured it:** 29 delegations (mapping a tool's findings to a rule each) plus module scaffolding, tool pinning and
  conformance fixtures; and **only the first ten or so of the 40 native cells**, chosen by priority. The remaining natives are the
  backlog below, not part of P4.
- **Estimate (ESTIMATE):** 12-23 lane-days: scaffolding and pinning 3-4, 29 mappings at 0.1-0.3 (3-9), ten natives at 0.3-1 (3-10), and
  slack for tool-output surprises. The earlier figure (5-8) assumed mostly wrappers; the measured native count is why it is higher.

### P5 — the Python module

- Same shape as P4 for Python. **Proves** the module interface generalises (a second ecosystem, different tool conventions).
- **Scope:** 28 delegations and the first ten or so of the 40 native cells. **Estimate (ESTIMATE):** 11-21 lane-days, 20-30% cheaper
  than P4 if P4 produced a module SDK worth reusing.

### P6 — the install policy and tool cache, completed

- **Delivers:** the consent flow (flag, config, terminal prompt), the licence ledger (`coderipper tools list`), offline bundles,
  update-by-lock-bump tooling, and the remaining installers (npm with `--ignore-scripts`, pip with `--require-hashes`, `cargo
  install --locked`).
- **Proves:** nothing is ever installed globally or without consent; a tampered download is rejected; the pinned versions are
  what actually ran (recorded in the run's output).
- **Estimate (ESTIMATE):** 3-4 lane-days (less if the P3 slice is pulled forward).

### P7 — the hosted sandbox (design pass first)

- **Delivers:** a design for running modules and tools for untrusted repositories: image build, microVM/sandbox choice, resource
  limits, abuse limits, result storage; then, only if approved, a prototype that runs one scan end to end.
- **Proves:** untrusted code never runs outside the sandbox (a test with a project whose build script and ESLint config try to
  write outside the checkout and reach the network).
- **Estimate (ESTIMATE):** design 2-3 lane-days; the build is not estimated until the design says what it is. The owner has said
  the hosted service is planned; nothing in P1-P6 depends on it.

**Total P1-P6 (ESTIMATE): 34-61 lane-days** for the delegation-first scope (P1 2-3, P2 2-3, P3 4-7, P4 12-23, P5 11-21, P6 3-4), of
which P4 and P5 are the least certain. This is a forecast to be revised after P1, the first phase with real code, and it assumes the
owner's answers to the open questions below do not enlarge the scope.

### The native backlog (not scheduled)

The triage proposes **123 native implementations** (13 neutral, 30 Rust, 40 TypeScript, 40 Python) that CodeRipper would have to write
because no mature tool was named for them; 97 other cells only wrap a tool. At 0.3-1 lane-day for a simple check and 1-3 for a
parser-heavy one (ESTIMATE, same basis as P3), the whole backlog is roughly **40-120 lane-days**, on top of the 34-61 above. P3-P5
take only the highest-value natives. **The coverage percentage a user sees will be decided by which natives get built**, which is the
owner's priority call (Q12), not an engineering default. 48 distinct rules account for these natives.

## 2. What would make us stop and re-plan

- P1 cannot keep the golden outputs byte-identical without editing the golden tests (the "no behaviour change" premise is false).
- A first-batch tool's licence forbids the external-process use, or its rules carry a restrictive licence (the design's tool
  criteria, §7): that rule moves to NOT-COVERED with the reason, and the owner is told.
- Conformance fixtures cost more than the checks they prove (then the "earned" rule needs a cheaper form, and that is a design
  change, not a shortcut).

## 3. Token and effort ledger for P0 (actual, for the cost model)

| Step | Agent tokens (actual) | Basis |
|---|---|---|
| Pass 1 classification (5 Sonnet agents, 216 rows) | 272,721 | the five agents' reported usage: 54,174 + 53,312 + 57,787 + 54,432 + 53,016 |
| Pass 2 per-language cells (3 Sonnet agents; the Rust agent also wrote the neutral column) | 256,719 | 83,841 (Rust + neutral) + 88,684 (TypeScript) + 84,194 (Python) |
| Tool facts | 0 | two scripts against read-only registry and GitHub APIs (`tools/lookup_tools.py`, `tools/verify_delegations.py`) |
| Independent audit | (filled in before READY) | |
| Running total of agent tokens, before the audit | 529,440 | the PM's hard stop is 900,000 |

The estimate for pass 1 was 150,000 and the actual was 272,721 (about 1.8x); for pass 2 the estimate was about 200,000 and the actual was
256,719 (about 1.3x). Cost-model data: a Sonnet classifier that reads about 150 lines of a rule document and writes about 45 rows costs
53,000-58,000 tokens, of which the reading is a small part; budget about 55,000-90,000 per agent, not 30,000-65,000.

The lane's own (Opus) tokens are not in this table because this session cannot meter them separately.

## 4. Open questions for the owner (each with a recommendation)

- **Q1 — Publishing a rule catalog derived from the knowledge base.** May the rule IDs (`ARC-004` ...) and our own paraphrased
  statements be published in this MIT-or-Apache repository, with an attribution line? *Recommend yes*, IDs and paraphrase only,
  never the document's text, with "informed by <the owner's> knowledge base v2" in the catalog header. Needs his decision
  (provenance is his).
- **Q2 — Adopt the knowledge base's rule IDs verbatim as CodeRipper's rule IDs?** *Recommend yes*, keeping the five legacy IDs as
  aliases. One namespace is cheaper than a mapping table nobody maintains.
- **Q3 — Which language after Rust, TypeScript and Python?** The design is language-agnostic. *Recommend* Go next (compiler-
  enforced `internal/`, mature tools, easy single-binary tool installs), then Java/Kotlin, then C/C++ last (build-dependent).
- **Q4 — In what language are the first external modules written?** The protocol allows any. *Recommend Rust for TypeScript and
  Python too* (one toolchain, our existing CI, no second runtime to pin), revisiting only if a module needs its language's own
  compiler API (the TypeScript compiler API, Python `ast`/`libcst`), in which case that part may be a tiny helper in that language.
- **Q5 — Pull the minimal tool cache (pin, checksum, explicit consent) forward from P6 into P3?** *Recommend yes*: delegating to
  tools without it means either hand-installed tools (not reproducible) or the policy violated.
- **Q6 — Does CodeRipper own the architecture-intent specification language** (components, allowed edges, public-surface
  budgets) that about 13 rules (class `A`) need? *Recommend not now*: ship the rules that need no declaration first; decide
  after P3, when we know which `A` rules users ask for. Adopting an existing format (for example the dependency-rule files the
  ecosystem tools already read) is cheaper than inventing one.
- **Q7 — Should a known coverage gap ever fail a run?** *Recommend no by default*; an opt-in `--min-coverage <pct>` for teams that
  want a floor. A gap is information, not a defect in the analysed code.
- **Q8 — Tools whose licence or rule packs are restrictive** (commercial analysers, rule packs under non-OSI terms). *Recommend* the
  default catalog uses only tools whose licence allows external-process use and whose rules are redistributable-or-not-needed; the
  rest are listed as optional integrations the user installs themselves, with the licence shown.
- **Q9 — Suppressing findings from delegated tools** that carry no symbol. *Recommend* modules supply a stable enclosing-item
  `subject` where they can; where they cannot, an allowlist entry is `check + file` with an optional line range, and its stale
  detection is "the rule ran to completion and found nothing at that place".
- **Q10 — The LLM-judgment tier (the knowledge base's V4).** *Recommend not before P6*, and not as a gate until a labelled corpus
  shows its precision; the design reserves the place for it in the protocol and states the guardrails (design §12).
- **Q11 — Default install consent in CI.** *Recommend never by default*, with a clear message and the exact command (design §8);
  CI that wants installs passes `--install-tools` on purpose.
- **Q12 — Which natives are worth building?** 123 are proposed (the backlog above); the owner's priorities decide the order.
  *Recommend* building natives only when a user asks for the rule or a conformance fixture shows a delegation is not enough, and
  reporting the rest as `NOT-COVERED` honestly rather than as planned coverage.
