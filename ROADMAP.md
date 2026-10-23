# CodeRipper roadmap

Where CodeRipper should go, in order, and who uses it. **Written 2026-10-14 against `main@1aebeb2` (0.6.14), at the PM's
request; refreshed 2026-10-20 against `main@ea58701` (0.8.3) at the PM's task. The PM owns priorities, so this is the lane's proposal and the PM edits it.** Detail lives in the plans under
`docs/superpowers/plans/`; this file orders them and says why. It records no promise of a date.

## Where it stands (read from `main@ea58701`, 0.8.3, 2026-10-20; the bullets below that quote a date are older measurements and keep their date)

- **Since 2026-10-14 (0.6.14 to 0.8.3):** items 1, 2, 3, 5, 6 and 7 below are done; two native rules were added (`lint-suppression`, extended profile
  only, 0.6.41; `conflict-marker`, every profile, 0.6.43); the crate runs its own sweep in CI (0.6.39); the portfolio-wide
  "group5" code audit was worked through (0.6.43-0.8.3: closed enums at the protocol boundary and in the catalog and config, rule ids generated
  from `rules/*.toml`, `Finding::absence` needs its control, a bad shipped `tools.lock` fails the build, fixture contract tested over the whole
  `conformance/` tree, GitHub error wording read in one place, `BuiltinCheck`). 0.7.0 and 0.8.0 are Breaking in the library's types; none
  changed a default. crates.io was last read on 2026-10-14 (below); it was not re-read for this refresh.
- **Bar re-measured 2026-10-20** (`coderipper 0.8.3`, depth-1 clones of the default branch of 33 repositories, `--no-config check <rule> --profile extended`,
  132 runs, all exit 0): **WSP-001 11 findings in 11 repositories (24 on 2026-10-14), DOC-009 0 (21), SUP-008 97 in 5 (326 in 18), SUP-001 7 in 7.**
  Each rule is clean on 22, 13, 26 and 24 repositories respectively and finds the rest; DOC-009 also fires on `conformance/DOC-009/defective`.
  Remaining: WSP-001 is "no CODEOWNERS" in `approvals-ciresnave`, `authframework-site`, `auto-discovery`, `candlelight`, `commy`, `commy-sdk-rust`,
  `commy-wire`, `dirty-ranges`, `fam`, `kiss-ref`, `role-system`; SUP-008 is `auto-discovery` 56, `role-system` 21, `commy` 9, `candlelight` 7, `fam` 4
  (68 `unpinned-uses`, 28 `excessive-permissions`, 1 `unpinned-images`); SUP-001 is cargo without `--locked` in `agentlife`, `commy`, `commy-sdk-rust`,
  `lightbulb`, `role-system`, `synapse` and an application crate without `Cargo.lock` in `PersistAnt`. DOC-009 is 0 because 0.6.25 added a ten-page
  threshold (20 repositories are below it) and the rest now keep a glossary. The population grew from 27 to 33 (`commy`, `commy-sdk-rust`, `commy-wire`,
  `dirty-ranges`, `role-system`, `authframework-site` are new); the two clones that track no workflow are "not run" for SUP-001 and SUP-008, not clean.

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
  which is a breaking release; as of 2026-10-20 it is **closer but not ready**: WSP-001 is on 11 repositories, DOC-009 on none and SUP-008 on 5 (97 findings),
  against 24 / 21 / 18 repositories on 2026-10-14. What is left is mechanical fixes the owning lanes have not made (and six repositories new to the population).
