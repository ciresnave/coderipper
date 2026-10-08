# CodeRipper — multi-language design (revision)

**Status: DRAFT for PM approval. Documentation only; no code is written against this until it is approved.**
Written 2026-10-07 by the CodeRipper lane. Supersedes parts of `2026-09-30-audit-host-design.md` (table in §0).

Evidence labels used throughout: **MEASURED** = read or run in this session, at the ref named. **ASSUMED** = reasoned or
remembered, not checked. A claim with no label is a decision (it is ours to make), not a fact.

## 0. What this revision changes

CodeRipper started as a Rust auditor and became a host with pluggable checks (`2026-09-30-audit-host-design.md`). Its
owner's long-term target, relayed by the portfolio PM on 2026-10-07 and quoted here verbatim:

> "CodeRipper is not just for checking Rust code.  It is designed to check all languages across multiple codebases."

> "CodeRipper should automatically install the best tools for any language it is checking if they aren't already installed
> and leverage those tools to verify that code meets the expectations of rules those tools cover well.  Perhaps each
> language should have a module that plugs into CodeRipper which is automatically loaded when CodeRipper needs to check
> that language.  That module would have implementations, call-outs to external tools, and skips where rules are not
> applicable for every rule that CodeRipper checks.  Those rules should likely be a common list across all languages."

> "I am fine with an external executable that CodeRipper calls for a language with a well defined interface between
> CodeRipper and that language-specific rule checker."

The criterion for building a check *into* CodeRipper is "any rule that may not be sufficiently checked through some software
that comes with a language by default"; mature third-party tools are integrated, not rebuilt.

| Old section | Status after this revision |
|---|---|
| §1 two axes (Scope, Network) and the fast/sweep tiers | **Kept.** §10 below says how they apply to modules. |
| §2 the `Finding` schema | **Extended, additively** (§10.3). Nothing is removed or renamed. |
| §3 reachability and §5 the check sketches | **Kept as written.** They are Rust (or language-unspecified) checks; each becomes a rule record in the catalog (§4) behind the built-in Rust module (§11). |
| §4 allowlist / suppression | **Kept.** Keyed as before; see §10.4 for delegated findings. |
| §7 "Plugin loading: compiled-in checks for v1" | **Superseded** by modules (§5). Compiled-in stays as one kind of module. |
| §7 "hosted mode needs its own trust-boundary pass" | **Started** in §9; the hosted infrastructure itself is still not designed. |

What is **not** in this revision: any code; the LLM-judgment evidence tier (§12); the architecture-intent specification
language (§12); the hosted infrastructure (§9 states only the posture).

## 1. Vocabulary (two different "tiers" exist; this is why)

- **Rule**: one checkable claim with a stable ID (`ARC-004`, or a legacy ID such as `reachability`). A rule says *what must
  hold*, not how it is checked. The set of rules is the **catalog** (§4) and is the same for every language.
- **Module**: the thing that checks rules for one language (or for no language: the **neutral** module). A module is either
  **built in** (compiled into the `coderipper` binary) or **external** (a separate executable speaking the protocol of §5).
- **Tool**: a third-party program a module calls (clippy, ruff, eslint, cargo-deny, ...), always as an external process.
- **Check** (legacy word): what the code calls one Rust implementation of one rule today (`reachability`, ...). After P1 a
  check is the Rust module's implementation of a rule; its ID is unchanged.
- **Run tier** (`fast` | `sweep`): *when* a rule runs, decided by the Network axis (existing, MEASURED: `src/check.rs`
  `Network::tier`, ref `75ca35a`). Unchanged.
- **Evidence tier** (`V0`..`V5`): *how trustworthy* a rule's evidence is. This is the knowledge base's vocabulary (exact,
  heuristic-structural, empirical, historical, semantic, attested). It is **metadata on a rule**, shown in reports and used
  for default gating; it is not a run tier. We never write bare "tier" for the second one.
- **Coverage status**: exactly one of four values per rule per language (§6). **Run outcome**: what happened in *this* run
  (§6.3). They are different and the report keeps them apart.

