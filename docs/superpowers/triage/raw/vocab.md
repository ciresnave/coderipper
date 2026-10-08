# Pass-1 classification vocabulary (CodeRipper triage)

You classify RULE ROWS of a software-quality rule catalog by *what evidence is needed to check them*. You are NOT judging
whether a rule is good, and you do NOT verify any tool.

## Classes (choose ONE `class` per rule: the cheapest class that could yield a USEFUL deterministic check)

- `N`  language-neutral: decidable from files, config, manifests, lockfiles, CI settings, VCS metadata or plain text, WITHOUT
       parsing a programming language (e.g. "a lockfile is committed", "a licence file exists", "secrets matching patterns").
- `P`  needs a language parser, resolved symbols/types, or the diagnostics of an existing language tool (linter, type checker,
       compiler, API-diff tool). Most code-structure rules.
- `A`  the primary input is a DECLARED architecture/intent specification (components, allowed edges, public-surface budget);
       code facts are secondary. (Put `P` in `also`.)
- `H`  needs version-control history mining (churn, co-change, ownership).
- `X`  needs EXECUTION: tests, coverage, mutation, fuzzing, benchmarks, sanitizers, runtime telemetry.
- `L`  only an LLM/semantic judgment can check it (names vs behaviour, comment drift, "is this abstraction justified").
- `M`  needs a human decision recorded as an artifact (waiver, threat model, approval).
- `T`  a feature of CodeRipper itself (baselines, ratchets, waivers, reporting), not a check of the analysed code.

Rule: if a rule has both a deterministic path and an LLM path, choose the deterministic class and put `L` in `also`.
`also` = comma list of other classes the full rule would need (may be empty `-`).

## Other columns

- `scope`: `project` (needs only this repo) | `portfolio` (needs several repos). 
- `network`: `local` | `network` (calls a registry/API/vulnerability database).
- `executes_code`: `yes` if checking it plausibly RUNS project code, build scripts, config files or tests; else `no`.
- `kb_tools`: tool names exactly as the rule row lists them, comma separated, no spaces inside a name, or `-`. COPY them;
  do not add tools from your own memory and do not judge them.
- `label`: your OWN words, at most 8 words, naming the rule. Never copy the row's wording.
- `note`: at most 12 words, your own words, only if something is non-obvious; else `-`.

## Output

Write a TAB-separated file (no header row is NOT allowed: first line is the header), UTF-8, LF line endings, columns:

`id	domain	label	evidence_tiers	class	also	scope	network	executes_code	kb_tools	note`

`evidence_tiers` is copied from the row's Tier column (e.g. `V0`, `V1,V4`; replace `·` and `+` with `,`).
One line per rule row, in the order they appear. No tabs or newlines inside a field.