- **Measured on the real fuel workspace (Task 7, 2026-10-15, `fuel@8cdb33dd` 0.19.5, CodeRipper 0.6.32, `--workspace`).** **45 members analysed = the plan's 43 publishable members + 2 non-publishable fixtures** (`fuel-model-fixture`, `fuel-model-umbrella-fixture`; the plan was written against `fuel@d1bfe127`, which listed 43). **Findings:** reachability 469 (all medium; 25 members; the cold and warm runs are byte-identical), unused-return-values 36 (28 low + 8 medium; 6 members), unused-parameters 0; every run exits 3. **Wall time per whole-workspace run:** reachability 1742 s cold and 915 s warm, unused-return-values 1992 s and unused-parameters 1334 s (both run after reachability, so against a warm cache); the cache is 73.5 GB. **The lever is an incremental cache, not parallelism** (PM's ruling). **Not collected:** per-member times (the runs were whole-workspace; the plan asked for them), per-member parity against `--workspace` (the PM ruled it unnecessary), and a run beside another lane's build. On Windows, `fuel-cuda-backend` and `fuel-metal-kernels` cannot build, so every run exits 3 for that environmental reason. **unused-parameters reported 0 findings and no positive control: the control's target was not built** (`fuel-capture-fixtures` has `required-features = ["capture"]` and is never compiled in a default build; no crate-wide `allow(unused)` hides it). **Defects found by this run, all fixed (0.6.33-0.6.35, proposed):** (1) when a control is absent because its target was not built, the output now says so ("target not built: required-features=[capture]"), fixed for unused-parameters in PR #83; (2) `--features`, `--all-features` and `--no-default-features` now reach the build step (PR #84); (3) CodeRipper reads the committed tree (a throwaway worktree), so uncommitted fixes show no change; `--help` and the run now say so (PR #85; 0.6.36 corrected it: the throwaway worktree is used by reachability, unused-return-values and unused-parameters only, `version-consistency` and API-006 read the working tree, and the note now counts untracked files too).

## Order of work

The order is: finish what users hit today, then what blocks the default, then reach, then the hosted service. Each item names what
would make it stop.

