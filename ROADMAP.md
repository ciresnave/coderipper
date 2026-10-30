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
- **Rules that run.** Six native Rust checks (`reachability`, `unused-return-values`, `unused-parameters`, `version-consistency`,
  `ci-protection-presence`, `dependency-freshness` since 0.9.28); five language-neutral native rules (SUP-001, SUP-011, DOC-005, DOC-009, WSP-001); six rules run by
  a pinned third-party tool through the delegation framework (SEC-002 gitleaks, SUP-002 and SEC-006 osv-scanner, DOC-010 lychee,
  SUP-008 zizmor, API-006 buf). Every rule except `ci-protection-presence` has a seeded defect and a clean twin under `conformance/` (it needs a forge: its pair is `tests/fixtures/forge/` run by `tests/ci_protection_fixture.rs` over a stub forge, so the built-in module does not claim it as `proof`).
- **Not built.** The TypeScript and Python modules (multi-language P4, P5), the remaining unscheduled native rules (the "96" of 2026-10-20 could not be reproduced on 2026-10-24: see M13 in the audit below),
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
4. **Precision gate.** A rule joins the default unless it produces many false positives (CireSnave, 2026-10-25: *"Unless a rule produces a lot of false positives, default them on. That holds true for all of our rules."*). **The written bar (2026-10-25, replacing the proposed 80%): at least 70% of a 30-finding sample a reviewer would act on**, across at least three repositories, with the seed, the sample, the rubric and **who judged** recorded beside the rule; see `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §1. Below it the rule ships **opt-in or scoped, never deleted and never "declined"**: it goes on this roadmap with a position. A rule that is exact on its syntax and finds nothing on the portfolio (nothing to sample) may ship default-on if a seeded fixture proves it both ways. The first sample (`perf`, 12 findings) found 4 true, and 0 of 7 in the allocation-in-loop family, so that family is opt-in.
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
| 9 | **Supply-chain policy** | (a) a git or path dependency (CireSnave's sources rule); (b) a dependency behind the newest release (his latest-versions rule); (c) an advisory, a yanked version, an unmaintained crate | his two standing rules, quoted in `C:\Projects\CLAUDE.md` 9 | **mostly built**: `SUP-004` (a), `SUP-002` and `SEC-006` (c). The staleness check (b) is built as `dependency-freshness` (0.9.28, Cargo only). Not built: the scheduled workflow that runs it, `docs/portfolio-ci-coverage.md` build order 6, on a schedule, not per pull request (audit row C6) |
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

**Vale and proselint were considered. Every fact about them in this paragraph is UNVERIFIED (from memory, not read from their documentation today), and none may be relied on until its check below has been run.**

| claim (unverified) | the check that would verify it, before a rule is built on it |
|---|---|
| Vale is a single Go binary that reads YAML style rules | install the release, run `vale --version` and read the style-file format in its documentation |
| Vale is MIT-licensed, proselint is BSD- or MIT-style licensed | read each `LICENSE` file in the tagged release; the portfolio's provenance rule applies to anything we bundle or download |
| Vale can lint Markdown, reStructuredText and AsciiDoc and can skip code spans and code blocks | run it on a README that holds fenced code and inline code, with a rule that would fire inside them |
| proselint is a Python tool with a fixed set of checks | install it in a clean virtual environment, list its checks, and run it on the same README |
| neither reads `GLOSSARY.md`, so the undefined-acronym rule is ours either way | read each tool's documentation for a vocabulary or glossary feature, and run an acronym fixture |
| a Vale run is fast and deterministic enough for a pull-request job | time it on this repository's `README.md`, `ROADMAP.md` and `CHANGELOG.md` on a hosted runner |

Recommendation (a design choice, which does not depend on those facts): a **small native Rust set** of the high-precision rules above (length, filler, duplicates, vague quantifiers, requirement words, undefined acronyms), because the portfolio rule is not to depend on an installed tool when the rule can be built in, a native rule is deterministic and has no install step, and the project already owns a glossary reader and a Markdown reader. **Optional shell-out to Vale when it is installed**, through the delegation framework that already runs gitleaks, osv-scanner, lychee, zizmor and buf (a tool that is missing is a reported gap, never a failure), so a project with its own Vale styles keeps them.

**Tier 2, metrics (advisory numbers).** Public-API doc coverage as a percentage; presence of `# Errors`, `# Panics` and `# Safety` where the body warrants them; presence of a tutorial, a how-to, a reference and an explanation (Diataxis); time to first success, measured by running the README quick start in CI.

