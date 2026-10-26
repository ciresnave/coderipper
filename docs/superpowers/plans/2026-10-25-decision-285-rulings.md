# Decision 285: what CireSnave ruled, and what each ruling changed in the plans

**Status: a record, written 2026-10-25 by the CodeRipper lane from the PM's task of the same day.** The rulings are CireSnave's, first-hand in the PM session, quoted verbatim in `C:\Projects\CIRESNAVE-DECIDED-ARCHIVE.md` ("285. ANSWERED 2026-10-25") and summarised as a standing rule in `C:\Projects\CIRESNAVE-EXPECTATIONS.md` §6.21. The walkthrough he decided from is `C:\Projects\CODERIPPER-285-WALKTHROUGH.md`. This file is where each ruling lands in this repository: what was built, what is a position on the roadmap, and what is still open. `ROADMAP.md` rows point here.

## 1. The standing rule behind R1 and the default-on threshold (§6.21)

> *"Unless a rule produces a lot of false positives, default them on. That holds true for all of our rules."* and, of a rule judged not worth acting on: *"If they're just deemed not high enough value, add them to our roadmaps a ways down but don't skip them entirely."*

**The written threshold (proposed by the PM, applied here; supersedes the 80% bar of ROADMAP "Default-on policy" item 4).**

1. **A rule is default-on when at least 70% of a 30-finding sample are findings a reviewer would act on**, drawn from at least three repositories of the portfolio.
2. **The record beside the rule states** the sample (which repositories, at which commits, the shuffle seed and the stopping rule), the rubric, and **who judged**. A rule judged by the lane that wrote it says so; an independent label is stronger and is asked for when a rule is borderline (60% to 80%).
3. **Stop early at the bar's impossibility:** after 9 false findings in a 30-finding sample the 70% bar cannot be reached, so the rule is not default-on on that evidence.
4. **A rule that is exact on its syntax** (a name, a type, a call, not a heuristic) and has fewer than 30 findings on the portfolio is default-on when a seeded fixture proves it both ways; a rule is not held back because the portfolio is clean of it.
5. **A rule below the bar ships opt-in, or scoped** (for performance rules: only inside a loop, or in a path declared hot with `[perf] hot_paths`). It is never deleted and never called "declined": it goes on this roadmap with a position (what would raise its precision, and where it sits in the order).
6. **Default-on never means blocking** (unchanged); opting a rule out stays per rule, in configuration, with a reason, and the opt-out count is reported.

### R1: the performance rules, against that threshold

Data: `docs/superpowers/plans/2026-10-24-performance-rules-precision.md` §2 (fuel `5a00e3a`, baracuda `8e162f7`, Unpopped `21e6279`; one reader, this lane, "not an independent labelling").

| Rule | Sample | Position |
|---|---|---|
| Allocation in a loop: `format!`, `clone`, `Vec::new`/`vec!`/`String::from`, `to_string` (PRF-016) | 0 of 42 | **Opt-in, hot paths only.** False-positive heavy; the 95% upper bound on its precision is about 7%. Raising it needs a sample drawn from declared hot paths, and none is declared yet. Scheduled after `[perf] hot_paths` exists. |
| `unbuffered-io` (PRF-011) | 2 of 2, a census | **Default-on** under item 4: exact on its syntax (`File` written per item without `BufWriter`), proven both ways by a seeded fixture when built. Both findings are in cold build scripts; the finding says so. |
| `collect-then-iterate` (PRF-015) | 0 of 2, a census | **Default-on** under item 4 once it ignores a collect into a different collection type (the two findings were deliberate deduplications); exact on the chain, nothing left to sample. |
| `vec-new-then-push-loop` (PRF-014) | 7 of 9 (78%), a census of 9 | **Opt-in until a fresh 30** from other repositories clears 70%; fix the two causes it showed (pushes in nested loops; a deliberate `Vec::new()` with a comment). 78% on 9 does not meet item 1. |
| `loop:to_owned`/`to_vec` | 6 of 20 (30%) | **Opt-in, a second prototype first:** fire only when the copy is only read afterwards (needs a use analysis). Roadmap, a way down. |
| `loop:iter-linear-search` and `loop:contains(vec)` | 1 of 10; 0 of 12 | **Not dropped.** The failures are size-blind; roadmap a way down as one narrower rule: an equality-keyed lookup in a collection that grows with the input. The plan's "drop" for `contains(vec)` is replaced by that. |
| `nested-loop-same-collection` (PRF-013) | 1 of 3, a census | **Opt-in**; sample more. |
| `Vec<Box<T>>`, `LinkedList`, `Rc<RefCell<T>>` (PRF-018), a regex in a loop, an N+1 call, `repr(C)` padding (PRF-019) | no finding in these three repositories | Exact on their syntax: **default-on** under item 4 when built, scoped as the plan says (PRF-018 only under `hot_paths`). |
| Claims need a benchmark (PRF-020) | not sampled | Built first, as the roadmap row says; its measurement is the next sample. |