## 2. Principles (each is a requirement on the sections below)

1. **A silent skip must never read as clean.** Every rule a run was supposed to judge ends the run as a verdict (ran) or an
   explicit, reasoned non-verdict (skipped with a reason, or an error). A rule with no outcome is synthesised into an error
   by the host. This is the existing rule ("an unreadable thing is an error, never clean", README) generalised.
2. **A covered claim is earned.** A module may claim a rule only with a seeded-defect fixture that the module must find and
   a clean fixture it must not (§6.2).
3. **Orchestrate, do not rebuild.** If a mature, maintained, licence-compatible tool covers a rule, the module delegates and
   maps the result; CodeRipper writes its own check only where no default tool covers the rule.
4. **No behaviour change for what exists.** The five Rust checks, their IDs, JSON lines, exit codes and allowlist semantics
   are frozen by the golden tests that already exist (`tests/golden.rs`, `tests/message_format.rs`, `tests/exit_codes.rs`).
   Refactoring them behind the module interface (P1) must leave those tests green and unedited.
5. **Pinned, isolated, consented tools.** Nothing is installed globally, nothing floats to "the latest at run time", and
   nothing is installed without consent (§8).
6. **The code under analysis is untrusted input** wherever CodeRipper is not running on the user's own checkout (§9).
7. **Thresholds are defaults, never laws.** A rule with a number ships a default and a per-project override with a recorded
   reason.

## 3. Architecture

```
 coderipper (CLI) / library
   │  detects languages per project root (manifest files), reads .coderipper.toml (profile, allowlist)
   ▼
 HOST  ── rule catalog (rules/*.yaml) ─ decides WHICH rules run (profile, run tier, unit, scope)
   │          │
   │          ├── built-in module: rust      (the five checks, in process, same interface)
   │          ├── built-in module: neutral   (rules that need no parser: manifests, CI, lockfiles, docs, VCS)
   │          └── external modules: typescript, python, ...   (separate executables, protocol of §5)
   │                      │
   │                      └── tools, from the isolated tool cache (§8): eslint, ruff, cargo-deny, ...
   ▼
 findings ─► suppression/allowlist (host, unchanged) ─► report: findings + coverage + exit code
```

The host knows no language. A module owns everything language-specific: how to build the facts, which tool covers which
rule, how a tool's output maps to a rule ID and a `Finding`.

## 4. The rule catalog

### 4.1 Record format

One YAML record per rule, adopting the knowledge base's rule-record shape (its section III.6) with CodeRipper's additions
(marked *new*). Guideline text, agent-facing instruction text and the machine description come from this one record, so
they cannot drift apart. Records live in `rules/<domain>.yaml` (one file per domain; a list of records).

```yaml
- id: SUP-003                  # stable; KB IDs are adopted verbatim, legacy IDs (reachability, ...) are kept as they are
  title: ...                   # our own words
  domain: SUP
  statement: >                 # our own paraphrase of the claim; never the knowledge base's text
    ...
  rationale: >
    ...
  evidence_tiers: [V0]         # V0..V5 (§1): metadata, not a run tier
  evidence_strength: C         # E | C | F: empirical / consensus / folklore
  contested: false
  scope: project               # project | portfolio             (existing axis, §1 of the old design)
  network: local               # local | network                 (existing axis; decides the run tier)
  unit: package                # package | repository            (existing: Check::unit)
  executes_project_code: false # NEW: does checking it run build scripts, config, plugins, tests? (§9)
  applicability: { languages: [any], scopes: [first_party] }   # NEW use of the KB's field
  default_severity: error
  confidence_class: high
  thresholds: { type: absolute, value: 0 }    # optional; defaults only (principle 7)
  aliases: [version-consistency]              # NEW: legacy IDs this record also answers to
  kb_refs: []                                 # NEW: knowledge-base IDs that overlap, for rules that are ours
  remediation: >
    ...
  related: [SUP-001]
  lifecycle: advisory          # experimental | advisory | blocking (KB governance)
```

