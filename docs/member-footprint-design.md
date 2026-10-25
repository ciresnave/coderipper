# Per-member footprint limit for `--workspace` sweeps (design)

Task from the PM, 2026-10-25. Consumer: Fuel's `portfolio.yml` job. On a GitHub-hosted runner (about 14 GB) it fails with "Out of diskspace" (index.lock write, cache save), and `fuel-aocl-cpu-backend` and `fuel-cuda-backend` fail to link (no SDK), so they are "not judged", which fails the job by design (`cli.rs`, the `members_failed` note and exit code). Run: fuel#493 job 121087704477, coderipper 0.9.14: 6362 units fresh, 2017 compiled, 23 of 45 members analysed. **Not reproduced here; every number is Fuel's report.** Self-hosted is ruled out for Fuel (public repository, board 241).

## Two problems, two mechanisms

### A. A member that cannot be built where the run is: a declared, reasoned skip

A repository's `.coderipper.toml` gains an entry per member, like `[[allow]]` (reason and owner, reviewed in the pull request that adds it, counted in the report):

```toml
[[skip_member]]
name = "fuel-cuda-backend"
needs = "nvcc"            # optional: skipped only while this executable is absent from PATH
reason = "links the CUDA toolkit; no hosted runner has it"
owner = "fuel lane"
```

- The member is not built. The run prints `not judged (declared skip): fuel-cuda-backend: <reason> (owner <owner>)`, once per member, and counts it beside "members analysed". It is not an error and not in `members_failed`, so the job does not fail on it.
- With `needs`, the skip applies only while the tool is absent: a runner that has `nvcc` builds and judges the member, so the waiver cannot hide a member that could have been judged. Without `needs` it is unconditional (a member that never builds on any runner), and says so in the note.
- Never silent, never a way to quiet a finding: a member that is skipped is named every run. An entry naming no member of the workspace is a finding (as `unknown_crate` is for `public_api_crates`), so a rename cannot leave a stale waiver.
- No command-line flag. `portfolio.yml` fixes the command so a caller cannot drift; repository-specific facts live in the repository file (`docs/portfolio-ci-coverage.md` section 2).
- Where: `package::workspace_members` already lists members; the loop in `lib.rs` (`run_workspace_over_in`) skips a member before `run_checks_over_in` and records it in a new `WorkspaceRun::members_skipped`. `portfolio.yml` only fails on notes containing `not run:`, so the new note must not contain that string.

### B. Disk: bound the build cache during the run

`CODERIPPER_CACHE_MAX_GB` (default 20) is enforced only when the first build of a process starts (`build_cache/mod.rs`), so one workspace run can grow past it, and 20 is above a 14 GB runner anyway.

1. **Measure first** (before any change): run Fuel's sweep locally with the cache on a scratch directory and print the cache size after each member (a `coderipper: note: build cache N GB after <member>` line, behind `CODERIPPER_CACHE_REPORT=1`). The result says whether growth is dependency units (shared, the bulk), per-member artifacts, or the per-member worktree checkouts. The mechanism below is chosen after that; this document does not claim which dominates.
2. **Enforce between members:** after each member, if the repository's cache directory exceeds the cap, delete it whole and say so (`build cache over cap (N GB > M GB): dropped; later members recompile`). Correctness never depends on the cache (module doc), so the cost is time, not results. A partial prune is rejected: cargo's fingerprints and artifacts for one unit are not safe to remove piecemeal.
3. **Hosted default:** `portfolio.yml` sets `CODERIPPER_CACHE_MAX_GB` to a value that fits a 14 GB runner with the checkout (the number comes from step 1) and does not save a cache that is over the cap, so "No space left on device" cannot come from the cache save.

## Build order

1. This design (docs only).
2. A (skip entry, note, stale-entry finding, tests: skipped member not built and not failed; `needs` present builds; unknown name found).
3. B step 1 measurement on Fuel's workspace; then steps 2 and 3.
4. Publish is the PM's; Fuel then bumps its pin (to at least the release carrying A) and adds its two `[[skip_member]]` entries.

## Not decided here

Whether a member skipped for a missing SDK should still have its non-building checks (SUP-004 sources, version) run on its manifest. Recommended: yes, later; the first version skips the member wholesale and says so.
