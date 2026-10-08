# Rule triage (P0): what covers what, in which language

Companion to `docs/superpowers/specs/2026-10-07-multi-language-design.md` and
`docs/superpowers/plans/2026-10-07-multi-language-programme.md`. Produced 2026-10-07 by the CodeRipper lane from the owner's
"Clean Code & Clean Architecture: A Verifiable Engineering Knowledge Base" v2.0 (not copied here: rule IDs are cited and the labels and
notes are our own wording; see "Provenance" below). **This is a plan, not coverage:** a status here says how a rule *could* be covered; nothing is earned until a
module's seeded-defect fixture passes (design section 6.2). Zero rules are earned today.

## Files

| File | What it is |
|---|---|
| `rules.tsv` | pass 1: one row per rule (216), its checkability class and attributes |
| `cells.tsv` | pass 2: one status per rule per language for the 91 first-batch rules (297 cells), tool facts and verification joined |
| `tools.tsv`, `tools_raw.json` | facts about each external tool, looked up from public registries on 2026-10-08T00:45Z |
| `verification_delegated.tsv` | 102 checks of the delegated claims against the tools' own docs or source |
| `spotcheck_sample.tsv`, `spotcheck_overrides.tsv`, `normalisation_log.tsv` | the 44 cells the lane spot-checked with a verdict each; the corrections from that and from the independent audit; every cell change made, by rule |
| `labels_override.tsv`, `licence_notes.tsv` | labels reworded for provenance; licence facts that a registry field does not carry |
| `legacy_checks.tsv` | how the five existing checks relate to KB rules |
| `raw/` | the unedited outputs of the classifier agents, and the vocabularies they were given |
| `tools/` | the scripts: `rebuild.py` runs `make_rules.py`, `normalise_cells.py`, `lookup_tools.py`, `verify_delegations.py` and `make_readme.py` in order to reproduce all of this; `check_provenance.py` needs the knowledge base (not in this repository) |

## A count to correct

The brief for this work said the knowledge base has 217 rule IDs. The document never states a count (MEASURED: the string `217` does not
occur in it). It has **216** rule rows. 217 is the number of distinct `XXX-nnn` tokens, because `ADR-014`, an example in section
I.6 ("Does this diff violate ADR-014?"), looks like a rule ID but is not one (every `| XXX-nnn |` table row gives 216, no duplicates).

## Pass 1: checkability class (one per rule)

Classes: `N` language-neutral (files, config, manifests, lockfiles, CI, VCS metadata; no parser), `P` needs a parser, resolved types or
an existing tool's diagnostics, `A` needs a declared architecture, `H` needs history mining, `X` needs execution (tests, mutation,
fuzzing, benchmarks, runtime), `L` needs LLM judgment, `M` needs a human decision, `T` is a feature of CodeRipper itself (baselines,
waivers, reporting). One class per rule: the cheapest that could yield a useful deterministic check.

| Domain | N | P | A | H | X | L | M | T | total |
|---|---|---|---|---|---|---|---|---|---|
| GOV | . | . | . | . | . | . | . | 9 | 9 |
| MOD | . | 8 | . | 1 | . | 1 | . | . | 10 |
| ARC | . | 4 | 5 | 2 | . | 1 | . | . | 12 |
| EXT | . | 6 | . | 2 | 1 | 1 | . | . | 10 |
| RDB | 2 | 7 | . | . | . | 4 | . | . | 13 |
| TYP | . | 9 | . | . | . | . | . | . | 9 |
| EFX | 1 | 4 | 1 | . | 1 | . | . | . | 7 |
| ERR | . | 8 | 1 | . | 1 | 1 | . | . | 11 |
| COR | . | 5 | 1 | . | 3 | 1 | . | . | 10 |
| TST | . | 3 | . | 1 | 7 | 2 | . | . | 13 |
| CON | . | 8 | 1 | . | 1 | 1 | . | . | 11 |
| REL | 2 | 5 | 1 | . | 3 | 1 | . | . | 12 |
| SEC | 3 | 5 | . | . | . | 1 | 1 | . | 10 |
| SUP | 10 | 1 | . | . | . | . | 2 | . | 13 |
| API | 1 | 7 | . | . | 3 | 1 | . | . | 12 |
| PRF | . | 4 | . | . | 4 | . | . | . | 8 |
| OPS | 4 | 6 | . | . | . | . | . | . | 10 |
| DOC | 4 | 2 | 2 | . | 1 | 1 | . | . | 10 |
| EVO | . | . | . | 8 | . | . | . | 1 | 9 |
| WSP | 3 | 1 | 1 | 1 | 2 | . | . | 1 | 9 |
| AIH | 3 | 1 | . | . | 1 | 3 | . | . | 8 |
| **all** | 33 | 94 | 13 | 15 | 28 | 19 | 3 | 11 | 216 |

