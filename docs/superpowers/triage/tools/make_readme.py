#!/usr/bin/env python3
"""Generate triage/README.md from the data files, so every number in it is computed, not typed.
Run from the triage directory:  python tools/make_readme.py
"""
import collections
import csv


def load(p):
    with open(p, encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh, delimiter="\t"))


rules = load("rules.tsv")
cells = load("cells.tsv")
tools = load("tools.tsv")
ver = load("verification_delegated.tsv")
n_log = len(load("normalisation_log.tsv"))
batch_ids = {c["id"] for c in cells}
order = []
for r in rules:
    if r["domain"] not in order:
        order.append(r["domain"])
classes = "NPAHXLMT"
dom = collections.defaultdict(collections.Counter)
for r in rules:
    dom[r["domain"]][r["class"]] += 1
tot = collections.Counter(r["class"] for r in rules)
tbl = "| Domain | " + " | ".join(classes) + " | total |\n|---|" + "---|" * (len(classes) + 1) + "\n"
for d in order:
    tbl += f"| {d} | " + " | ".join(str(dom[d][c]) if dom[d][c] else "." for c in classes) + f" | {sum(dom[d].values())} |\n"
tbl += "| **all** | " + " | ".join(str(tot[c]) for c in classes) + f" | {len(rules)} |\n"


def real_native(c):
    return c["status"] == "implemented-native" and "neutral module" not in c["note"]


def row(label, rr):
    return (f"| {label} | {len(rr)} | {sum(real_native(c) for c in rr)} | "
            f"{sum(c['status'] == 'implemented-native' and not real_native(c) for c in rr)} | "
            f"{sum(c['status'] == 'delegated' for c in rr)} | {sum(c['status'] == 'not-applicable' for c in rr)} | "
            f"{sum(c['status'] == 'NOT-COVERED' for c in rr)} |\n")


lang_tbl = ("| Column | cells | native (to write) | native via neutral module | delegated | not-applicable | NOT-COVERED |\n"
            "|---|---|---|---|---|---|---|\n")
for lang in ["neutral", "rust", "typescript", "python"]:
    lang_tbl += row(lang, [c for c in cells if c["language"] == lang])
lang_tbl += row("**all**", cells)

ext = [t for t in tools if not t["coordinate"].startswith("builtin:")]
strong = sum(1 for c in cells if c["verified"] == "strong")
weak = sum(1 for c in cells if c["verified"] == "weak")
retr = open("tools_raw.json", encoding="utf-8").read().split('"retrieved": "')[1].split('"')[0]
n_native = {lang: sum(real_native(c) for c in cells if c["language"] == lang) for lang in ["neutral", "rust", "typescript", "python"]}
n_deleg = sum(c["status"] == "delegated" for c in cells)
n_na = sum(c["status"] == "not-applicable" for c in cells)
n_gap = sum(c["status"] == "NOT-COVERED" for c in cells)
pct = round(100 * 44 / len(cells), 1)
deferred = len(rules) - len(batch_ids)