| # | Item | Why now | Size | Blocked by |
|---|---|---|---|---|
| 1 | **DONE in 0.6.16 (proposed): proc-macro crates under `cfg_attr`** (the remainder of issue 54). 0.6.7 kept `#[proc_macro*]` entry points `pub`, but an attribute written inside `cfg_attr(..., proc_macro_derive(..))` was not recognised and that crate still exited 3. | A crate that exits 3 cannot go clean under "all rules"; issue 54 found it in 1 of the 23 repositories measured on 2026-10-12. | small | nothing |
| 2 | **Backport of item 1 and of the 0.6.7 fix.** Published 0.6.9, 0.6.11 and 0.6.14 already carry 0.6.7; only 0.6.3 lacks it. There are no release branches and the line is pre-1.0 (users take the newest 0.6.x), so no backport branch is cut: the answer is "publish the item 1 fix in the next 0.6.x". | The portfolio rule is to backport when it is not a lot of work. | none | the PM's `cargo publish` |
| 3 | **DONE 2026-10-14, re-measured 2026-10-20 (see "Where it stands"): triage WSP-001, DOC-009 and SUP-008 across the portfolio** (report-only for this lane: the per-repository fixes belong to the lanes that own those repositories). Produce one list per rule: true defect, rule too strict, waiver with a reason. **Lists produced 2026-10-14 (27 repositories at their default branch): `docs/portfolio-triage/2026-10-14-wsp001-doc009-sup008.md`; the per-repository fixes are the other lanes' to make.** | It gates the breaking release (item 4). | medium, mostly other lanes | the other lanes |
| 4 | **DONE in 0.9.0 (proposed): all rules by default (breaking).** The default profile is `extended`; `--profile classic` restores the old set. | CireSnave's ruling (board 144). The earlier "blocked by the other lanes' fixes" was this lane's sequencing, not a dependency: a consumer pins a version and meets the new findings only when it upgrades, and CireSnave 2026-10-23: "There is zero reason to hold back CodeRipper based on the fact that projects that are *consumers of CodeRipper* couldn't pass the tests of that new release." | small | nothing |
| 5 | **DONE in 0.6.21 (published): the configuration layers** (system file, user file, restricted repository file, environment, flags; `--config`, `--no-config`; #69). | The hosted service and a site-wide policy both need a stronger-than-repository layer, and today a repository can only waive what it chooses. | medium | nothing |
| 6 | **Known reachability limits.** (a) an intended public API of a library with no in-repo consumer is still reported (**DONE in 0.6.23, proposed: `[reachability] public_api` declares the surface with a reason per entry, printed as declared public; no source signal is guessed**); (b) `[patch]` and `[replace]` dependencies are not seen by the dependents rescue (**DONE in 0.6.22, proposed: a patch whose `path` is a member's directory counts as a path dependency; git, out-of-workspace and `.cargo/config.toml` patches are still not seen**). | These are the two shapes the sweep still reads as false positives; each waived finding costs a reviewer. | medium | nothing |
| 7 | **DONE (0.6.17-0.6.19, proposed): small defects found and not filed:** the gitleaks wrapper "did not read the history" on a Windows path longer than 260 characters (fixed in 0.6.17, proposed: `core.longpaths=true` through git's environment configuration); `--redact` on `conformance` (fixed in 0.6.18, proposed); and run `cargo coderipper --deny medium` in this repository's own CI (done in 0.6.19, proposed). | Cheap, and each is a wrong or missing message. | small | nothing |
| 8 | **TypeScript module (multi-language P4), then Python (P5), then the install policy (P6).** The estimates are in the programme plan: 9-22 and 8-21 lane-days, both unmeasured. | The portfolio's websites and tools are TypeScript; Rust-only coverage leaves them as gaps. | large | items 3-5 (so a module is not built on a default that is about to change) |
| 9 | **Hosted service.** Result document and intake interface first (design), then the build. | CireSnave's stated plan for ThinkersJournal.com; the design is complete enough to price. | large | **CireSnave's separate go** |

**2026-10-20:** items 1, 2 (by the next 0.6.x policy, no branch cut), 3, 5, 6 and 7 are done. Item 4 was done on 2026-10-23 (0.9.0, proposed); item 8 waited on item 4 and
on the triage of the default ruleset; item 9 waits on CireSnave's go and is not worked. Nothing in this lane is unblocked beyond re-measuring.

## Next rule families (proposal of 2026-10-23, from the PM's tasks carrying CireSnave's words)

**Status: proposal, nothing built.** Ranked by incidents recorded in this portfolio. The incidents are **as the PM relayed them on 2026-10-23; this lane has not re-read the sources**, so
each rule's first step is a fixture that reproduces the incident's shape (the positive control), and the rule is not claimed until that fixture is found. Where the PM supplied no
incident, the row says so and the first control is synthetic. Order of work after this PR merges: the `perf` design (done: `docs/superpowers/plans/2026-10-23-performance-rules.md`, 0.9.3), the
`--summary` mode, the precision sample (30 per candidate perf rule), then a design document each for families 1 to 4, in that order.

### Default-on policy

CireSnave, 2026-10-23, verbatim: *"I think everything except possibly the AI-assisted tier should be on by default in CodeRipper. After all, projects can opt out of any rule."*

1. **Every family ships in the default profile (`extended`) except the LLM-assisted tier**, which is off by default.
2. **Opt-out is per rule, in configuration, with a required reason.** The sweep reports the **opt-out count per project**, so waivers are visible (this ties to `lint-suppression`, which already counts suppressions that carry a reason).
3. **Default-on never means blocking.** Whether a rule is a required check stays a per-project decision (`blocking = [..]`, or the portfolio floors).
4. **Precision gate.** A rule joins the default only after its measured precision on the portfolio clears a bar. **Proposed bar: at least 80% true positives on at least 30 sampled findings** across at least three repositories, with the seed and the sample recorded beside the rule. Below it the rule ships **opt-in**. A rule that is exact on its syntax and finds nothing on the portfolio (nothing to sample) may ship default-on if a seeded fixture proves it both ways. The first sample (`perf`, 12 findings) found 4 true, and 0 of 7 in the allocation-in-loop family, so that family is opt-in.
5. **Perf rules are default-on only where scoped** (inside a loop, or in a path declared hot with `[perf] hot_paths`); the looser ones are opt-in until measured. This **replaces** the earlier instruction that `perf` be an opt-in profile outside `portfolio`.
6. **Release numbering.** A new default-on rule is a **Breaking** change for a consumer who upgrades (it can add a finding to a project that was clean). Before 1.0 that is the second number. Bundle families into few releases, with the exact rules together, and the opt-in rules in patch releases.
7. **Network checks stay in the sweep tier**, never in a pull-request job.
8. **The `portfolio` profile** (what every project's advisory job runs) picks up a new default-on rule, and **each change is announced first by a [NOTICE]**.

### The families

| # | family | three candidate rules (all static unless marked) | the incident that is the positive control | notes |
|---|---|---|---|---|
| 0 | **Performance** | see the design document: regex in a loop, collect-then-iterate, `repr(C)` padding, claim without a benchmark (exact); allocation in a loop, linear search, N+1, unbuffered I/O (opt-in until the gate) | measured on fuel, baracuda, Unpopped (design, section 6) | design merged in 0.9.3; the build is not scheduled |
| 1 | **Security and cryptography** | (a) `unwrap`/`expect`/`panic!`/indexing in code that handles untrusted input (scoped by declared entry points, not the whole crate); (b) AEAD encryption with an empty or missing AAD, a non-constant-time `==` on a secret type, a secret type with no zeroize on drop; (c) a hard-coded key, a weak algorithm (MD5, SHA-1, DES, RC4, ECB), TLS verification switched off, a log macro whose argument is a secret field | auth-framework: the JARM AAD and zeroize defect. synapse: the CodeQL cleartext-logging alert | injection, zip-slip and path traversal, and unbounded allocation from an untrusted length are the next tier; they need taint, so they are delegated to CodeQL/Semgrep when installed rather than guessed from syntax |
| 2 | **Provenance and licence of committed data** | (a) a data, binary or model file with no provenance record beside it (source, licence, sha256; `data/patent-tiers.PIN` is the shape); (b) a record whose licence contradicts the upstream's licence (**network**, sweep tier); (c) third-party code with no licence and notice kept | fuel: a GPL-3.0 Qwen-AWQ slice recorded as Apache-2.0 | ties to CireSnave's *"I am not a plageurist"* (the portfolio's provenance rule); (b) cannot be static, and says so |
| 3 | **Doc-versus-code truth** | (a) a doc comment that promises behaviour the code does not have, in its mechanical form: it names a function or check that has zero callers; (b) a command in a README that does not run (fenced blocks run from a clean checkout; the same tier-1 rule as family 13); (c) a ROADMAP or status row marked done whose named file, symbol or test does not exist, and a CHANGELOG that disagrees with the manifest version | fuel: the AWQ doc says it "enforces `can_implement`", which has 0 callers | the engine for (a) is the existing `reachability` check; judging whether prose *means* the behaviour is not static and stays out |
| 4 | **Silently ignored input** | (a) a config field that is parsed and never read; (b) a CLI flag that is parsed and never read; (c) an environment variable the documentation names and the code never reads | none supplied; the first fixture is synthetic and the portfolio sweep supplies the real ones | the engine is a field-level read count, the same family as `reachability` |
| 5 | **Test quality** | (a) a test that prints and asserts nothing; (b) a wall-clock assertion (`Instant`, `sleep`, then an assertion on the elapsed time); (c) `#[ignore]` with no reason, and a test that reads a source path | lightbulb: a flaky wall-clock test | "new behaviour with no negative control" is a review judgement, not a static rule, unless a branch-level diff tool is added later |
| 6 | **API and semver** | (a) a public enum without `#[non_exhaustive]`; (b) a public change with no version change (`cargo-semver-checks` is the model, and CI already has a "public API matched by the version" job); (c) a public function with no `# Panics` or `# Safety` section | fuel: `WeightStorage` | overlaps family 13 (doc presence); one rule, two families, counted once |
| 7 | **Correctness and concurrency** | (a) a lossy `as` cast; (b) a lock guard held across `.await`, and a blocking call in an `async fn`; (c) a fixed-window rate limiter, and a network client built with no timeout | lightbulb: fixed-window rate limits | (b) overlaps Clippy (`await_holding_lock`); require the lint and add only what it lacks |
| 8 | **Schema versus query** | (a) a SQL literal selecting a column that no migration creates; (b) a migration runner with no lock, so two instances can migrate at once; (c) a migration edited after it was released (checksum against the base) | auth-framework #147 | needs a migrations parser first; (c) reads the merge-base, like the `lint-suppression` ratchet |
| 9 | **Supply-chain policy** | (a) a git or path dependency (CireSnave's sources rule); (b) a dependency behind the newest release (his latest-versions rule); (c) an advisory, a yanked version, an unmaintained crate | his two standing rules, quoted in `C:\Projects\CLAUDE.md` 9 | **mostly built**: `SUP-004` (a), `SUP-002` and `SEC-006` (c). Not built: the scheduled staleness check (b), `docs/portfolio-ci-coverage.md` build order 6, run on a schedule, not per pull request |
| 10 | **Portability** | (a) target-specific code with no fallback (a `cfg(unix)` item used unconditionally); (b) a hard-coded path separator in a path join; (c) line endings disagreeing with `.gitattributes` | the path-rewriting and CRLF costs recorded in `C:\Projects\CLAUDE.md` 2 | the Windows exit-3 of fuel (`fuel-cuda-backend`, `fuel-metal-kernels` cannot build there, ROADMAP 2026-10-15) is an environment limit, not this family |
| 11 | **Repository hygiene** | (a) no `SECURITY.md`; (b) no CODEOWNERS (`WSP-001`, built); (c) a `TODO` or `FIXME` with no issue link | `WSP-001`: 11 repositories on 2026-10-20 | (b) is the control for the whole family: it exists, and has a conformance pair |
| 12 | **Non-Rust stacks** | web accessibility (an image with no `alt`, a form control with no label, a missing `lang`, a skipped heading level), security headers (CSP, HSTS in a server config), and the language-neutral families above through the TypeScript module (item 8) | none supplied | static accessibility catches a minority of real problems; the doc says that |
| 13 | **Documentation usability and accuracy** | three tiers, below | none supplied for the tier-2 and tier-3 rules; tier 1 reuses family 3's incident | tiers 1 and 2 default-on after the gate; tier 3 off by default |
| 14 | **End-user usability of apps** | accessibility and security headers as in family 12; first-run and error-message checks are tier 3 | none supplied | **onboarding and task flows need humans**; no static rule can judge them, and this row says so rather than imply otherwise |

### Family 13 in detail: documentation, in four tiers

CireSnave, 2026-10-23: *"What about having CodeRipper check things like usability for users, usability for developers, depth, completeness, and accuracy of documentation, and the like?"* and, on doc comments: *"accuracy of documentation when it comes to documenting code can somewhat be checked statically ... do we have doc comments for each function? Do those doc comments explain each parameter and each chunk of output? While we can't statically judge the quality ... we should be able to statically check to see if they're there."*

**Tier 1, deterministic.** Doctests and examples compile. Fenced README commands are extracted and **run** from a clean checkout. Internal links, files and anchors exist (external links stay in the sweep tier, `DOC-010`). CLI flags, config keys, environment variables, cargo features and error variants are documented **and** nothing is documented that the code lacks (both directions; overlaps families 3 and 4). Version numbers, install commands and documented defaults match `Cargo.toml`, the registry and the code. A "not yet implemented" note on something the code implements. Every breaking change has a CHANGELOG line and a migration note. **Doc-comment presence and staleness (Rust first):**

| id | rule | precision |
|---|---|---|
| (a) | a doc comment on every `pub fn` (and on a private function above a size or complexity threshold, configurable) | exact on presence; reuse `rustc` `missing_docs` and require it enabled |
| (b) | every parameter named, or covered by an `# Arguments` section | exact on names; a doc that covers parameters in prose under another heading is missed |
| (c) | the return value described; `# Errors` on a `Result` function; `# Panics` when the body has `panic!`, `unwrap`, `expect` or indexing; `# Safety` on an `unsafe fn` | reuse Clippy `missing_errors_doc`, `missing_panics_doc`, `missing_safety_doc`; add the per-return check; indexing is a weak signal for `# Panics` (often bounded) |
| (d) | every `pub` struct field, enum variant and `const` documented | exact |
| (e) | **staleness**: the doc names a parameter that is not in the signature; describes a return type the function no longer has; has `# Errors` on a function that returns no `Result` | exact for the parameter name; the return-type check compares words with the type name and misses paraphrase |
| (f) | tautological doc: it only restates the identifier (`/// Gets the name.` on `fn get_name`), by token overlap with a threshold | **advisory**: it cannot tell a short, complete doc from a restatement when the function is trivial; say so in the finding |

What Clippy and `rustc` lack, and the new rules add: per-parameter and per-return presence, staleness, the **portfolio count**, and a finding for a project that `#[allow]`s those lints (the `lint-suppression` rule). Other languages later: TSDoc `@param` and `@returns`, Python docstring sections. Each rule needs a positive control (a function with an undocumented parameter) and its twin.

**Tier 1.5, prose lint (static, advisory).** CireSnave: *"prose can be checked statically for readability. We were doing that before AI existed ... Sentences that are too long, have potentially ambiguous meanings, or which contribute no added definition to the conversation can often be caught with a static checker."* Candidate rules, each with a precision note and a control:

| rule | precision note | control |
|---|---|---|
| sentence length over a threshold | code spans and identifiers are stripped first; technical text has legitimately long sentences, so the threshold is per document type (spec, README, doc comment, CHANGELOG) | a 60-word sentence is found; the same text split in two is not |
| readability score per document (Flesch, Gunning Fog) | numbers move with text length; advisory, reported as a number, never a finding by itself | a fixture pair on either side of a threshold |
| a sentence that opens with *it*, *this* or *that* with no noun antecedent in the previous sentence | heuristic; a heading-led list defeats it | "This is slow." after a heading: found |
| vague quantifiers (*some*, *various*, *several*, *etc.*) and stacked hedges | exact on the word list; some uses are fine, so Low | "various improvements" |
| *should*, *must*, *may* mixed outside RFC 2119 usage | **respects a document's own requirement words** (KISS-style specs define them); fires only where the document defines none | a spec that defines MUST, then writes "should" |
| filler (*basically*, *in order to*, *it should be noted that*) | exact on the phrase list | the phrases |
| a sentence that only restates its heading or the identifier | token overlap; the same limit as (f) above | "## Installation / Installation instructions." |
| a duplicated sentence | exact | repeated line |
| an acronym or term used and never defined | checked against the project's `GLOSSARY.md` (`DOC-009` already reads it) | an acronym absent from the glossary |
| passive voice and long noun chains; insensitive wording | weak signals; the word lists are curated and short | seeded phrases |

**Vale and proselint were considered.** Both are existing prose linters (Vale: a Go program with YAML style rules; proselint: Python); this is from memory and **was not re-verified today**. Recommendation: a **small native Rust set** of the high-precision rules above (length, filler, duplicates, vague quantifiers, requirement words, undefined acronyms), because the portfolio rule is not to depend on an installed tool when the rule can be built in, a native rule is deterministic and has no install step, and the project already owns a glossary reader and a Markdown reader. **Optional shell-out to Vale when it is installed**, through the delegation framework that already runs gitleaks, osv-scanner, lychee, zizmor and buf (a tool that is missing is a reported gap, never a failure), so a project with its own Vale styles keeps them.

**Tier 2, metrics (advisory numbers).** Public-API doc coverage as a percentage; presence of `# Errors`, `# Panics` and `# Safety` where the body warrants them; presence of a tutorial, a how-to, a reference and an explanation (Diataxis); time to first success, measured by running the README quick start in CI.

**Tier 3, judgement (depth, clarity, accuracy of prose, helpful error messages). LLM-assisted, strictly advisory, off by default.** It must quote the objectionable sentence, runs only on changed documents, and routes to cheaper non-Claude models (CireSnave's cost rule, `C:\Projects\CLAUDE.md` 9). **It does not ship until it has been validated on documents seeded with known mistakes**, with the catch rate recorded. With the prose-lint tier above, this tier shrinks to **semantic correctness and depth** only.

### Where each family goes next

Families 1 to 4 each get a design document in the order above (the first is security and cryptography). Families 5 to 14 are scheduled only after those, and after the precision gate has been run on the first two families' rules.


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