**Tier 3, judgement (depth, clarity, accuracy of prose, helpful error messages). LLM-assisted, strictly advisory, off by default.** It must quote the objectionable sentence, runs only on changed documents, and routes to cheaper non-Claude models (CireSnave's cost rule, `C:\Projects\CLAUDE.md` 9). **It does not ship until it has been validated on documents seeded with known mistakes**, with the catch rate recorded. With the prose-lint tier above, this tier shrinks to **semantic correctness and depth** only.

### Where each family goes next

Families 1 to 4 each get a design document in the order above (the first is security and cryptography). Families 5 to 14 are scheduled only after those, and after the precision gate has been run on the first two families' rules.


## Spec-to-roadmap audit (2026-10-24, against `main@102e19f`, 0.9.28)

At the PM's task (CireSnave's idea): every piece of work that a spec, plan or README describes and this file did not name. **This lane adds the rows and starts none of them.**
**Read in full:** 21 documents (the audit-host and multi-language specs; the programme, three-faces, configuration-layers, redaction, hosted-service, runner-isolation, performance, performance-precision, provenance, security-crypto, reachability-project-scope and unused-parameters plans; the terms-of-service draft; the triage README and its two vocabulary files; the two portfolio-triage reports; `docs/portfolio-ci-coverage.md`). **Read in part, plus a grep of the whole file for deferred, not-yet and out-of-scope wording:** 7 plans (unused-return-values, allowlist-stale-entries, version-consistency, ci-protection-presence, workspace-members, reachability-lib-plus-bin, workspace-flag-and-build-cache). **By keyword or table-row extraction only:** `README.md`, the 26 `docs/rules/*.md` pages, `CHANGELOG.md`. **Not read:** the triage `.tsv` data and its Python tools, the conformance fixtures' READMEs. The statuses below were checked by grep against `src/`, `rules/`, `tests/` and `CHANGELOG.md` at the ref above; a status was not re-run beyond that. Source column: `audit` = `2026-09-30-audit-host-design.md`, `mld` = `2026-10-07-multi-language-design.md`, `prog` = `2026-10-07-multi-language-programme.md`, `3f` = `2026-10-04-three-faces.md`, `cfg` = `2026-10-13-configuration-layers.md`, `hs` = `2026-10-12-hosted-service.md`, `ri` = `2026-10-13-runner-isolation.md`, `cov` = `docs/portfolio-ci-coverage.md`.

**Call** = needs a decision from CireSnave (through the PM); each is in the [ASK] that came with this PR.