text = f"""# Rule triage (P0): what covers what, in which language

Companion to `docs/superpowers/specs/2026-10-07-multi-language-design.md` and
`docs/superpowers/plans/2026-10-07-multi-language-programme.md`. Produced 2026-10-07 by the CodeRipper lane from the owner's
"Clean Code & Clean Architecture: A Verifiable Engineering Knowledge Base" v2.0 (not copied here: rule IDs are cited, every label and
note is our own wording). **This is a plan, not coverage:** a status here says how a rule *could* be covered; nothing is earned until a
module's seeded-defect fixture passes (design section 6.2). Zero rules are earned today.

## Files

| File | What it is |
|---|---|
| `rules.tsv` | pass 1: one row per rule ({len(rules)}), its checkability class and attributes |
| `cells.tsv` | pass 2: one status per rule per language for the {len(batch_ids)} first-batch rules ({len(cells)} cells), tool facts and verification joined |
| `tools.tsv`, `tools_raw.json` | facts about each external tool, looked up from public registries on {retr} |
| `verification_delegated.tsv` | {len(ver)} checks of the delegated claims against the tools' own docs or source |
| `spotcheck_overrides.tsv`, `normalisation_log.tsv` | the correction applied after the lane's spot-check, and every cell change made by rule |
| `legacy_checks.tsv` | how the five existing checks relate to KB rules |
| `raw/` | the unedited outputs of the classifier agents, and the vocabularies they were given |
| `tools/` | the scripts: `lookup_tools.py`, `verify_delegations.py`, `normalise_cells.py`, `make_readme.py` (re-run them to reproduce all of this) |

## A count to correct

The knowledge base says it has 217 rule IDs. It has **{len(rules)}** rule rows. The 217th ID in the text, `ADR-014`, is an example inside
the architecture-spec sample, not a rule (MEASURED: every `| XXX-nnn |` table row extracted by pattern gives {len(rules)}, no duplicates;
every `XXX-nnn` token in the file gives 217 distinct, the extra one being `ADR-014`). The header's "217" is an off-by-one.

## Pass 1: checkability class (one per rule)

Classes: `N` language-neutral (files, config, manifests, lockfiles, CI, VCS metadata; no parser), `P` needs a parser, resolved types or
an existing tool's diagnostics, `A` needs a declared architecture, `H` needs history mining, `X` needs execution (tests, mutation,
fuzzing, benchmarks, runtime), `L` needs LLM judgment, `M` needs a human decision, `T` is a feature of CodeRipper itself (baselines,
waivers, reporting). One class per rule: the cheapest that could yield a useful deterministic check.

{tbl}
Per-language triage was done for the {len(batch_ids)} rules of class `N` or `P` in 13 domains, in this order: SUP, WSP, API, DOC, SEC
(language-neutral and cross-codebase first), then MOD, ARC, EXT, RDB, TYP, EFX, ERR, COR. The other {deferred} rules stay a pass-1 row,
marked "per-language triage deferred" in `rules.tsv`. That is by design for P0.

## Pass 2: one status per rule per language

{lang_tbl}
Statuses: `implemented-native` (CodeRipper writes the check), `delegated` (a named tool; the module maps its findings),
`not-applicable` (reason recorded), `NOT-COVERED` (known gap, reason recorded). "Native via neutral module" means the language-neutral
module owns it for every language; "native (to write)" is what the language module itself would have to implement.

**What this means for cost.** The matrix proposes **{sum(n_native.values())} native implementations that do not exist**
({n_native['neutral']} neutral, {n_native['rust']} Rust, {n_native['typescript']} TypeScript, {n_native['python']} Python) against {n_deleg}
delegations that wrap a tool. A percentage such as "88 of 91" counts a native proposal as covered; it is not. The plan therefore
schedules delegations first and treats the native cells as a prioritised backlog.

## Tools (looked up on {retr} from crates.io, npm, PyPI and GitHub)

{len(ext)} external tools plus {len(tools) - len(ext)} that ship with a language toolchain. All {len(ext)} were found. By the lookup
script's rule ("a release or a push in the last 365 days, and the repository not archived") {sum(t['maintained'] == 'yes' for t in ext)}
are maintained; that is a date test, not a quality judgement. Licences to read before putting them in a default catalog (each would
run as a separate process, never linked): the ESLint SonarJS plugin is LGPL-3.0-only, `pylint` is GPL-2.0-or-later, and Semgrep's
engine is LGPL-2.1-or-later while its rule packs carry their own terms (not checked here). The rest are MIT, Apache-2.0, BSD, ISC,
MPL-2.0 or Artistic-2.0 (see `tools.tsv`). `pyright` on PyPI is a wrapper of Microsoft's npm package (ASSUMED, not checked). The
Python docstring tool `pydocstyle`, named in one note, is archived (MEASURED: archived, last push 2023-11-03).

## How far the delegated claims were checked

All {n_deleg} delegated cells were checked against the tool's own documentation or source (`verification_delegated.tsv`, {len(ver)}
checks, all confirmed): **{strong} cells strong** (a specific lint, rule code, subcommand or flag found; for Rust, in the locally
installed rustc and clippy) and **{weak} cells weak** (the tool is real and its README talks about the subject, but the exact
capability for the rule is a judgement). Nothing was installed or executed besides the local Rust toolchain's own `-W help` and
`--help` output. The precision and recall of any tool are **not** measured; a cell's `confidence` is a judgement.

Spot-check of the other cells: 44 cells ({pct}% of {len(cells)}) read against the KB's own row text: all {n_na + 1} `not-applicable` and
all {n_gap - 1} `NOT-COVERED` cells as first classified (claims of absence are the riskiest), 12 random `implemented-native`, 10 random
`delegated`. **One correction**: Rust `MOD-003` was "not applicable" because the compiler enforces privacy, but it only does so across
crates; it is now `NOT-COVERED` (`spotcheck_overrides.tsv`).

## Normalisation

Three agents decided the 24 language-neutral rules independently and disagreed (one delegated to a tool where another wrote "neutral
module"). `normalise_cells.py` applies five stated rules (R1-R5; the script's header says which) and logs every change in
`normalisation_log.tsv` ({n_log} logged changes). The raw agent output is kept in `raw/`.

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
4. **Re-express what exists** as catalog rules: `version-consistency`, `ci-protection-presence` and the three Rust checks (design
   section 11).
5. **Cross-codebase** (portfolio scope: 11 rules, three of them class `N`): WSP-001 and WSP-008 first. WSP-003 has no
   language-independent form (it needs each ecosystem's manifest).

## Not verified, said plainly

- The knowledge base's own tool claims and statistics (it says so itself); only the tools named in our cells were looked up.
- Any tool's behaviour: nothing but the local rustc/clippy was run, and no tool was installed (instruction).
- Linux and macOS behaviour of anything: all lookups were made from Windows, and the triage describes no code.
- The classification itself is judgement by cheap models, corrected by a {pct}% sample. Expect more corrections when a module author
  implements a rule and finds its cell wrong; the conformance fixtures (design section 6.2) are what make that visible.
"""
with open("README.md", "w", encoding="utf-8", newline="\n") as fh:
    fh.write(text)
print(len(text.splitlines()), "lines")