Per-language triage was done for the 91 rules of class `N` or `P` in 13 domains, in this order: SUP, WSP, API, DOC, SEC
(language-neutral and cross-codebase first), then MOD, ARC, EXT, RDB, TYP, EFX, ERR, COR. The other 125 rules stay a pass-1 row,
marked "per-language triage deferred" in `rules.tsv`. That is by design for P0.

## Pass 2: one status per rule per language

| Column | cells | native (to write) | native via neutral module | delegated | not-applicable | NOT-COVERED |
|---|---|---|---|---|---|---|
| neutral | 24 | 13 | 0 | 7 | 0 | 4 |
| rust | 91 | 28 | 18 | 33 | 1 | 11 |
| typescript | 91 | 40 | 19 | 27 | 2 | 3 |
| python | 91 | 40 | 20 | 28 | 1 | 2 |
| **all** | 297 | 121 | 57 | 95 | 4 | 20 |

Statuses: `implemented-native` (CodeRipper writes the check), `delegated` (a named tool; the module maps its findings),
`not-applicable` (reason recorded), `NOT-COVERED` (known gap, reason recorded). "Native via neutral module" means the language-neutral
module owns it for every language; "native (to write)" is what the language module itself would have to implement.

**What this means for cost.** The matrix proposes **121 native implementations that do not exist**
(13 neutral, 28 Rust, 40 TypeScript, 40 Python) against 95
delegations that wrap a tool. A coverage percentage that counts a native proposal as covered would overstate: a native cell is work to do, not coverage. The plan therefore
schedules delegations first and treats the native cells as a prioritised backlog.

## Tools (looked up on 2026-10-08T00:45Z from crates.io, npm, PyPI and GitHub)

41 external tools plus 3 that ship with a language toolchain. All 41 were found. By the lookup
script's rule ("a release or a push in the last 365 days, and the repository not archived") 41
are maintained; that is a date test, not a quality judgement.

**Licences.** The script reads the registry's licence field and, for npm packages, also the LICENSE file the package actually ships,
because a registry field is whatever the publisher typed. That probe found a real case: `eslint-plugin-sonarjs` 4.2.2 declares
`LGPL-3.0-only` on npm but ships the **SONAR Source-Available License v1.0**, whose "Competing" clause forbids marketing "a substitute
for the functionality or value of SonarQube" (MEASURED 2026-10-08, jsDelivr copy of the 4.2.2 LICENSE). A code auditor can plausibly
compete, so the two TypeScript cells that used it (RDB-003, TYP-007) are now `NOT-COVERED`. Also: Semgrep's engine is LGPL-2.1-or-later
but the Registry rules are under the Semgrep Rules License v1.0, which allows "your own internal business purposes" only and does not allow
making the rules "available to others as a service" (MEASURED 2026-10-08 from semgrep.dev/legal/rules-license): fine for a developer
running it, **not** for bundling in a default catalog or for a hosted mode unless the rules are our own (cells SEC-001 in three languages and
TypeScript SEC-004). `pylint` is GPL-2.0-or-later (separate process only). The rest are MIT, Apache-2.0, BSD, ISC, MPL-2.0 or Artistic-2.0
(`tools.tsv`). PyPI describes `pyright` as a "Command line wrapper for pyright" (MEASURED), so the Python cells use a wrapper of Microsoft's
npm tool. `pydocstyle`, named in one note, is archived (MEASURED: archived, last push 2023-11-03).
The shipped-LICENSE probe could not confirm the declared licence at file level for: `npm:jscpd`, `npm:squawk-cli` (the package carries no licence file or the files disagree with the declared licence; the declared licence is what `tools.tsv` shows).

## How far the delegated claims were checked