`lifecycle` is the default gating level; the project profile can raise or lower it (`--deny` still decides the exit code).

### 4.2 Provenance

The catalog is written in our own words. The knowledge base this design was prepared from is not copied into this
repository (its provenance and licence are its owner's call); rules are cited by ID and paraphrased. Whether KB IDs may be
published as the catalog's IDs is an open question (Q1, `2026-10-07-multi-language-programme.md`).

### 4.3 Legacy IDs

The five existing checks keep their kebab-case IDs. Each becomes a catalog record (`id: reachability`, ...) whose
`kb_refs` names the overlapping KB rules, found in the triage (`docs/superpowers/triage/`). Allowlist entries keyed by
`reachability` etc. keep working unchanged. A rule is addressed by its `id` or any `alias`; `Finding.check_id` carries the
`id`.

### 4.4 Profiles and thresholds

`.coderipper.toml` already holds `[[allow]]` and `[[tracks]]` (MEASURED: `tests/golden/coderipper.toml`, ref `75ca35a`).
New, additive tables, all optional:

```toml
[profile]
name = "default"                       # which defaults to start from
[rules.RDB-003]
level = "error"                        # off | advisory | blocking (never silently on/off without a recorded reason)
threshold = { warn = 15, error = 30 }
reason = "table-driven parser"         # required whenever a default is overridden
[languages]
typescript = { enabled = true }
```

An override without a `reason` is a configuration error (exit 2), the same discipline as an allowlist entry.

## 5. The module protocol

### 5.1 Shape

A module is an executable named `coderipper-module-<name>` with two subcommands. It is **stateless per invocation** (no
daemon in v1: simpler to sandbox, time out and kill; a long-lived mode for IDEs is a later, additive extension).

- `coderipper-module-<name> describe` — prints one JSON object (the *hello*, §5.3) on stdout and exits 0.
- `coderipper-module-<name> check` — reads **one JSON object** (the request, §5.4) from the first line of stdin, writes
  **newline-delimited JSON events** on stdout (§5.5), exits 0 when it finished (findings or not), non-zero when it
  crashed. stderr is for humans and is never parsed; the host captures its tail for error messages.

Why reuse the existing JSON lines: the CLI already prints `{"reason":"coderipper-finding", ...Finding fields}` and
`{"reason":"coderipper-summary", ...}` (MEASURED: `src/cli.rs`, `tests/message_format.rs`, ref `75ca35a`). The module's
finding line is **byte-compatible** with that finding line, so a module can be written in any language by printing the same
shape, and the host forwards a module's finding lines almost verbatim. The CLI's own `--message-format json` output is
unchanged (principle 4); the coverage report (§6.4) is a *new* line, never a field added to the summary (a golden test pins
the summary's exact key set).

### 5.2 Discovery and trust

A module is arbitrary code, so it is not found by "whatever is on PATH". The host resolves a module name only from (a) the
built-in set, (b) the tool cache's `modules/` directory (checksummed, §8), or (c) a path configured explicitly in the
user's own configuration (not the analysed project's). A project's `.coderipper.toml` can *enable* a module; it can never
name an executable path. (Rationale: analysing a repository must not let that repository choose what to run.)

### 5.3 `describe` (the hello)

```json
{"reason":"coderipper-module-hello",
 "module":"typescript","module_version":"0.1.0",
 "protocol":["1.0"],
 "languages":["typescript","javascript"],
 "detect":["package.json","tsconfig.json"],
 "capabilities":{"executes_project_code":true,"needs_network":false,"incremental":false},
 "tools":[{"name":"eslint","version":"9.12.0","licence":"MIT","lock":"tools.lock#eslint"}],
 "rules":[
   {"id":"MOD-002","status":"delegated","tool":"dependency-cruiser","mapping":"mappings/dependency-cruiser.toml",
    "proof":"conformance/MOD-002"},
   {"id":"MOD-001","status":"implemented","proof":"conformance/MOD-001"},
   {"id":"EFX-004","status":"not_applicable","reason":"no equivalent in this language"}
 ]}
```

`status` is one of `implemented` (the module's own code), `delegated` (a tool, with the mapping from the tool's findings to
the rule), `not_applicable` (reason required). A rule the module does not list is **not covered** by it; the host derives
that status (§6.1). `proof` points at the conformance fixture (§6.2).

### 5.4 The request

```json
{"reason":"coderipper-check-request","protocol":"1.0","run_id":"9f3c...",
 "unit":"package","project_root":"/abs/readonly/checkout","member":{"name":"web","dir":"/abs/readonly/checkout/web"},
 "run_tier":"fast","rules":["MOD-001","MOD-002"],
 "rule_config":{"MOD-002":{"level":"blocking"}},
 "trust":"untrusted","network":false,"tools_dir":"/abs/cache/tools",
 "limits":{"wall_secs":600,"idle_secs":120,"max_findings":5000,"max_output_bytes":16777216}}
```

The host chooses `rules` (profile, run tier, unit, scope); the module checks exactly those. `project_root` is a **throwaway
checkout** the host made (today's session/worktree machinery, MEASURED: `src/worktree.rs`, `src/session.rs`), never the
user's working tree. A module that must rewrite files to analyse (the Rust reachability trick) does so inside that
checkout, as the built-in Rust module already does.

### 5.5 Events from the module

All have a `reason`. Unknown reasons are ignored (forward compatibility).

| `reason` | Required | Meaning |
|---|---|---|
| `coderipper-finding` | per finding | The existing finding shape plus optional `rule`, `language`, `tool` (§10.3). |
| `coderipper-rule-result` | **exactly one per requested rule** | `{rule, status: "ran" \| "skipped" \| "error", findings: n, tool?: {name,version}, reason_code?, detail?}` |
| `coderipper-progress` | optional | `{done, total, what}` for a long run; display only. |
| `coderipper-module-summary` | exactly once, last | `{rules_requested, rules_ran, rules_skipped, rules_errored, findings, incomplete: bool}` |

`skipped` is legal only with a `reason_code` from a closed set (`not_applicable`, `disabled_by_profile`, `out_of_scope`,
`not_in_this_unit`) and is printed in the report. Everything else that stops a rule from giving a verdict is `error` with
an `error_kind` from a closed set: `tool_missing`, `tool_failed`, `tool_output_unreadable`, `timeout`, `config_invalid`,
`untrusted_refused`, `internal`. **The host never accepts a missing `coderipper-rule-result`**: a requested rule with none
becomes `error: no verdict`; a missing `coderipper-module-summary`, a non-zero exit, or output past the limits makes every
rule without a result `error` and the module run `incomplete`.

### 5.6 Versioning

`protocol` is `"major.minor"`. A module lists the majors it speaks; the host picks the highest major both support and sends
it in the request. A minor bump only adds optional fields and new `reason`s (receivers ignore what they do not know). A
major bump may change a required field; the host supports the current and the previous major for at least one release.
The *finding* line has its own stability promise: it is the same contract as `--message-format json` (golden-tested), so it
changes only additively.

### 5.7 Timeouts, limits, determinism

- The host enforces `wall_secs` and `idle_secs` (no output for that long) and `max_output_bytes`. On expiry it terminates the
  process tree (a job object on Windows, a process group elsewhere), keeps the findings already received, and marks the run
  `incomplete`; every rule without a result is an `error: timeout`. Partial findings are shown and flagged, never used to
  claim a clean rule.
- Module output order is not significant; the host sorts (the existing triage order).
- Environment: the module is started with a scrubbed environment (an allowlist of variables), the checkout as its working
  directory, and no network unless the request grants it.

## 6. Coverage

### 6.1 Four statuses, one per rule per language

| Status | Meaning |
|---|---|
| `implemented-native` | The module's own code checks it. |
| `delegated:<tool>` | A named tool checks it; the module maps the tool's findings to the rule ID. |
| `not-applicable` | The rule does not apply to this language; a reason is recorded. |
| `NOT-COVERED` | A known gap. This is also what a rule the module never mentions is. |

The triage table (`docs/superpowers/triage/`) is the *plan* version of this matrix; at run time the matrix is **catalog
joined with each module's `describe`**, so the report cannot disagree with what the modules actually claim.

### 6.2 Earned, not asserted

A module may claim `implemented` or `delegated` for a rule only with a **conformance fixture** (the `proof` in §5.3): a
small project containing a seeded defect annotated with the rule ID and line (`// expect: ARC-004` in a comment in the
language's own comment syntax), and a clean twin that must produce nothing (the absence-claim discipline: a query that never
finds anything proves nothing, so the clean twin is paired with a defective one).
`coderipper conformance --module <name> [--rule <id>]` runs every fixture offline against the module and fails if a seeded
defect is not reported at its line or a clean twin reports anything. A claim whose fixture is missing or failing is shown
as `claimed-unproven` and **counted as a gap**, not as coverage.

### 6.3 Run outcomes (this run, not the matrix)

For each in-scope rule: `clean`, `findings`, `skipped (reason)`, or `could-not-run (error_kind)`. A rule that is `NOT-COVERED`
is not a run outcome: it is listed in the gap list and does not fail a run on its own. A covered rule that
`could-not-run` fails the run with exit code 3, exactly as a check that cannot run does today (MEASURED: `tests/exit_codes.rs`).

### 6.4 The report

Human form (stderr or stdout under a flag; exact layout decided in P2):

```
coverage: rust        61 of 217 covered (28.1%), 40 gaps listed          this run: 58 clean, 3 findings, 0 could not run
coverage: typescript  14 of 217 covered ( 6.5%), 203 gaps (use --coverage=full to list)   this run: ...
```

JSON form: one new line `{"reason":"coderipper-coverage","language":"rust","rules_total":217,"covered":61,"claimed_unproven":2,
"not_applicable":9,"not_covered":145,"gaps":["RDB-004", ...],"run":{"clean":58,"findings":3,"skipped":0,"could_not_run":0}}`
per language, emitted before the summary line. `--min-coverage <pct>` (a later, opt-in flag) can turn a coverage floor into
a failure; by default a known gap never fails a run.

## 7. Delegating to tools

A `delegated` rule has a **mapping record** in the module: for each tool rule or code (`ruff F401`, `eslint no-unused-vars`,
`clippy::...`), the catalog rule ID it evidences, the severity and confidence to report, and how to fill `location`,
`subject` and `summary`. Tool output is read in its **machine-readable form** (SARIF where the tool offers it, else its JSON);
scraping human text is not allowed. A tool whose output cannot be parsed is `error: tool_output_unreadable`, not "no
findings".

Tool choice criteria (all must hold, and are recorded per tool in the module's `tools.lock` and the triage table):
actively maintained (a release or commit in the last 12 months, checked on the date recorded); licence compatible (we keep
licences and credit; the tool runs as an **external process, never linked**; a tool whose licence forbids that use, or whose
*rules* carry a restrictive licence, is excluded from the default catalog and listed as optional); measured precision where
anyone has published it (otherwise the rule's confidence is capped at Medium until the module's own fixtures say more);
machine-readable output.

## 8. Installing tools

All of this is a **policy**, enforced by the host and the module SDK, not left to each module.

- **Isolated tool cache**: `<cache root>/tools/<tool>/<version>/`, beside the existing build cache (MEASURED: `src/build_cache`,
  `CODERIPPER_CACHE_DIR`; the tools directory would add `CODERIPPER_TOOLS_DIR`). Tools are invoked by absolute path from
  there. Nothing is written to a global PATH, a global npm prefix, `~/.cargo/bin`, or site-packages.
- **Exact versions, checksums**: each module ships a lock file (`tools.lock`) with, per tool and platform, the version, the
  source (URL or registry coordinate), the **SHA-256** and the SPDX licence. A download whose checksum does not match is
  deleted and is `error: tool_missing`. Installers allowed: a checksummed prebuilt binary; `cargo install --locked --root
  <cache>`; `npm install --prefix <cache> --ignore-scripts` with the lockfile's integrity hashes; `pip` into a cache venv with
  `--require-hashes`. Install-time scripts are disabled wherever the ecosystem allows it (ASSUMED per ecosystem; each module
  states what it could not disable).
- **Consent**: a tool is installed only when the user said so: `--install-tools`, or `[tools] install = "yes"` in the user's own
  configuration, or an interactive yes on a terminal. The default in CI (no terminal) is **never**: a missing tool is
  `could-not-run (tool_missing)` and the message prints the exact command that would install it.
- **No "best tool at run time"**: the tool and version a module uses are whatever its lock says. Updating a tool is a
  reviewed change to the lock (a module release), because "latest" drifts and breaks reproducibility. (This is also how the
  portfolio's rule of staying on current dependency versions is met: the lock is updated on a schedule, in a PR, not at run.)
- **Offline and air-gapped**: `coderipper tools bundle` / `tools install --from <dir>` for a prefetched, checksummed set.
- **Licence ledger**: `coderipper tools list` prints every installed tool with its version, licence and source, so credit and
  licence obligations are one command away.

## 9. Untrusted code and the hosted mode

Facts (MEASURED or by construction):
- The Rust module already **builds the analysed project** in a throwaway worktree (reachability, unused-parameters,
  unused-return-values run `cargo build`/`cargo check`, ref `75ca35a`). Cargo executes build scripts and procedural macros
  while doing so, so these checks **execute code from the repository** (by construction of cargo, not separately tested here).
- Many linters execute project-controlled files: ESLint loads JavaScript config and plugins; Python tools read and sometimes
  import project config; package managers run lifecycle scripts (ASSUMED per tool; each tool's entry in the triage records
  whether it executes project code).

Posture:
- Every rule carries `executes_project_code` (§4.1) and every module declares the same capability (§5.3).
- Two trust levels: `trusted` (the default for a local run on the user's own checkout; the code is theirs) and `untrusted`
  (hosted mode, or `--untrusted`). In `untrusted` mode a rule or module that executes project code **runs only if the host
  attests a sandbox** (the runner image sets a marker the host verifies); otherwise it is `error: untrusted_refused`, never
  silently skipped, never run unsandboxed.
- **Hosted mode** runs modules and tools only from **prebuilt, sandboxed images**; it installs nothing on demand; the sandbox
  has no network, a read-only code mount, a scratch directory, CPU/memory/process/wall-clock limits, no credentials in the
  environment, and is destroyed after the scan. Findings are data derived from untrusted text: strings are escaped when
  rendered, size-capped, and never interpreted as instructions (relevant for any later LLM tier).
- Designing the infrastructure (microVM, scheduler, queue, abuse limits) is out of scope here and stays the P7 design pass.

## 10. How what exists carries over

### 10.1 Scope, Network, run tier, unit (MEASURED, ref `75ca35a`, `src/check.rs`)

`Scope` (`Project`|`Portfolio`) and `Network` (`LocalOnly`|`NetworkRequired`) are rule attributes (§4.1). The run tier is
still derived from Network: `fast` = every local rule, `sweep` = also network rules. `Unit` (`Package`|`Repository`) is a rule
attribute: the host runs a `package` rule once per member of a language's project roots and a `repository` rule once. For
`Portfolio` scope the host passes sibling project roots; today no built-in check reads them (the audit found
`portfolio_root` unused), so this axis stays declared-but-unused until a cross-codebase rule needs it (§12, WSP rules).

### 10.2 Language detection and polyglot repositories

The host detects project roots by manifest (`Cargo.toml` -> rust; `package.json`/`tsconfig.json` -> typescript;
`pyproject.toml`/`setup.cfg`/`requirements*.txt` -> python; ASSUMED list, finalised per module's `detect`), runs each
language's module per root (a monorepo gets several), and always runs the neutral module once per repository. A language with
no installed module is reported as `NOT-COVERED` for every rule ("no module for typescript") in the coverage report, not
ignored: "I looked at a repository containing TypeScript and checked none of it" must be visible.

### 10.3 The `Finding` shape

Additive fields (the struct is `#[non_exhaustive]`, so adding fields is not a breaking change; MEASURED: `src/finding.rs`):
`rule: Option<String>` (catalog ID; equals `check_id` for catalog rules, absent for none), `language: Option<String>`,
`tool: Option<{name, version}>`. Existing fields, severity, confidence, `positive_control` and `Finding::validate` (absence
claims need a control) apply to every module's findings unchanged; the host validates what modules send exactly as it
validates what built-in checks return.

### 10.4 Allowlist and suppression

Unchanged in mechanism (MEASURED: `src/suppression.rs`; identity = `check` + `file` + `symbol`, a required `reason`, stale
entries reported as `Info` from check id `allowlist`). Two things to settle (Q9): (a) delegated findings often have no symbol;
the module must supply a stable `subject` (enclosing item path) where it can, and where it cannot the entry is
`check + file` plus an optional `line_range` rather than a fingerprint that churns; (b) stale-entry judgement needs "this rule
ran to completion", which is now exactly `coderipper-rule-result: ran`, a *stronger* signal than today's.

### 10.5 Exit codes and `--deny`

Unchanged: 0 clean, 1 a finding at or above `--deny`, 2 usage error, 3 the audit could not be completed. A covered rule that
`could-not-run` is a 3; a known gap is not.

### 10.6 Build cache and sessions

The Rust module keeps the build cache and the `--workspace` session (MEASURED: `src/build_cache`, `src/session.rs`). They are
Rust-module internals; other modules own their equivalents (a node_modules cache, a venv). The host's contract is only the
read-only checkout and the limits.

## 11. The built-in Rust module (P1, no behaviour change)

P1 introduces a `Module` trait in the library mirroring §5 (`describe`, `check(request) -> events`), an `ExternalModule`
adapter that speaks the protocol to a child process, and moves the five checks behind `RustModule`. The gate for P1 is
mechanical: **every existing golden and CLI test passes unedited**, the JSON output for a fixed fixture is byte-identical
before and after, and `coderipper conformance` passes for the five rules using fixtures extracted from the existing tests.
The library's public API (`run_checks`, `run_checks_with`, `Check`) keeps working; `Check` becomes the way a Rust user writes a
native rule inside the Rust module.

## 12. Deliberately not designed here

- **LLM-judgment (V4) rules.** The protocol can carry them (a module declares `evidence_tiers: [V4]`) but no V4 module is in
  P0-P7. The guardrails the knowledge base lists (evidence-bound output, calibration before gating, no sole-evidence security
  verdicts, injection hygiene) are recorded as requirements for whoever builds it.
- **Architecture intent** (`architecture.yaml`: components, allowed edges). Many dependency-direction rules are only
  checkable against a declared architecture. Whether CodeRipper owns that specification language is an open question (Q6).
- **History (V3) rules** need a VCS-mining component and a run-history store (the old design already notes none exists).
- **The hosted service infrastructure** (P7 is its design pass).

## 13. Risks

- **Tool churn.** Linters change rule sets and output formats; pinning plus conformance fixtures catch it at the lock-bump PR,
  not in a user's CI.
- **Coverage-number gaming.** A percentage invites padding with `not_applicable`; every `not_applicable` needs a reason that is
  shown, and the report lists them.
- **Maintenance cost grows with languages x rules.** The two-pass triage is designed to keep P0 cheap; the real cost is in P4/P5
  and is estimated there (`2026-10-07-multi-language-programme.md`).
- **Supply chain of the tools themselves.** The install policy narrows it (pins, hashes, no scripts where possible) but does not
  remove it; the licence ledger and the lock-bump review are the controls.
