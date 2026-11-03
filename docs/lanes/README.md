# Lane briefs: start cold

One page per package of the remaining ARC/MOD rules, written so a new lane can start without this conversation. Read this file, then
your package. Packages: [B](package-b.md) ARC-005 + MOD-009 · [C](package-c.md) ARC-004 + ARC-012 · [D](package-d.md) MOD-006 + ARC-009 (+ ARC-011) ·
[E](package-e.md) the remaining rules, documentation only.

## Preconditions (every lane)

- **Sonnet only** (`--model sonnet`, subagents `model: sonnet`); never Opus. Local cargo `-j2` per extra lane; unset `CODERIPPER_RESULT_CACHE`.
- Work in your own worktree `C:\Projects\coderipper-wt-<topic>` off `origin/main`; the primary checkout stays on `main`.
- Authenticate as `ciresnave-bot`; never `gh auth switch`. Messages to the PM use the compact `[READY]` / `[HOLD]` formats.
- `coderipper check` reads the committed tree: **commit before measuring**.

## One rule, one loop

1. **Read first:** `src/builtin.rs` (add one row to `builtin_checks!`), `src/checks/architecture/` (`mod.rs`, `modgraph.rs`, `api_budget.rs`
   and the newest check, `layers.rs`, are the pattern), `rules/arc.toml` / `rules/mod.toml` (the catalog entry), `docs/rules/arc.md` / `mod.md`.
2. **Implement** the check with unit tests and register it: one `builtin_checks!` row and the module line in `checks/mod.rs`. The test files derive
   their counts from the table; the only pinned lists left are the legacy six in the `src/lib.rs` tests.
3. **Conformance:** `conformance/<RULE>/defective` (seeded defect, the check must flag it) and `clean` (twin, must not).
4. **Measure** on the corpus (coderipper, auth-framework, Unpopped, PersistAnt, configurant, httpant, releaseant, synapse), committed trees; read the
   samples. Default-on only if false positives are few; otherwise opt-in via a `[ruleNNN]` table in `.coderipper.toml` (see `Mod010Settings`). A new
   default-on rule that fires on coderipper itself is fixed or waived with reason, owner and end condition.
5. **Patent sweep** ([`docs/PATENT-SWEEP.md`](../PATENT-SWEEP.md)): ideas, queries, claims read with `tools/patent-claims.py`, element-by-element charts for
   near hits. Write rows and addendum as a fragment in `patent-sweep.d/`. A hit on a live patent whose claims the rule meets is a `[HOLD]` to the PM, not a merge.
6. **Docs:** a README row, `docs/rules/*.md`, and a `changelog.d/<rule>.md` fragment. Summaries must not contain a bare "no"/"none" (the absence-claim lint).
7. **Verify:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (default and `--no-default-features`), `cargo doc`, `cargo test --no-fail-fast`, the self-sweep `coderipper check --deny info`.
8. **At [READY], ask the PM for the version**, then run `python tools/stamp-version.py <version>` (sets Cargo.toml/lock/README, writes the CHANGELOG heading,
   numbers the patent-sweep rows and deletes the fragments), commit, push, open the PR. The PM merges in READY order; a later PR rebases onto `main`
   (conflicts only in the `builtin_checks!` table: keep both rows) and re-stamps with its new number.

Do not edit `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, the README version or the numbered tables of `docs/PATENT-SWEEP.md` by hand.