**Not built.** Everything in this table is a position; no performance rule exists in the code yet.

## 2. M8: the programme's seven questions

Bundle accepted (`docs/superpowers/plans/2026-10-07-multi-language-programme.md` §4, lines 161 to 188), **with Q12 changed**:

> *"I agree with the bundle except the 'build checks ourselves only on demand'. I believe that one should be taken on a case-by-case basis as many checks are too valuable to delay, so if others haven't built them, we need to sooner rather than later."*

So Q3 (Go next), Q4 (modules in Rust), Q6 (no architecture-intent language yet), Q8 (restrictive tools are optional integrations, licence shown), Q10 (no LLM tier before the sixth phase), Q13 (build natively only where neither default nor mature third-party tooling suffices; integrate mature tools as external processes) stand as the programme recommends. **Q12 now reads: a check that is valuable and that no mature tool already provides is built now, case by case; the rest are reported as not covered, not as planned.**

### Valuable and unbuilt: the candidates (this lane's judgement)

The built-in catalog has 225 rules; 22 are covered and 203 have "no check yet" (`--coverage=full` at 0.9.43, measured by the coverage report). Fifteen of the 203 are default severity **high**; with the rules that overlap a tool someone else already provides set aside, these are the ones worth building or delegating soon. "Others" means a mature tool exists; **whether each tool does the whole job is not verified**, except where a row says it was run.

| Rule | What it would catch (example) | Built by others? | Position |
|---|---|---|---|
| AIH-002 referenced APIs and new packages really exist | an AI-written `Cargo.toml` naming a crate that does not exist (or exists only as a squatter) | no mature tool found | **Build now**, native, in the sweep tier: the registry lookup `dependency-freshness` already does (0.9.28) is most of it |
| SUP-012 new package names screened for squatting | a dependency one letter from a popular crate | cargo-deny checks advisories and licences, not names | **Build next**, with AIH-002 (same registry data) |
| API-001 breaking public changes since the last release | a removed `pub fn` in a patch release | cargo-semver-checks (Rust); v0.51.0 ships archives for Windows, Linux and macOS (assets read 2026-10-25) | **Delegate** (per M7 the language does not matter); the version-vs-API portfolio check overlaps it |
| API-007 database migrations safe step by step | an `ALTER TABLE` that locks a big table | squawk (Postgres); v2.70.0 ships single-file binaries (assets read 2026-10-25) | **Delegate** |
| SEC-004 cryptography misuse | a hard-coded key, TLS verification off | clippy has none of these | **Build** natively as the security plan orders it (SEC-004a to c default-on) |
| SEC-001 injection, traversal, zip-slip | untrusted input reaching a file path | CodeQL, Semgrep (licence of their rule packs is the open point, triage README 95 to 100) | **Delegate** first, native subset after (R3, as recommended) |
| SEC-009 least privilege in infrastructure | a wildcard IAM policy, a privileged container | checkov | **Done in 0.9.45** (M7) |
| OPS-006 no secrets in configuration files | a token in a `.env` | gitleaks (already SEC-002) | Reuse SEC-002's result under this id; check the overlap before building anything |
| CON-006 no lock-order cycles | thread A takes lock 1 then 2, B takes 2 then 1 | none for Rust at lint level (loom and shuttle need tests; clippy has `await_holding_lock` only) | **Build** (native), after the AIH rules; the highest-value Rust-specific gap |
| CON-001 no data races | a shared mutable without a lock | the compiler stops most in safe Rust | Low value in safe code; position: with `unsafe` review, later |
| ARC-001, ARC-002, ARC-003 component graph, cycles | a module cycle | cargo-modules (cycles); the graph needs an architecture file (Q6: not now) | **ARC-002 delegate** to a module-graph tool; ARC-001 and 003 wait for Q6 |

The other 188 rules stay on the roadmap in catalog order (row M13); none is declined.

## 3. R2: the provenance record format is adopted