All 95 delegated cells were checked against the tool's own documentation or source (`verification_delegated.tsv`, 102
checks, all confirmed): **49 cells strong** (a specific lint, rule code, subcommand or flag found; for Rust, in the locally
installed rustc and clippy) and **46 cells weak** (the tool is real and its README talks about the subject, but the exact
capability for the rule is a judgement). Nothing was installed or executed besides the local Rust toolchain's own `-W help` and
`--help` output. The precision and recall of any tool are **not** measured; a cell's `confidence` is a judgement.

**Spot-check** (`spotcheck_sample.tsv`): 44 cells (14.8% of 297) read against the knowledge base's own row text: every
`not-applicable` and `NOT-COVERED` cell as first classified (22; claims of absence are the riskiest), plus
12 random `implemented-native` and 10 random `delegated` (seed 20261007).
1 correction from the sample: Rust `MOD-003` was "not applicable" because the compiler enforces privacy, but it does so only
across crates, so it is now `NOT-COVERED`. The independent audit then corrected further cells (Rust ERR-007 and Python API-009 were
"not applicable" and are now `NOT-COVERED`; Rust ERR-011 only covers a `Result`, tested locally; two TypeScript capability claims were
overstated; two cells lost their tool to the licence finding above). All are in `spotcheck_overrides.tsv` with the reason.

## Normalisation

Three agents decided the 24 language-neutral rules independently and disagreed (one delegated to a tool where another wrote "neutral
module"). `normalise_cells.py` applies six stated rules (R1-R6; the script's header says which) and logs every change in
`normalisation_log.tsv` (74 logged changes). The raw agent output is kept in `raw/`.

## The five existing checks versus the knowledge base

`legacy_checks.tsv`. Four overlap a KB rule only partly (`reachability` ~ RDB-006 and MOD-001; `unused-parameters` ~ RDB-006;
`unused-return-values` ~ ERR-001 and ERR-011; `version-consistency` ~ WSP-003). **`ci-protection-presence` has no KB counterpart**: no
rule says the default branch must enforce required status checks. It stays ours, and is a candidate for a rule the catalog adds
beyond the KB.

## The cheapest high-value first batch (language-neutral and cross-codebase first)

No parser is needed, so one implementation serves every language (the neutral column):

1. **Wrap a maintained tool, high confidence:** SEC-002 (gitleaks), SUP-002 and SEC-006 (osv-scanner: one tool, two rules), DOC-010
   (lychee).
2. **Wrap a maintained tool, medium confidence:** SUP-008 (zizmor), SEC-009 (checkov), API-006 (buf).
3. **Small native checks on files and config:** SUP-001 (lockfile committed, CI resolves from it), SUP-011 (third-party code excluded
   from own analysis), DOC-005 (ADR links), DOC-009 (glossary present), WSP-001 (repository inventory).
4. **Give what exists a catalog record**: `version-consistency`, `ci-protection-presence` and the three Rust checks. The records only
   describe them; their implementation stays in the Rust module (design sections 3 and 11).
5. **Cross-codebase** (portfolio scope: 11 rules, three of them class `N`): WSP-001 and WSP-008 first. WSP-003 has no
   language-independent form (it needs each ecosystem's manifest).

## Provenance

The knowledge base is not in this repository. Rule IDs are cited, tool names are facts, and every label and note is written in our own
words: 21 labels or notes that repeated the document's wording were reworded (`labels_override.tsv`, applied to the committed raw files
too), and `tools/check_provenance.py` (run locally against the document) reports 0 runs of six or more consecutive shared words in
`rules.tsv`, `raw/pass1_all.tsv` and `raw/pass2_input.tsv` after that. The `kb_tools` column holds tool names copied from the rows.

## Not verified, said plainly

- The knowledge base's own tool claims and statistics (it says so itself); only the tools named in our cells were looked up.
- Any tool's behaviour: nothing but the local rustc/clippy was run, and no tool was installed (instruction).
- Arguable classes the audit raised and we left: `SUP-013` is class `M` (a human decision), but `cargo vet`-style tooling makes a
  deterministic check of it, so it may belong in `N`; it is outside the first batch, so no cell was made for it.
- Linux and macOS behaviour of anything: all lookups were made from Windows, and the triage describes no code.
- The classification itself is judgement by cheap models, corrected by a 14.8% sample and one independent audit. Expect more corrections when a module author
  implements a rule and finds its cell wrong; the conformance fixtures (design section 6.2) are what make that visible.