### The Rust checks and the host

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| E1 | Cached per-project symbol index, so `Portfolio`-scope `fast` checks read a cache, not N trees | audit 60-64 | open | |
| E2 | Reachability portfolio scope (sub-pass B): rustdoc-JSON public surface plus cross-repository reference search | audit 120-135; reachability-project-scope plan 62 | open (no `Portfolio`-scope check exists) | |
| E3 | Severity-ranked brief mode (top N plus omitted count) | audit 90-95 | open (`--summary`, 0.9.8, counts per rule only) | |
| E4 | `until_fixed` links and "no match in N runs" waivers | audit 164-179; allowlist plan 19-23 | held: needs a run-history store that does not exist | |
| E5 | A qualified `subject`, so same-named methods in one file do not share an allowlist entry (`reachability`, `unused-return-values`; `unused-parameters` has it) | audit 181; urv plan 111 | open | |
| E6 | A finding with no `subject` cannot be allowlisted | allowlist plan 88 | open, minor | |
| E7 | One check declaring two depths (shallow `fast`, deep `sweep`) under one id | audit 276-283 | open (no depth field in `src/check.rs`) | |
| C1 | Reachability limits: `pub` items in a bin when a lib exists, custom `[lib] path`, a lib item used only from a `#[cfg(test)]` module, macro-made identifiers, a test-only caller not rescuing, the line-based `pub` downgrade | reachability plans; `reachability/mod.rs` 23-39 | open | |
| C2 | `unused-return-values` limits: functions inside `macro_rules!`, only `src/` rewritten, HEAD only | urv plan 108-111 | open | |
| C3 | `unused-parameters` limits: the `_`-prefix blind spot, trait declarations, default bodies and closures, targets with a custom `path`; shape (b) for languages with no built-in lint | unused-parameters plan 22-24, 102-104; audit 255-274 | open | |
| C4 | `version-consistency` for `package.json` and `pyproject.toml`, and an npm SDK paired with a crate (6.6a) | audit 206; version-consistency plan 82; cov 15 | open | |
| C5 | `ci-protection-presence`: (ruleset `non_fast_forward`/`deletion` done in 0.9.41, ruleset-only branches); required reviews and signed commits; required-check names matched against the workflows; non-default branches; non-GitHub forges; rate limits; a taken-over fork told from a plain fork | audit 229; ci-protection plan 87; CHANGELOG 0.9.27 | partly built (ruleset force-push/deletion); the rest open | |
| C6 | `dependency-freshness`: **the check is done (0.9.28, #151, Cargo only)**. The scheduled workflow for this repository is built (0.9.36, `scheduled-audits.yml`: it runs it with SUP-002, SEC-006 and DOC-010 link rot); open: the portfolio-wide schedule in `portfolio.yml`, which needs a published release with the check, and npm, pip, Maven and Gradle lockfiles | audit 209-216; cov 37, 60 | check done; schedule and other ecosystems open | |
| C7 | **Ruled 2026-10-25 (CireSnave): "CodeRipper was designed to work across multiple projects at once right from the start... Lets keep cross-repo abilities. If you feel building a cache first is important, do so."** The `--scope` list is no longer held: it is to be built, and the incremental cache first. **Slice 1, the result cache, is built (0.10.2), with design rules from the patent reading (whole tree + toolchain + options only; no per-file/snippet/function keys; no reuse for a partly changed tree; no shared store; `docs/PATENT-SWEEP.md` hit 15), whole-tree residual on the decisions board before it is published:** a second run over an unchanged committed tree reads the stored result (this repository's `reachability` 48 s then 1 s); not measured on the 45-member `fuel`; keyed on the whole tree, so one changed member re-analyses all (a member-level key is open). **Slice 2, `--scope <file>`, is built (0.10.3)** (plan: `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §7). Workspace runs: members whose targets lie outside the package directory; a relative out-of-repository path dependency in the throwaway checkout; per-member times; **the incremental cache that the 2026-10-15 ruling named as "the lever" (no row built it)**; parallel members; a `--scope` list of repositories | workspace plans 122-124, 157, 491, 521-526; ROADMAP "Where it stands" | both slices built (result cache 0.10.2, `--scope` 0.10.3); member-level key open | no |
| C8 | `patent-index-match` did not read `yarn.lock`, `pnpm-lock.yaml`, `go.sum`, `gradle.lockfile`: the first, second and fourth are read (0.9.42); `go.sum` is not, because the index lists no Go module | README; CHANGELOG 0.9.24 | done (go.sum: nothing to match) | |
| C9 | Residue the rules say they do not judge: SUP-004 (a run-time `..` join, `version` on in-repository path dependencies, a registry allowlist), `licence` (other owners' names, manifests below the root, whether a copied header is the origin's), `ci-security`, `version-and-changelog`, `lint-suppression` (`#[expect]`, macro bodies), a nested `[buf] baseline` | README 250-262, 497; cov 32-35 | open | |
| C10 | Coverage-doc rules never built: `deny.toml` present and `cargo deny check` run (a **blocking** row), GOV-002 waivers counted (partial: `--summary`), README has the five parts, DOC-001 CONTRIBUTING and `missing_docs`, TST-001, a per-repository fork switch for the `portfolio` profile, the `version-deferred` label | cov 36, 39, 43-46, 67, 70 | open | |

### Modules, languages, tools

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| M1 | External-module discovery (a `modules/` directory in the tool cache, an explicit configured path) and installing a module with lock entry, checksum and consent | mld 187-212 | open (CHANGELOG 0.3.1: "nothing selects an external module yet") | |
| M2 | `[languages] ts = { enabled, required = true }` (a missing module is exit 3) and `[rules.ID]` overrides (`level`, `threshold`, `reason`) | mld 162-169; prog 42 | open (`--skip-rule ID=WHY` is a partial substitute) | |
| M3 | `--min-coverage <pct>` as an opt-in floor on COVERAGE (covered / rules in the catalog, per language; NOT on the share of rules passed, which `--deny` already covers). **Ruled 2026-10-25 (CireSnave): report the percentage of rules passed on every run, keep the floor optional.** The percentage is built (0.9.44: summary line, JSON summary keys, coverage `run.passed_percent`); the coverage floor is built (0.15.2 proposed; PM confirmed the coverage meaning 2026-10-30) | mld 372; prog Q7 | built | no |
| M4 | `--untrusted` and `untrusted_refused` for rules that execute project code, with sandbox attestation | mld 440-446 | open; belongs with item 9 | |
| M5 | Process-tree kill by Windows job object, or cgroup/subreaper on Unix (today `taskkill /T` / a process group) | mld 313-315; `module/external.rs` 39 | open | |
| M6 | The rest of the install policy (P6): consent prompt, `tools bundle`, `tools install --from`, update-by-lock-bump tooling, `cargo`/`npm`/`pip` installers | mld 393-423; prog 85-92 | open (item 8 names P6 only in general) | |
| M7 | SEC-009 (checkov), the seventh P3 delegation. **Ruled 2026-10-25 (CireSnave): support any good tool whatever language it is written in; the 2026-10-09 "no Python prerequisite" was a PM ruling, not his.** Built in 0.9.51 from checkov's standalone release binary (no Python). **Not built, positioned next:** an isolated, version- and hash-pinned environment for a tool that ships only as a Python package (detect an installed one, offer `coderipper tools` install, no silent install, a missing runtime stays a reported gap); no tool needs it today | prog 52-54; README "Rule run by a tool: SEC-009" | SEC-009 done; the Python-environment installer open | no |
| M8 | **Ruled 2026-10-25: the bundle is accepted except Q12, which is case by case ("many checks are too valuable to delay, so if others haven't built them, we need to sooner rather than later").** The valuable-and-unbuilt list is in `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §2. The programme's unanswered questions: Q3 (the language after TypeScript and Python), Q4 (the module language), Q6 (an architecture-intent spec), Q8 (restrictive-licence tools), Q10 (the LLM tier), Q12 (which rules are native), Q13 (does "build natively" forbid delegating). Q7 and Q14 are answered (0.5.0, 0.9.0) **Built 0.11.0: API-001 via cargo-semver-checks, API-007 via squawk and CON-006 (native lock-order cycles, Low confidence)** (the first three of the "build now" list). **Positions for the rest:** AIH-002 (referenced APIs and new packages really exist) is not built: cargo and the other package managers already fail on a package or API that does not exist, so it adds nothing until a registry client exists (and a squatted name that does exist is SUP-012's case); SUP-012 (squatting) needs a registry client (`cli` feature only, the library has no HTTP client) and download and age data, next; ARC-002 (module cycles) waits for a module-graph tool to delegate to. | prog 151-196 | Q12 list written; building follows | no |
| M9 | Per-language triage of the 125 rules outside the first batch; 121 native cells "to write" (13 neutral, 28 Rust, 40 TypeScript, 40 Python) | prog 22-24; triage README 61-82 | open | |
| M10 | First-batch items never built: WSP-008 (cross-codebase), a `cargo vet`-style SUP-013 | triage README 132-158 | open | |
| M11 | Licence findings that block bundling or hosting: Semgrep Registry rules (SEC-001, TypeScript SEC-004) and `eslint-plugin-sonarjs` are NOT-COVERED cells | triage README 95-100 | open | |
| M12 | **Ruled 2026-10-25: "Ring is fine for now but keep our eyes out for a pure-Rust alternative once one matures."** Watch line: check the rustls crypto-provider landscape at each dependency refresh. Replace `ring` (C and assembly, via `rustls` in `ureq`) with a pure-Rust provider | README 421 | watching | no |
| M13 | The 203 catalog rules whose pages say "no check yet" (214 rows, 11 built). The "96 unscheduled native rules" in "Where it stands" is a number with no list behind it and could not be reproduced from the pages; 41 rows name a native build, and many name a tool to delegate to (cargo-semver-checks, cargo-public-api, squawk, dependency-cruiser, jscpd, knip, vulture, cargo-machete, deptry, Bandit, cargo-geiger, cargo-deny licenses and bans, Checkov, Clippy and Ruff lints) | `docs/rules/*.md` | open | |

### Faces, configuration, redaction

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| M14 | `OPS-011` CLI basics built in 0.12.0 (Rust binaries; PM task 2026-10-29). Not built: non-Rust binaries (`package.json` `bin`, Python console scripts: NOT CHECKED), a missing-file argument probe (needs to know a binary takes a file) | PM task 2026-10-29 | built (Rust); rest open | |
| M15 | **OPS-011 follow-up, static (PM tiering 2026-10-29):** a hardcoded `nano`/`vim` instead of `$EDITOR`/`$VISUAL` in a tool that spawns an editor; a config dotfile in the home root (platform-aware: on Windows `%APPDATA%`/`%LOCALAPPDATA%`, on Linux XDG; XDG is Linux-only); hand-rolled `env::args` loops instead of clap/argparse; `-v` as version and `-V` as verbose swapped | PM task 2026-10-29 | built (0.13.0 proposed, Rust: editor, home dotfile, hand-rolled args, -v/-V; `OPS-012`) | |
| M16 | **OPS-011 follow-up, declared-feature checks (PM tiering 2026-10-29), NOT CHECKED unless declared:** stdin via `-`, `-y`/`-q`/`-v`, `NO_COLOR` when a colour crate is used (counts when set AND non-empty), Ctrl-C on a long-running binary | PM task 2026-10-29 | built (0.14.0 proposed, static, as extra cases of `OPS-012`; no dynamic probe) | |
| F1 | `cargo coderipper`: `-p/--package`, `--manifest-path`, `--sweep` and `--check <id>` as flags, the `short` message format | 3f 5, 123-135 | built (0.15.3: `--manifest-path`; 0.15.4: `-p/--package`; 0.15.5: `--sweep`, `--check <id>` flags; 0.15.6: `short` format); merged, F1 complete | |
| F2 | A working-tree snapshot mode (checks read committed HEAD); a `compiler-message`-shaped output for editors | 3f 137-142 | open | |
| F3 | A typed error enum in place of `anyhow::Error` in `Check::run` | 3f 59 | deferred | |
| F4 | Enforcing `max_runtime_minutes` (the ceilings are only reported); the dotless `/etc/coderipper/coderipper.toml` spelling; pinning `profile` against a repository's choice | cfg 135-163; CHANGELOG 0.6.21 | open; pinning waits for a hosted operator | |
| F5 | Redaction: on by default in hosted mode; stripping the scratch-directory prefix and escaping on render | redaction plan 93-106; CHANGELOG 0.6.11 | held by item 9 | |

### Hosted service (all held by CireSnave's separate go, item 9)

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| H1 | Result document and JSON schema, the ingest validator as a library function; metrics that must exist before the runner opens | hs 173-183, 299-302 | open | |
| H2 | **Ruled 2026-10-25: ask the front-end lanes and give CireSnave the best options with reasons.** Asked; the answers and a recommendation are in `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §5. Face A: GitHub Action, OIDC upload endpoint, dashboard, retention setting; needs the site's stack, which is "not decided here" | hs 178 | options sent to CireSnave | no |
| H3 | **Ruled 2026-10-25: "Yes."** Route (a), no redirect. The community post: route (a) copy button and prefill, or route (c) the service posts; the Community lane's `Idempotency-Key` on `POST /posts` (a DB migration CireSnave applies); whether a return-to-dashboard redirect is wanted | hs 134-169 | decided (route a) | no |
| H4 | **Ruled 2026-10-25: home PC first, but "everything is hardened" before it is considered.** The gate CireSnave approves is `docs/hosted-service/sandbox-hardening-gate.md`; nothing runs a stranger's code before his go. Sandbox spike on a chosen provider, the hostile fixture set, whether a home PC runs jobs in v1, emulation arm C, the spot-check rate for non-operator runners, the unmeasured isolation overheads | hs 181, 297; ri 224-275 | gate written; his approval and go outstanding | no |
| H5 | Build the runner agent, the default-deny proxy and per-repository domain approvals; the launch-gate fixtures per host class | ri 97-203 | open | |
| H6 | **Ruled 2026-10-25 ("Definitely! Sooner rather than later"); done in 0.9.42.** DOC-010 off and osv-scanner limited to the OSV host in hosted v1; re-verify the `buf` and `zizmor` network and plugin use | hs 78-88 | done | no |
| H7 | **Ruled 2026-10-25: approved; ship, with a professional review when funds allow.** The terms of service: CireSnave's approval, no lawyer review (no funds), eleven unqualified assumptions, the contact values, the renderer, installer and stored acceptance text, the unverified Copyright Office and Arizona-law points | ToS draft 3-6, 44-46, 108-121, 401-402 | approved | no |
| H8 | **Ruled 2026-10-25: leave at 2.** Any change to the per-account concurrency cap of 2 | hs 107 | decided | no |
| H9 | **Design rule for face B (patent sweep, ruled 2026-10-25: CireSnave chose A on decision 300):** face B keeps ONE fixed runner image per job; it never adds a step that selects per-tool images from an image store by a file property; it keeps the pinned-tarball fetch (no clone); a job is triggered by a user's submission only. Why: US20210200834A1 (Atlassian, "Asynchronous static analysis system for a collaborative software development environment"); the PM read its claims from the USPTO page images: not too close as designed, claim 9 broad, grant status unchecked, so re-read at grant or when Google Patents answers. The command-line tool and the library are unaffected. Recorded in `docs/superpowers/plans/2026-10-12-hosted-service.md` §3 item 10 and `docs/PATENT-SWEEP.md` hit 5. | PATENT-SWEEP hit 5; decision 300 | decided (design rule); re-read at grant | no |

### Rule families (designs merged, nothing built)

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| R1 | **Ruled 2026-10-25 (CireSnave): default on unless many false positives; a low-value rule goes on the roadmap, not away.** The allocation-in-loop family (0 of 42) stays opt-in and hot-path-only; `unbuffered-io` and `collect-then-iterate` are default-on when built (exact syntax); `vec-new-then-push-loop` is opt-in until a fresh 30 clears 70%; every other rule has a position in `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §1. Performance: build PRF-020 first, then PRF-009/015/019, then the opt-in rules; `[perf] hot_paths`, `[perf] benchmarks` and a `// coderipper: hot` marker; the loader rejects a waiver with no benchmark link; rewrite PRF-012, 014, 015 and sample small populations to 30; fix the design document's 4-of-12 and default-on lines | performance plans | open (not scheduled) | marker syntax, PRF numbering, `unbuffered-io` default-on |
| R2 | **Ruled 2026-10-25: the record schema is adopted** (`source`, `revision`, `licence`, `sha256`, `credit`; free-text records keep working; `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` §3). Provenance: SUP-015 (default-on), SUP-018, SUP-014 (needs `[provenance] own`), SUP-019 (network), SUP-016, SUP-017; route the unrecorded assets in fuel, Humboldt, vulkane, baracuda, unisql, amt and candlelight to their lanes | provenance plan 100-120 | open | record schema, SUP-019 vs cargo-deny, routing |
| R3 | **Ruled 2026-10-25: as recommended.** Security: SEC-011 (opt-in, needs the v4 matcher and a fresh 30), SEC-004a-c default-on, 004d/e and 008a/b opt-in, SEC-012/013 (need `[security] entry_points`), SEC-001 as a delegated CodeQL or Semgrep module; the weak-algorithm waiver check; section 7 waits for the auth-framework fix release | security plan 104, 230-250 | open; section 7 held | weak algorithms at Info, routing section 7 |
| R4 | A shared `cargo deny` `[bans]` list on direct backend crates outside PersistAnt | cov 47 | open | |

### Docs that disagree with the code (also sent as a [FINDING])

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| D1 | Eleven built rules still read "no check yet" on their `docs/rules` page: API-006, DOC-005, DOC-009, DOC-010, SEC-006, SUP-001, SUP-002, SUP-004, SUP-008, SUP-011, WSP-001 | `docs/rules/*.md` | open | |
| D2 | Stale status lines: the audit-host spec says "nothing implemented"; the multi-language spec and programme say "no code is written"; both keep `classic` as the default (0.9.0 made `extended` the default); the three-faces plan says `--deny` defaults to never-fail (0.5.0 made it `info`); the triage README says "zero rules are earned"; `cov` 16 says the settings audit is missing; the performance design still shows its first sample and default-on list; the security plan section 4 and the provenance plan counts (75 data files, 45 after 0.9.19) | the named documents | open | |
| D3 | `README.md` 255 says lockfiles are not judged by `patent-index-match`; `reachability/mod.rs` 38 says a nested `--project` analyses the repository root | README; `src/checks/reachability/mod.rs` | open | |

### Other lanes' work the triage records (report-only here)

| # | Work | Source | Status | Call |
|---|---|---|---|---|
| T1 | **Ruled 2026-10-25: as recommended (`* @ciresnave`).** The live list is `C:\Projects\CODERIPPER-T1-T2-TABLE.md` (14 repositories, not 11 or 24). WSP-001: CODEOWNERS on 11 repositories, the owner handle for ThinkersJournal and EvansLaboratories repositories, and the `approvals-ciresnave` profile repository | portfolio-triage 2026-10-14 26-28 | owning lanes (table sent) | no |
| T2 | **Ruled 2026-10-25: pin `actions/*` too, and pin images by digest.** The live list is `C:\Projects\CODERIPPER-T1-T2-TABLE.md` (6 repositories). SUP-008: whether `actions/*` is exempt from pinning, and waiver versus digest for service-container images (`auth-framework` has 8) | triage 76-77; q14 17 | owning lanes (table sent) | no |
| T3 | Meeting the bar for the default: SUP-001 edits, SUP-008, `public_api` declarations for published libraries, 3119 lint-suppression findings (1428 `#[ignore]` with no reason, at 2026-10-16); reachability unread on `amt`, `consent`, `vulkane`, `candlelight`, and the other Rust rules not run portfolio-wide | q14 29-45 | open (the owning lanes); the release shipped anyway (0.9.0) | |

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