> *"R2: Agreed."* (the PM's recommendation: adopt a record schema.)

**The schema.** A provenance record names, per asset or per directory of assets: `source` (a URL or `owner/name`), `revision` (a commit, tag or content id, never a branch), `licence` (an SPDX id), `sha256` (of the **committed** bytes; a git blob id is also accepted) and `credit` (the holder, in the licence's words). It is written either as a Markdown table whose header row uses those five words, or as a `*.provenance` TOML file of `[[asset]]` tables with those keys. Records in the free-text style the portfolio uses today **keep working**: the rules read table rows and prose as the plan says, and the schema only makes SUP-015, SUP-016 and SUP-019 exact when it is present. Nothing about an existing record changes.

**Order (unchanged, now ruled):** SUP-015 (default-on), SUP-018 once the extractor reads only `Copyright (c) YEAR Holder` and SPDX lines, SUP-014 (needs `[provenance] own`), SUP-019 (network; sweep tier), then SUP-016 and SUP-017 opt-in. SUP-019 compares a record's licence with the source's; the *dependency* licence check against `cargo-deny`'s policy is SUP-003, a different tool, and stays separate. **Routing the unrecorded assets** (fuel, HumboldtUnifiedKidTracker, vulkane, baracuda, unisql, amt, candlelight) to their lanes is the PM's: one small task each, the files listed in `docs/superpowers/plans/2026-10-24-provenance-rules.md` §5.

## 4. R3: as recommended

> *"R3: Agreed."* The security order in `docs/superpowers/plans/2026-10-24-security-crypto-rules.md` §8 stands: weak algorithms reported at **Info** with a waiver that names the protocol, SEC-011 opt-in until a fresh 30 clears the bar (it fell to 47% on other repositories), section 7 (the auth-framework sites) routed privately through the PM and printed only after the fix is released.

## 5. H2: the dashboard's stack (CireSnave asked for a list of the best options with reasons)

> *"H2: Ask the front-end lanes and provide me with a list of the best options and reasons for them."*

The page needs an upload endpoint for a GitHub Action, a per-account result list, a retention setting and a "copy to Community post" button (`docs/superpowers/plans/2026-10-12-hosted-service.md:178-180`). The four front-end lanes were asked on 2026-10-25; their answers (verbatim substance), with the Journal site's own stack read from its repository (`ThinkersJournal.com`: Astro 7 with a `functions/` directory, i.e. Cloudflare Pages Functions; not verified beyond its `package.json`):

| Option | Who suggested it, and why | Cost and risk |
|---|---|---|
| **A. An Astro server-rendered page on the Community "web" Worker, plus API routes and one or two tables** | Community lane. Reuses login, session, CSRF and the prefill link (`/new-post#cr=...`) the copy button needs; no new framework; same deploy. | Needs a **per-account API token** for the Action (Community has none: a new token table and auth path); **every table is a migration CireSnave applies before the code ships**; a cron purge for retention. No Community lane has agreed to host it: the PM must board it as a Community row. |
| **B. A standalone Cloudflare Worker with D1 (SQLite)**, static assets for the UI, GitHub OIDC or a per-account token for uploads | Foundation lane. Ships independently, no Community migration, no second framework beyond Workers; D1 is small and cheap. | A second login for users unless OIDC is used end to end; one more deploy and CI to maintain; any Cloudflare change is run by the PM or CireSnave. |
| **C. SvelteKit (static) over a small Rust axum + sqlx JSON API on Postgres** | Humboldt lane (its own stack). One language per tier already shipped there; typed rows; plain CRUD. | The Thinker's Journal site is not Svelte, so nothing is reused; a Postgres to host and migrate; a token scheme separate from browser cookies. |
| **D. Server-rendered HTML from axum (askama or maud) with htmx, no JavaScript build** | Humboldt lane's alternative. Fewest moving parts. | The copy button and list filtering need hand-written JavaScript; does not match the Journal's look. |

**This lane's recommendation: A if Community can host it, otherwise B.** A reuses the one login and the prefill route that route (a) of the community post (H3) already depends on; its cost is Community's lane time and one migration. B is the fallback that needs nothing from Community. C and D are good designs for a different site. Whether the Journal site's own stack changes the answer is the open question the Humboldt lane flagged; the synapse lane's answer, if one arrives, is added to the table.

## 6. H3, H4, H6, H7, H8, M12, M3, C7: where each ruling is recorded

| Item | Ruling (CireSnave, 2026-10-25) | Recorded in |
|---|---|---|
| H3 the Community post | *"H3: Yes."* Route (a): a copy button and a prefilled editor, no return-to-dashboard redirect; the idempotency key and its migration are Community's lane, applied by CireSnave | `docs/superpowers/plans/2026-10-12-hosted-service.md` §6.1 |
| H4 sandbox | *"Home PC first but we do our best to make sure everything is hardened before we even consider it."* | `docs/hosted-service/sandbox-hardening-gate.md` (new): the checklist he approves; nothing runs a stranger's code before his go |
| H6 DOC-010 off, osv limited | *"Definitely! Sooner rather than later, please."* | done in 0.9.42 (#165), same plan §3 |
| H7 terms of service | *"Approve it. When we can afford a professional review, we will get one...but we can't just not release it before then."* | `docs/hosted-service/terms-of-service-draft.md`, status banner |
| H8 two jobs per account | *"Agreed. Leave it."* | same plan §3 table row |
| M12 `ring` | *"Ring is fine for now but keep our eyes out for a pure-Rust alternative once one matures."* | ROADMAP row M12: a watch line |
| M3 pass percentage | *"I think percentage passing is information every run should provide."* | built in 0.9.44 (#166); `--min-coverage` stays an optional floor on the roadmap |
| M7 checkov | *"If an existing checker is good, we should support it regardless of the language it is written in."* | built in 0.9.45; the isolated Python-environment installer is a roadmap row |
| C7 cross-repository | *"Lets keep cross-repo abilities. If you feel building a cache first is important, do so."* | ROADMAP row C7 |

## 7. C7: cross-repository runs and the cache that comes first

> *"CodeRipper was designed to work across multiple projects at once right from the start. C7 sounds like you're suggesting we limit it. If we do, we cut out a lot of what CodeRipper should be capable of. Lets keep cross-repo abilities. If you feel building a cache first is important, do so."*

The `--scope` list (one command over several repositories) is no longer held. It is built in two slices, **the cache first**, because the portfolio sweep is the use that needs the speed.

**Why the cache is first (this lane's judgement).** A whole-workspace run of the 45-member `fuel` took 1742 s cold and 915 s warm for one rule (ROADMAP "Where it stands", measured 2026-10-15 on `fuel@8cdb33dd`); the existing build cache (merged as 0.2.7, #20) and the session checkout already remove dependency and sibling recompilation, so what is left is the per-member analysis itself. A weekly cross-repository sweep re-analyses mostly unchanged code. The PM's ruling of 2026-10-15 is that **the lever is an incremental cache, not parallelism** (cargo's build lock serialises builds in one target directory anyway).

**Slice 1: a result cache (the incremental cache).** For a project (or workspace member) and a rule set, the key is the git tree hash of the member's directory **and of every in-repository path dependency it builds against**, the CodeRipper version, the rules requested with their configuration and allowlist, the toolchain and the tool versions the rules used. A run whose key matches a stored entry reuses its findings and says so (`cached` in the human line and in the JSON). Rules:
- **Correctness never depends on the cache** (as for the build cache): a miss, an unreadable entry or a key it cannot compute runs the rule. A rule that read the network or a registry (the sweep tier) is keyed by the registry's answer date, or is not cached; it is never served stale as fresh.
- A cached entry is **never** served for a rule that ended in a gap or an error: only a rule that ran to a verdict. `--no-cache` and `CODERIPPER_CACHE=off` turn it off.
- Stored beside the build cache, under its size cap and pruning; never in a project's tree.
- **Done when** a second run over an unchanged `fuel@8cdb33dd` re-analyses no member (measured, not asserted), and a one-member change re-analyses that member and its dependents only. **Built in 0.10.2** (README "Build cache": keyed on the whole committed tree, so one changed member re-analyses all; measured on this repository, not on `fuel`: `reachability` 48 s, then 1 s; a member-level key is open).

**Slice 2: `--scope <file>`.** One command over a list of repositories, each analysed under its own `.coderipper.toml`, output grouped by repository, the exit code the worst of them, a repository that cannot be read reported as an error for that repository (the others still run). The list is a file of repository paths (one per line; `#` comments), so the portfolio sweep is `coderipper sweep --scope portfolio.list`. The scope never reads a working tree another lane may be editing: it enumerates from the index (`git ls-files`) of each repository, as every rule does. **Built in 0.10.3** (README "Many repositories in one command").

**What stays out:** parallel members (measure after slice 1), and "siblings surviving between runs" (needs content-keyed freshness, not available on stable cargo).
