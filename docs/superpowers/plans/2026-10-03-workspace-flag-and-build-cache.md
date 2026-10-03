# `--workspace` and a dependency-build cache — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **STATUS (revision 6, 2026-10-03): PR 1 (the cache) is merged (#20, 0.2.7). This revision replaces PR 2 and adds a PR 1.1 fix. PR 2 is
> a *session checkout* design (below), after Task 0 measured that sibling crates are what is left once the cache is in. Two independent
> audits so far; labels below are prefixed with the revision whose audit raised them.**
> **Rev-4 audit (on the first session design), all accepted:** R4-B1 a concurrent CodeRipper run can poison a long session through the shared
> cache, so the session holds the cache lock for its whole run, and the same interleave exposes the merged PR 1 (section "PR 1.1"); R4-B2 the
> hazard test could not fail, so it drives the session in-crate with an explicit order and per-unit freshness; R4-B3 dev-dependency cycles exist
> in fuel, so no member ordering is used (it had no measured benefit); R4-B4 the session's target directory had no path to the build and `git clean`
> deleted untracked files, so it is dropped for a tracked-files `git status` check.
> **Rev-5 audit (on revision 5), all accepted:** R5-N1 a session that holds the lock would deadlock against `build_with`'s per-build `acquire`
> (a second handle in one process gets `WouldBlock`), so in a session `build_with` never calls `acquire`; R5-N2 in-crate tests need a cache seam that
> does not exist, so a thread-local config override is specified; R5-N3 the overlap test did not pin an arrangement that fails without the lock;
> plus eight should-fix items (false poison from a rewritten `Cargo.lock`, units keyed ambiguously, `freshen` under-specified, the cycle count, poison
> semantics, `RewrittenWorktree`'s `Drop`, label collisions, evidence references).
> PR 2 has had no code written and is not approved until revision 6 is accepted.
> The rest of this banner is the history of revision 3.
>
> **STATUS: PLAN FOR APPROVAL (revision 3, after two independent audits). No code has been written for this plan.**
> The audit found four blocking defects, all accepted and fixed below; the largest (B1) is a *measured limit of the idea itself* (rows H and I below).
> Revision 3 folds in the second audit: the busy-lock holder is written to an unlocked sidecar (a locked file cannot be read on
> Windows), the payoff test asserts relative properties instead of an exact count, and `collect_dead_code_in` gets the source repo.
> In-repo path crates are recompiled in every new worktree, so the cache helps registry, git and out-of-repo dependencies, not a
> workspace's own sibling members. See "What was measured" rows H and I and "Where the cache does not help". Unlike earlier plans, the code here is *described*, not
> pre-built: what was verified is the facts it rests on (the experiments in "What was measured"). Each task below says which test
> fails first. Approval of this plan is not approval of any open decision in "Decisions for the PM"; those need answers first.

**Goal:** Run the project-scope checks over *every* member of a cargo workspace in one command
(`coderipper check reachability --workspace --project fuel`), with findings tagged by member, without paying the full
*external* dependency build once per member per check. Today each run builds its dependencies from scratch in a throwaway directory
(README: "about two minutes for a large crate"), and `--workspace` over `fuel` (43 workspace members, per `cargo metadata --no-deps`
on the fuel clone) would multiply that by every member and every check. The cache removes that multiplication for registry, git
and out-of-repo dependencies. It does **not** remove the rebuild of a member's *sibling* members (in-repo path dependencies); how big
that share is on `fuel` is the first thing the plan measures (Task 0).

**Architecture:** Two independent pieces, shipped as two PRs, **the cache first** (it helps today's `--project` runs and is
measurable on its own):
1. **Build cache.** Every build the checks run gets a `CARGO_TARGET_DIR` that persists across runs, private to CodeRipper, keyed by
   toolchain and repository, guarded by CodeRipper's own *bounded* lock, with a size cap. Cargo's own fingerprinting does the reuse; we
   only give it a stable directory. Correctness never depends on the cache: if it cannot be used, the build runs in a throwaway
   directory exactly as it does today, and says so.
2. **`--workspace`.** A host loop over `cargo metadata --no-deps` members. A new `Check::unit()` says whether a check judges one
   *package* (run per member) or the whole *repository* (run once, at the workspace root). Each member is judged exactly as
   `--project <member>` is judged today (same allowlist file, same stale-entry logic), so `--workspace` adds no new semantics
   to the checks; it only repeats the existing ones and labels the output.

**Tech Stack:** Rust 2021, std only for the lock (`File::try_lock`, stable since 1.89; this box has 1.98.1). No new dependency is
planned (cache location from `LOCALAPPDATA` / `XDG_CACHE_HOME` / `HOME`, read with `std::env`).

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §1-2 (scopes, host loop). The spec's "cached per-project symbol
index" (§1, last paragraph, `Portfolio`-scope) is a different, later cache and is **not** this one.

## Global Constraints

- Lanes never `gh auth switch`, never `activate_project`; nothing here touches either. (CLAUDE.md §0, §1.)
- **The cache must never be under `C:\Projects`, inside any project's `target/`, or in `~/.cargo`**: other lanes' tools walk
  those, and a lane's `target/` carries cargo's build-directory lock. (Evidence: `PORTFOLIO-EVIDENCE.md`, "A lane blocked on a lock".)
- **No unbounded wait on any lock.** The evidence file's lesson is that "still running" looks the same as deadlocked; every wait
  here has a limit and a message that names the holder.
- **Versions:** additive change; the PM allocates the number at gate time (CLAUDE.md §9). Nothing here is breaking.
- Dependencies stay at their latest versions (none added).

## What was measured (2026-10-03, this box, Windows 11, rustc/cargo 1.98.1)

All of this is from `scratchpad/cachexp.py` / `cachexp2.py`, which build `coderipper` itself (58 compilation units) with the same
command shape the checks use (`cargo build -p <pkg> --lib --message-format=json`, `RUSTFLAGS=--cap-lints=warn`). "Fresh" is cargo's own
`compiler-artifact.fresh` flag, not a timing inference.

| # | Setup | Result |
|---|---|---|
| A | Two builds, **separate** target dirs, different worktree paths (**today**) | 58/58 compiled both times; 50.3 s and 52.1 s |
| B | Same two worktree paths, **one shared** target dir | run 1 cold 54.3 s (58 compiled); run 2 **58/58 fresh, 3.9 s**; run 3 (third path) **58/58 fresh, 1.4 s** |
| C | Shared dir, the *other* flag set (`--cap-lints=warn --force-warn deprecated --force-warn unused_must_use`, which `unused-return-values` uses) | cold again, 41.3 s (58 compiled); repeated from another path: 58/58 fresh, 1.8 s |
| D | Back to the first flag set | still **58/58 fresh, 1.6 s**: the two flag sets coexist in one directory |
| E | A **new** worktree created *after* the artifacts exist (what a real run looks like), unchanged sources | 57 fresh, **1 compiled** (the package itself, because its checkout is newer than the artifact); 13.3 s |
| F | Same worktree, `lib.rs` rewritten now | 57 fresh, 1 compiled (the package); 17.1 s |
| G | **Hazard:** `lib.rs` content changed, mtime forced to 1 hour ago | the package's unit was reported **fresh**: cargo did **not** rebuild a changed file |
| H | **In-repo path dependency** (`dep` is a sibling workspace member), new worktree made after run 1 (`cachexp3.out`) | `dep` **COMPILED** again, and `app` too: an in-repo path crate is rebuilt in every new worktree. (Found by the audit; reproduced here.) |
| I | **Out-of-repo path dependency** (absolute path), same setup | `dep` **fresh**, only `app` compiled. A *relative* out-of-repo path does not resolve in the throwaway checkout at all (rc 101): an existing limitation, noted below |
| J | **Freshness is mtime-driven, not path-driven** (`cachexp4.out`): a NEW worktree path, every source file's mtime forced to one hour before the artifact | `dep` and `app` both **fresh** (an in-repo crate included). Same path reused with CHANGED content and an old mtime: still **fresh** (stale) |
| K | **Session checkout** (`cachexp5.out`), chain a -> b -> c, one checkout, a member rewritten in place then restored with `git checkout HEAD -- <member>` (mtime = now) | dependents-first: a: 3 units compiled; b: 1 (c **fresh**); c: 1 = **5** units, versus **6** when every member gets a new checkout (3+2+1). Dependencies-first order: also 5 and still correct |
| L | **Negative control**, same chain: member `c` rewritten, built, restored with an **old** mtime, then dependent `a` built | `a`'s build reports `c` **fresh**: the dependent was compiled against `c`'s *rewritten* artifact while `c`'s source is original. This is the stale hit, and why the restore must stamp **now**, never the old time |
| — | Size of the shared directory after A-G's shared runs | 533 MB for a 58-unit crate, debug profile, two flag sets |

Evidence files, beside the scratch scripts: A-D `cachexp.out`; E-G `cachexp2.out`; H-I `cachexp3.out`; J `cachexp4.out`; K-L `cachexp5.out`; Task 0 `t0.out` (fuel-core, deleted with its scratch directory; the numbers are in the table below) and `t0_vulkan.out`. The R4-B1 cross-checkout interleave has a script (`aud4/cross.py`) and **no saved output yet**; it is re-run and saved in Task H.

What this establishes, and what it does not:
- **Reuse across different worktree paths works for dependencies that are not in the repository** (B, E, I): such units do not
  depend on where the throwaway checkout is. Rows B-D are the *best case*: their worktrees were created before the first build,
  so no source file was newer than an artifact. A real run always creates its worktree *after* the artifacts exist, which is
  rows E-F and H.
- **Cargo decides "fresh" by source mtime (inferred: row G shows it for one worktree, and E/H are consistent with it, but a path-based cause was not separately excluded), and a new checkout's files are all newer than the artifacts** (E, H): so the package under
  analysis, and every in-repo path crate it depends on, is recompiled every run. That is correct for the rewritten package and
  wasted work for its siblings (see "Where the cache does not help").
- **Two flag sets double the *cold* cost once, not per run** (C, D): `unused-return-values` is the only check with extra flags.
- **G is the one correctness risk.** Cargo decides "changed" by file mtime. `RewrittenWorktree` writes every rewritten file at run
  time, so its mtime is "now" and F is what happens; G would only occur if a rewrite preserved an old mtime. A new
  worktree's checkout mtime is also "now", so the hazard cannot arise from checking out a different commit either; it would take
  code that sets an mtime *older than the previous artifact* on changed content. A test pins this (Task 2); its sabotage is exactly
  that: after the rewrite, set the rewritten file's mtime one hour into the past, and the test must then fail (G shows cargo
  serves the stale unit).
- **Not measured:** a large crate (fuel), and the share of fuel's compile time that is registry/git versus in-repo path crates.
  533 MB / 58 units is the only size datum; the plan states no fuel size or time. Task 0 measures the split first; Task 6 measures the result. `--force-warn` in the second flag set is not itself a cost driver; that cold build is just a second
  full dependency build.
- Long paths are enabled on this box (`LongPathsEnabled=1`) but the key is still kept short (12 hex digits per level), because
  GitHub's Windows runners and other users' machines may not have them.

## Task 0 results (2026-10-03, measured once each, machine at 72-95% CPU, `-j 8`, below-normal priority)

Same method for both: `cargo build -p <member> --lib --timings`, shared target, the second build from a new worktree made after the first. The numbers are
`t0.out` (fuel-core) and `t0_vulkan.out` (fuel-vulkan-backend), kept beside the scratch scripts; fuel clone at `d1bfe127`.

| member (siblings, closure) | cold wall | second worktree wall | unit-seconds external / in-repo (cold) | second run: in-repo compiled | of which the member itself |
|---|---|---|---|---|---|
| `fuel-core` (16 siblings, 336 pkgs) | 233.7 s | 89.7 s (-62%) | 1491 s (92.8%) / 116 s (7.2%) | 14 units, 113 unit-s | 9.6 s; siblings ~91% |
| `fuel-vulkan-backend` (6 siblings, 192 pkgs) | 207.6 s | 43.4 s (-79%) | 1183 s (96.6%) / 42 s (3.4%) | 7 units, 42 unit-s | 9.9 s; siblings ~76% |

The sibling counts per member are **bimodal** (cargo metadata, no build): 10 members have 0, 12 have 1-7, and 21 have 10 or more (fuel-core 16;
fuel-model-llama/phi, fuel-nn, fuel-datasets 17-19; fuel, fuel-inference, -parallel, -onnx, -examples, -training 20-22). The median (6) understates
the heavy half. Sum of siblings over all 43 members: 372.

**Estimates, not measurements** (a two-point linear model, ~5-6 s of sibling rebuild per sibling per run, each figure a *sum of unit-seconds*, so a wall-time
upper bound only: `-j 8` ran fuel-core's 113 in-repo unit-seconds in 89.7 s of wall time): the plain cache with a per-member loop costs roughly 35 min of
sibling rebuilds per check (372 x ~5.7 s) plus ~7 min of members' own compiles, so up to about 2 hours for three checks; with no cache up to about 7 hours
(43 x 3 x ~200 s cold); with the session checkout the siblings compile **once per distinct flag set and target kind** (at least two: the
`--cap-lints=warn` set, and the `unused-return-values` set with `--force-warn`, row C; `reachability` builds `--lib` while the others build `--all-targets`, which
may or may not unify features: unmeasured) plus 129 x ~10 s own compiles. That is a few cold sibling builds, not one, so the earlier "25-30 minutes" was too
optimistic and is withdrawn. Task 7 re-measures on the first real `--workspace` run and the numbers replace these.

## Where the cache does not help (and what was considered)

- **In-repo path crates are rebuilt per run** (row H). For `--workspace` this means the siblings a member depends on are compiled again
  for every (member, check) pair, because each pair builds in its own new worktree. Registry/git dependencies, which in most
  workspaces are the bulk of the compile time, are not. *Whether that holds for fuel is unmeasured; Task 0 decides whether PR 2 is
  worth building in this form.*
- **In-repo rebuilds do not grow the cache** (audit, from the fingerprint directory after row H): a rebuilt in-repo crate overwrites its own
  unit in place; it does not add one per worktree. So the size cap is about external dependencies and flag sets, not about run count.
- **Rejected: normalising mtimes** (set every unrewritten file's mtime to a fixed old value so siblings look fresh). It is unsafe:
  cargo would then treat changed content as fresh whenever its timestamp is older than an artifact, e.g. build commit X, switch
  to an older commit, switch back. Rows G, J and L are that failure. A content-based freshness check (`-Zchecksum-freshness`) is nightly-only.
- **Now planned for `--workspace` (PR 2, below): one session checkout per run**, rewriting each member in place and restoring it afterwards, so
  siblings keep their mtimes and stay fresh *within a run*. A single `--project` run is unchanged and keeps this limit. Task 0 showed
  siblings are 76-91% of what the cache leaves, which is what promoted this from "considered" to "planned".
- **Known limitation, unchanged:** a path dependency given as a *relative path outside the repository* does not resolve in the throwaway
  checkout (row I note). This affects `--project` today and is not made worse or better here.

## How the cache avoids the contention we measured

The measured stalls (`PORTFOLIO-EVIDENCE.md`) are two cargo locks: `target/<profile>/.cargo-lock` (per build directory) and
`~/.cargo/.package-cache` (global). Today CodeRipper sidesteps the first by building in a fresh temp directory every time, and pays a
full rebuild for it. The cache keeps that property where it matters:

1. **It is never a lane's `target/`**, so CodeRipper cannot hold or wait on a lane's build lock, and a lane's `cargo build` cannot
   wait on CodeRipper's.
2. **It is per repository**, so two repositories never share a lock. Contention is only possible between two CodeRipper runs on the
   *same* repository checkout.
3. **CodeRipper takes its own lock first, with a bound** (`CODERIPPER_CACHE_WAIT_SECS`, default 5, proposed): if another run holds
   it, this run does not queue inside cargo's silent "Blocking waiting for file lock"; it prints
   `cache busy (held since <time>, pid <n>): building without the cache` and builds in a throwaway target directory, i.e. today's
   behaviour. A busy cache costs time, never correctness and never a hang.
4. **Not solved by this plan:** the global `~/.cargo/.package-cache` lock. A fresh target directory never helped with it either (the
   evidence file says so). It is held by any `cargo` invocation that resolves or fetches. This plan does not touch it; whether a
   `--offline`/`--locked` build is safe to default to is a separate, measured question (listed under "Not in scope").

## Decisions for the PM

Each has a recommendation; none blocks the plan being approved; the task each one blocks is named.

1. **Cache size cap.** Recommend **20 GB** default, `CODERIPPER_CACHE_MAX_GB` to change, pruned least-recently-used per repository
   directory automatically when a run starts (Task 3). 533 MB per 58-unit crate suggests a fuel-sized workspace could be several GB;
   the real number is Task 6's. *Blocks Task 3 (a default can ship and be changed).*
2. **Lock wait.** Recommend **5 s**, then build uncached with a message (above). *Blocks nothing: Task 1 takes the wait as a parameter; this only sets the default.*
3. **Allowlist in workspace mode.** Recommend **each member reads its own `<member>/.coderipper.toml`, exactly as `--project <member>` does
   today; repository-unit checks read the workspace root's.** It makes `--workspace` equal to "every `--project` run", which can be
   compared against the 102 reachability / 2 unused-return-values findings already recorded for `fuel-core`. The alternative (one root
   file for the whole workspace) is a new semantics and needs the `project` key of an entry to mean a package. *Blocks Task 5.*
4. **Member label.** Today `reachability`, `unused-parameters`, `unused-return-values` and `version-consistency` set `Finding.project` to
   the *directory name* of the project (`ci-protection-presence` uses the GitHub repository name instead). Two members can share a
   directory name (`a/util`, `b/util`). Recommend a new optional `Finding.member: Option<String>` (the cargo *package* name, set by the host
   in workspace mode only), printed as the label; `project` is unchanged. Additive; no consumer of `project` changes, but it
   touches about two dozen `Finding { .. }` literals in the crate and its tests (`grep -rn "Finding {" src tests` finds 24-26 lines depending on what is counted as a literal; the compiler lists the exact ones) (mechanical, same commit; the field is `#[serde(default)]`).
   *Blocks Task 5.*
5. **Cache key = which repository.** Recommend the canonical **git common directory** of the project (so each checkout has its
   own cache, and lanes using different checkouts never share a lock). The alternative, the `origin` URL, shares one cache across
   checkouts of the same repository, which saves disk and reintroduces cross-checkout lock contention. *Blocks Task 1 (it fixes what the key hashes).*
6. **PR shape.** Recommend **two PRs**, cache first. Both additive.

### Decisions for PR 2 (revision 5; recommendations, none approved yet)

7. **How the checks reach the session.** Recommend a **thread-local active session** that `RewrittenWorktree::create_with` consults. "The same source repository" means the
   **same canonical `git rev-parse --path-format=absolute --git-common-dir`** (the identity the build cache already keys by), never a path comparison (Windows `\\?\` prefixes
   and case make equal paths compare unequal). Alternatives: a field on `CheckContext` (18 struct-literal sites), or a process-wide static (leaks between tests in one
   binary). Audit-confirmed sufficient: only the three building checks call `create_with`, each exactly once per run with one guard, none reads or writes outside the member
   directory, and the crate has no threads. *Blocks Task 5.*
8. **Order of members: none.** Plain sorted-by-path order. Revision 4 proposed dependents first; row K measured **both orders at 5 units** (so the benefit was unmeasured),
   and it needs an edge set, and fuel has **two cyclic groups when dev-dependencies are included (3 and 7 members; both audits and my own strongly-connected-components count over `cargo metadata --no-deps` agree; the "8 cycles" quoted earlier was a different, unreproduced count and is withdrawn)** (e.g. `fuel-aocl-cpu-backend -> fuel-dispatch -> fuel-hardware -> fuel-vulkan-backend -> fuel-core -> fuel-aocl-cpu-backend`)
   and none with normal+build edges only. An ordering would have to specify which edges count and what a cycle does; with no ordering the question does not arise. *Blocks nothing.*
9. **Where cargo builds in a session.** The persistent cache's directory when the session got the cache; otherwise a **session-owned temporary directory outside the checkout**.
   The path reaches `build_with` through the thread-local (`session::target_dir() -> Option<PathBuf>`). **In a session `build_with` does not call `build_cache::acquire` at all** (R5-N1:
   the session already holds the lock through its own handle, and a second `try_lock` handle in the same process answers `WouldBlock`, which would cost 5 s and a "cache busy" note on every build and
   then fall back to `<checkout>/target`); it also skips PR 1.1's freshen (a freshen would defeat all sibling reuse), and reports its cache directory to `build_stats` from the session. A busy or unusable cache must never
   leave cargo building in `<checkout>/target`. *Blocks Task 5.*
10. **The session holds the cache lock for its whole run** (R4-B1). Recommend yes. A session lives for hours, its sources are older than any artifact another run writes
    in between, and unit hashes are path-independent, so an overlapping CodeRipper run (even a plain `--project` run) that builds a rewritten member into the shared cache makes
    the session's next build serve that artifact as fresh (measured by the auditor; same mechanism as row L). With the lock held, an overlapping run waits its bounded 5 s and
    then builds uncached with a note: correct, slower. The session itself, if it cannot get the lock within its bound, runs with a private temp target (no persistent cache).
    The cost is that **no other CodeRipper run on the same repository uses the persistent cache while a `--workspace` run is in progress.** *Blocks Task 5.*

## File Structure

| File | Responsibility |
|---|---|
| `src/build_cache.rs` (new) | where the cache lives; the key; the bounded lock; pruning; `CacheStats`. No cargo knowledge. |
| `src/cargo_json.rs` | `build_with` asks `build_cache` for a target directory and sets `CARGO_TARGET_DIR`; counts `compiler-artifact` `fresh` flags into `BuildOutput` and into the process-wide `build_stats`. |
| `src/worktree.rs` | `RewrittenWorktree` exposes its `source_repo` (done in PR 1). PR 2: it can be backed by a session checkout instead of a throwaway one. |
| `src/session.rs` (new, PR 2) | the session checkout: one `git worktree` per `--workspace` run, the thread-local active session, in-place rewrite of a member, restore, poisoning. |
| `src/check.rs` | `Unit { Package, Repository }` and `Check::unit()` (default `Package`). |
| `src/lib.rs` | `run_workspace(...)`: enumerate members, run repository-unit checks once, package-unit checks per member, isolate errors. |
| `src/package.rs` | `workspace_members(dir)` over the existing `metadata()`. |
| `src/finding.rs` | optional `member`. |
| `src/main.rs` | `--workspace` on `fast`, `sweep`, `check`; `cache status` / `cache prune`. |
| `README.md`, spec §1 | the "Workspaces" section loses "not supported yet"; a "Build cache" section. |

---

# Task 0 — measure before building (no code, one afternoon)

On this box, on the real `fuel` clone, with `cargo build --message-format=json` (the same shape the checks use), report:
- the number of `compiler-artifact` units and wall time for `-p fuel-core --lib` cold;
- the split of those units into **registry/git** versus **in-repo path crates** (by `package_id` source), and the share of the cold
  wall time each accounts for (sum of per-unit time is not available from cargo's JSON; use `cargo build --timings` for the per-unit
  durations and report which one was used);
- whether a second build in a new worktree recompiles the in-repo path crates (row H predicts yes) and how long that takes.

**Gate:** if in-repo path crates are the majority of the cold time, PR 2 as designed buys little and the "session worktree" option in
"Where the cache does not help" must be designed first; PR 1 is still worth shipping for the registry/git share. The measurement goes
to the PM as a `[FINDING]` before any code.

# PR 1 — the build cache

### Task 1: `build_cache` — location, key, lock

**Files:** Create `src/build_cache.rs`; modify `src/lib.rs` (`mod build_cache;`).

**Interfaces:**
- Produces `pub struct CacheConfig { pub root: PathBuf, pub wait: Duration, pub max_bytes: u64 }` and
  `pub fn set_cache_config(CacheConfig)` backed by a process-wide `OnceLock`. **The default is no cache.** Library users, the in-crate
  unit tests and any test that does not call `set_cache_config` build exactly as today, so no test can write to a developer's
  real cache by accident. `main` sets it from the environment (below); the integration tests set it once per test binary, in
  `tests/common`, to a process-lifetime temporary directory (different fixtures are different repositories, hence different keys;
  there is no per-test environment variable and no race).
- Produces `pub(crate) struct CacheDir { pub path: PathBuf, _lock: CacheLock }` and
  `pub(crate) fn acquire(source_repo: &Path) -> CacheChoice` where
  `enum CacheChoice { Shared(CacheDir), Throwaway { why: Option<String> } }`. `Throwaway` means *set nothing*: cargo uses the worktree's own
  `target/`, which is deleted with the worktree, i.e. today's behaviour. `why` is `None` when no cache is configured and `Some(..)`
  when one was configured but could not be used (busy, unwritable); the caller prints a `Some` once per run.
- `acquire_in(config: &CacheConfig, toolchain: &str, source_repo: &Path) -> CacheChoice` is the testable core (no globals, no env).
- Produces the **pure** `pub fn config_from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<CacheConfig>`: `CODERIPPER_CACHE_DIR`,
  else `%LOCALAPPDATA%\coderipper\build`, else `$XDG_CACHE_HOME/coderipper/build`, else `$HOME/.cache/coderipper/build`; also
  `CODERIPPER_CACHE_WAIT_SECS` (default 5) and `CODERIPPER_CACHE_MAX_GB` (default 20, decision 1). Taking the lookup as a parameter
  is what lets the tests cover every branch without touching the process environment. `None` if no root can be derived (then no cache).
- The first `acquire_in` that creates a root writes the marker file `<root>/.coderipper-cache` (Task 3's prune refuses a directory without it).
- Produces `fn key(toolchain: &str, repo_identity: &Path) -> (String, String)`: two 12-hex-digit hashes (toolchain id; repository). Pure, so it is unit-testable.
- Consumes nothing from earlier tasks.

The toolchain id is the output of `rustc -vV` run in the project directory (so a `rust-toolchain.toml` in the project is honoured).
The repository identity is the canonical `git rev-parse --git-common-dir` **of the source repository**
(decision 5); the callers pass `RewrittenWorktree::source_repo()`, never the throwaway checkout (a checkout's git-common-dir
is the source's, but its path is not, and `build_with` is given the checkout's root). Hash: `std::hash::DefaultHasher` is not stable across Rust
versions, so use a small fixed FNV-1a over bytes written here (about ten lines; stability matters because the key names a directory
that must survive a toolchain upgrade of CodeRipper itself).

- [ ] **Step 1: write the failing tests** (unit tests in `build_cache.rs`)
  - `the_key_is_stable_and_short`: `key("rustc 1.98.1 ...", "/r")` equals a literal pinned in the test; both halves are exactly 12 hex digits. *Fails: no function.*
  - `two_repositories_and_two_toolchains_get_different_directories`: four combinations, four distinct paths.
  - `the_config_honours_the_override_and_the_default_is_never_under_a_project`: `config_from_env` with a fake lookup: the override wins; the Windows/XDG/HOME defaults are derived in that order; none contains a `target` component, and none is under `C:\Projects` unless the user says so explicitly.
  - `a_new_root_gets_the_marker`: after `acquire_in` on an empty root, `.coderipper-cache` exists.
  - `a_held_lock_makes_the_next_run_throwaway_after_the_wait_not_a_hang`: take the lock on a temp cache dir (the holder writes its sidecar), call `acquire_in` with `wait = 300ms`, assert `Throwaway`, the reason names "cache busy" **and the holder's pid read from the sidecar**, and the call returned in under 3 s. (This is the test that fails if the pid were stored in the locked file, on Windows.) *This is the test for the 11-hour failure mode.*
  - `a_released_lock_is_taken_by_the_next_run`: drop the first `CacheDir`, `acquire` returns `Shared`.
  - `an_unwritable_root_is_throwaway_with_a_reason_not_an_error`: root set to a path under a regular file.
  No test in this task reads or writes the process environment: `config_from_env` takes a lookup closure, `acquire_in` takes the config.
- [ ] **Step 2: run, see them fail** (`cargo test build_cache`): *Expected: compile errors for the missing items; then, once stubbed, assertion failures.*
- [ ] **Step 3: implement** `cache_root`, `key`, `acquire_in` (create the directory, open `<dir>/.coderipper.lock`, `File::try_lock` in a loop to the deadline, then write `pid` and a timestamp to a **separate, never-locked sidecar** `<dir>/.coderipper.holder` for the holder message, and `touch` the lock file for LRU). **Why a sidecar:** `File::try_lock` locks the whole file, and on Windows a second handle cannot even *read* a file another handle has locked (probed by the audit: `read: Err(os error 33, another process has locked a portion of the file)`), so the waiter could not read the holder's pid from the file it is blocked on. A stale sidecar (holder crashed) is harmless: the message says "last holder", and the lock itself is the authority., and `Drop` (unlock).
- [ ] **Step 4: run, see them pass.**
- [ ] **Step 5: commit** `feat: build cache directory, key and bounded lock`.

### Task 2: use it in the builds; count fresh vs compiled; the mtime test

**Files:** `src/cargo_json.rs`; tests in `src/cargo_json.rs` and `tests/build_cache.rs` (new).

**Interfaces:**
- Consumes `build_cache::acquire`, `RewrittenWorktree::source_repo()` (new accessor, Task 2 adds it).
- `build_all_targets` / `build_lib_only` gain `cache_source: Option<&Path>` (the source repository; `None` means "throwaway, as today"). The three checks, which hold a `RewrittenWorktree`, pass `Some(wt.source_repo())`. `reachability` reaches the build through `collect_dead_code_in(worktree_root, targets)`, which gains the same parameter; its 7 unit-test callers in `diagnostics.rs` pass `None` (they hold a bare temp directory, no source repository). Signature changes inside the crate only; no public API.
- Produces `BuildOutput { ..., pub units_fresh: u32, pub units_compiled: u32 }` (additive), counted from `compiler-artifact.fresh`.
- **The route from a build to the user** (rev-3 audit B2): `Check::run` is unchanged, so builds report into a process-wide
  `build_stats` accumulator (`record(units_fresh, units_compiled)`, `note(String)`, `take() -> BuildStats`), which `main` prints
  once, at the end of every run, to **stderr**: `cache: <dir or "off"> — <N> units fresh, <M> compiled` plus any `note` (e.g. `cache busy
  (held since <time>, pid <n>): building without the cache`). A global is the honest choice here: the alternative is a new field on
  `CheckContext`, which is built as a struct literal at 18 sites (`grep -rn "CheckContext {" src tests`). `main` prints it from one wrapper around `run_and_report`, so the early "no issues found" return and the error exit both
  print it. Tests read the line from the CLI's stderr; none calls an internal function to learn what a check did.
- `build_with` sets `CARGO_TARGET_DIR` only when the choice is `Shared`; `Throwaway` sets nothing (cargo's own worktree-local `target/`, as today).

- [ ] **Step 1: failing tests** (`tests/build_cache.rs`, fixtures are tempdir cargo packages with a **path dependency**, so they need no network):
  The CLI tests use `assert_cmd` and set `CODERIPPER_CACHE_DIR` **on the child process only** (not the test process), so there is no
  environment race. The fixture's dependency is a path crate **outside** the fixture's git repository, given by an **absolute** path
  (rows H, I: only that kind of unit is reusable, and a relative out-of-repo path does not resolve in the throwaway checkout).
  - `a_second_run_reuses_an_out_of_repo_dependency`: fixture repo `app` whose manifest names `dep` (a sibling temp dir, outside the repo) by absolute path; `check unused-parameters --project app` twice. Run 1's stderr line reports the dependency compiled, run 2's reports it fresh (the line's counts differ by exactly the dependency's unit). *Fails today: no cache, so no `cache:` line and both runs compile `dep`.*
  - `an_in_repo_path_crate_is_still_recompiled_and_the_findings_are_still_right`: same, but `dep` is a sibling member inside the repository. Pins the measured limit (row H): run 2 compiles it again, and the run's findings are unchanged. If a later change makes siblings reusable, this test is the one that is consciously updated.
  - `a_changed_rewrite_is_never_served_from_the_cache` (the G hazard): two runs on a fixture where the second commit changes the package's source so that the set of findings changes; run 2's findings reflect the new source. **Sabotage, applied and asserted to have applied (CLAUDE.md §6):** in `RewrittenWorktree::create_with`, after each rewritten file is written, set its mtime one hour into the past (`File::set_modified`); the test must then fail. Recorded in the PR body.
  - `an_unusable_cache_gives_identical_findings_and_one_note`: `CODERIPPER_CACHE_DIR` under a regular file: identical findings to the cache-off run, exit 0, and the stderr note appears exactly once.
  - `a_busy_cache_is_not_waited_on`: the test first does one ordinary **cached priming run** (so the repository's directory exists), finds its lock file with `coderipper cache status` (which prints each repository directory), takes `File::try_lock` on `<that dir>/.coderipper.lock` itself, then runs the CLI with `CODERIPPER_CACHE_WAIT_SECS=1`: it must finish, with the same findings as an uncached run, and stderr contains `cache busy`. Forced overlap, deterministic; the earlier idea of racing two processes is dropped because it passes trivially when they do not overlap.
  - Existing integration tests (`tests/reachability.rs`, `unused_*`, `workspace`, `cli`) call the checks in-process: `tests/common` gains `use_temp_cache()`, called once per test binary (`std::sync::Once`), which calls `set_cache_config` with a process-lifetime temp directory. Without it they run uncached, exactly as today; the in-crate unit tests never enable the cache.
- [ ] **Step 2: run, see them fail.** [ ] **Step 3: implement.** [ ] **Step 4: pass, then the whole suite.** [ ] **Step 5: commit** `feat: build with a persistent per-repository target directory`.

### Task 3: size cap, `cache status`, `cache prune`

**Files:** `src/build_cache.rs`, `src/main.rs`.

- Per repository directory the LRU key is the mtime of `.coderipper.lock`. `prune(root, max_bytes)` deletes whole **unlocked** repository directories, oldest first, until the total is under the cap; a locked directory is never touched. Never deletes anything outside `root` (the function takes the root and refuses a path that does not contain the marker file `.coderipper-cache`, written when the root is created: a guard against `CODERIPPER_CACHE_DIR=C:\Projects`).
- `coderipper cache status` prints root, per-repository size and last use; `coderipper cache prune [--max-gb N]`.
- **Automatic:** the first `acquire_in` in a process calls `prune_to_cap` before taking its own directory's lock, so the cap in decision 1 is enforced without anyone running a command. It skips locked directories and never prunes the one being acquired.
- [ ] Tests (fail first): `acquire_prunes_the_oldest_unlocked_directory_when_over_the_cap` (cap set below two small fake directories' total; the older one is gone, the newer and the acquired one remain); `prune_removes_the_oldest_unlocked_directories_until_under_the_cap`; `prune_never_removes_a_locked_directory`; `prune_refuses_a_directory_without_the_marker` (the `C:\Projects` guard); `status_lists_sizes`. Sizes use small files in a temp root, no real builds.
- [ ] Commit `feat: cache size cap and cache status/prune`.

---

# PR 1.1 — freshen the checkout after taking the lock (found by the audit; reasoned, not yet reproduced on the merged code)

**The exposure.** In the merged cache each check creates its checkout, rewrites it and injects the sentinel (so its files are stamped T1), and only then does `build_with`
take the cache lock, waiting up to 5 s. If another CodeRipper run finishes a build of the same unit slot at T2 > T1 while this one waits (or before it gets there), cargo finds this
run's files older than that artifact and calls the unit fresh: this run analyses the *other run's* compile. The slot is shared across checks (reachability `--lib` and
`unused-parameters` `--all-targets` use the same `RUSTFLAGS` and an identical lib unit) and across checkouts of one clone, so the other run's rewrite, or even its commit, can differ.
The mechanism was measured by the auditor on a shared target (`aud4/cross.py`: the older checkout's build reports the other run's unit `fresh`, and the artifact contains the other run's rewritten symbol; the script's output was not saved: Task H re-runs it). That
the *merged code* hits it is **reasoned from the code order, not reproduced**, and needs two CodeRipper runs on one repository overlapping within seconds; it is rare and silent, which is the class of failure this project guards against.

**The fix** (costs nothing: a fresh checkout already has every sibling rebuilt per run, row H): in `build_with`, right after the lock is taken and before cargo runs, set the
mtime of every non-`.git` file of the checkout to now. A session (PR 2) does not use this, because it holds the lock for the whole run (decision 10).

### Task H: failing test first, then the fix (needs cargo; after the build-quiet window)

**Interfaces:**
- `build_cache::with_config<T>(config: CacheConfig, f: impl FnOnce() -> T) -> T` (new, `pub(crate)`): a **thread-local override** of the cache configuration, consulted by `acquire` before the process-wide
  `OnceLock` (R5-N2: the global can be set once per process and in-crate tests run in parallel threads, so a test cannot use it; the default stays "no cache", so no in-crate test touches a real cache). Production code never calls it.
- `freshen_checkout(toplevel: &Path) -> anyhow::Result<()>` (new): walk **recursively** from the checkout's top level (`git rev-parse --show-toplevel` run in the package directory, because `build_with` is given the package root);
  skip `.git` (directory or file); do not follow symlinks (`symlink_metadata`; a symlink is skipped, never opened, so a link cannot stamp a file outside the checkout); open each file with write access and `set_modified(now)`,
  clearing and restoring a read-only attribute where needed. **Error policy (R5-S-c): if any file cannot be stamped, the build does not use the persistent cache** (`Throwaway` with a note naming the file): correct, slower, and never the silent hazard.

- [ ] **Failing test first:** `a_build_after_another_run_wrote_the_same_unit_is_never_served_that_runs_artifact`, in-crate, inside `build_cache::with_config` (no CLI): checkouts A and B of one repository (B created after A);
  in A rewrite member `c` (adds `fn rewritten_by_a`) and build it into the cache; then, **without re-creating B**, build `c` in B with a different rewrite from B's older files. Assert B's `c` units are **compiled** and that no `c`
  artifact contains `rewritten_by_a`. *Fails today: B's unit is fresh.* Also `the_same_slot_is_not_shared_across_checks_unsafely`: the cross-**check** case the PM asked for (a `--lib` build with one rewrite, then an `--all-targets` build with
  another, same flags). **Sabotage:** remove the freshen call; both tests must fail again.
- [ ] `freshen_checkout_skips_symlinks_and_git_and_reports_an_unstampable_file`, and `an_unstampable_file_makes_the_build_uncached_with_a_note`.
- [ ] Re-run `aud4/cross.py`'s mechanism once and **save its output** next to the plan evidence (currently a script only).
- [ ] Commit `fix: a build never reuses a unit another run wrote after this checkout was made`; the version is allocated by the PM (0.2.9 reserved).

# PR 2 — `--workspace`, with sibling reuse (revision 5)

## Design: the session checkout

Today every check run makes its own throwaway checkout (`RewrittenWorktree`), rewrites the analysed package, builds, and deletes the checkout. A new checkout
makes every file newer than every cached artifact (row E), so the package's *siblings* (in-repo path crates) are recompiled in every run (row H). For
`--workspace` the fix is to stop making a new checkout per (member, check):

1. **One checkout per `--workspace` run**, at HEAD, created by `run_workspace` and removed when it ends (also on error).
2. **Analysing a member** (a check's `create_with` call) rewrites that member's `src/` **in place** in the session checkout, exactly as today's rewrite does, and returns a guard.
   Audit-verified: the three sentinel injectors and all the rewriters only append to or overwrite **existing tracked files under the member's `src/`**; none creates a file or writes outside `src/`.
3. **When the guard drops** (success, error or panic) the member is restored with `git checkout HEAD -- ':(literal)<member dir>'` (a literal pathspec, so a member whose name has glob characters is safe).
   **Both** the exit code of the checkout and a following `git status --porcelain --untracked-files=no` over the *whole* checkout are checked; any tracked modification **other than the workspace root's `Cargo.lock`** is a restore failure.
   (R5-S-a: `cargo build` without `--locked` rewrites a committed `Cargo.lock` that is out of step with the manifests, which this portfolio's "the version changes with every push" rule makes plausible; the restore is member-scoped, so a root `Cargo.lock`
   is never restored and would otherwise poison the session at the first member. fuel's and coderipper's lock files match their manifests today, so it would not fire on them now. The status check runs after every (member, check), about 129 times on fuel
   over ~1800 tracked files; its cost is unmeasured and Task 7 reports it.) There is **no `git clean`**:
   it is unnecessary (nothing creates files) and was measured to be harmful (below). Verified: only the files that differ get rewritten, so **only they get a new mtime, which is "now", after the build
   ended**; untouched files keep their mtimes.
4. **Never restore an old mtime** (row L; the stale artifact was confirmed to contain the rewritten symbol). An artifact compiled from the rewritten source is fresh for any file whose mtime is older than it;
   restoring the original bytes with the original mtime would make a dependent of that member link against the rewritten artifact. Stamping "now" makes the next build of that member recompile it.
   (This is also what a restore through a timestamp-preserving copy would break: Review Focus 7.)
5. **A failed restore is retried briefly** (a transient lock is real: the auditor reproduced `unable to unlink old` while a file was held open, and an `index.lock` in the worktree's gitdir; both exit non-zero), up to a short bound.
   **If it still fails the session is poisoned**: `Session::is_poisoned()` becomes true, the host loop checks it after every member and stops, reporting the remaining members as not analysed. Poison is a session flag,
   not an error type: `run_checks_over` flattens check errors to strings, so a typed error would be lost on the way out. Semantics (R5-S-e): findings a check computed **before** its guard dropped and poisoned stay valid and are
   reported (the restore failed after the analysis); a later check's `create_with` in the poisoned session returns an error, and **the poison message replaces it** as the member's error (it is the cause); nothing partly built is ever reported as a finding.
6. **Siblings are compiled once per session per distinct flag set and target kind** (at least two flag sets, row C), then reused, because nothing touches their files. Each analysed (member, check) pays that member's
   own compile (the rewritten source is new). Measured on a 3-crate chain (row K): 5 units instead of 6; the saving grows with the graph, and the fuel numbers are Task 7's.
7. **Members are analysed in sorted order** (decision 8): no dependency ordering.
8. **The session holds the cache lock for its whole run** (decision 10) and **owns its cargo target directory** (decision 9): the persistent cache's directory if it got the lock, otherwise a temp directory outside the checkout.
   Why the lock: a concurrent run could otherwise write a rewritten artifact into the shared slot between two of the session's builds (audit B1).
9. **A single `--project` run is unchanged**: no session is active, `create_with` makes its own checkout as before (plus PR 1.1's freshen).

**Dev-dependency cycles** (R4-B3, executed by the auditor on a fixture and counted on fuel: two cyclic groups of 3 and 7 members with dev edges, none without): cargo accepts a cycle through a dev-dependency, and `cargo build -p a --all-targets` builds the cycle's
members. Members a (dev-depends on b), b (depends on a), c (depends on b), analysed with in-place rewrite and restore in order c, b, a: all builds succeeded, no final rlib contained a rewritten symbol, and a pristine
`-p c --lib` afterwards recompiled a, b and c once and then reported all of them fresh. So **correctness holds under a cycle; the cost is one recompile of the cycle's members**. fuel has two such groups (3 and 7 members) with dev edges and none with normal edges.

**What this does not fix:** a *new session* starts with new file mtimes, so the siblings compile once per `--workspace` run (not once per machine). Making them survive between runs needs content-keyed freshness,
which cargo does not offer on stable. Also not examined: filesystems with 1-2 s timestamp granularity (FAT/exFAT), where "restore stamps now" relies on strict ordering; NTFS, ext4 and APFS have sub-millisecond granularity, but the rule still needs the clock to advance between the build and the restore, which a
fast restore after a sub-millisecond build could in principle violate on a coarse clock; the post-restore stamp therefore adds one millisecond if its mtime does not exceed the build's end time (a one-line guard, tested).

### Task 4: members and `Check::unit()`

**Files:** `src/package.rs`, `src/check.rs`, the five checks, `src/lib.rs`.

- `package::workspace_members(dir) -> anyhow::Result<(PathBuf, Vec<Package>)>`: `(workspace_root, members)` from the existing `metadata()` (`cargo metadata --no-deps` lists the workspace's packages; directories under `[workspace] exclude` are not members and are not listed: verified, and README will say so). Sorted by path for a deterministic order.
- `Check::unit()`: `Package` for `reachability`, `unused-return-values`, `unused-parameters`; **`Repository` for `version-consistency` and `ci-protection-presence`** (the first already analyses a whole workspace and answers `Info` from a member; the second reads the GitHub settings of the repository and would repeat one finding per member).
- **Rev-3 audit B3:** `run_checks_over(checks, ctx, tier, only_check_id)` runs every check of the tier, so it cannot by itself run
  "package checks per member, repository checks once". It gains a fifth parameter `units: UnitFilter` (`Any` | `Only(Unit)`); `run_checks`
  passes `Any` and behaves as today, and `run_workspace` calls it with `Only(Repository)` once and `Only(Package)` per member. A named
  check (`check <id> --workspace`) is run only in the unit it declares.
- **Stale-entry dedupe (rev-3 audit S11):** `stale_findings` reports unknown-check entries regardless of which checks ran, and on a workspace whose root is
  itself a package, the repository-unit run and the root member's run load the same `.coderipper.toml`. `run_workspace` therefore
  keeps the set of allowlist files already judged and drops a second identical stale/unknown finding for the same file and entry.
- [ ] Failing tests: `a_package_unit_filter_skips_repository_checks_and_the_reverse` (drives `run_checks_over` with fakes);
  `a_root_package_workspace_reports_an_unknown_check_entry_once`; `members_of_a_virtual_workspace_are_listed_sorted`; `a_root_package_workspace_lists_the_root_too`; `an_excluded_directory_is_not_a_member` (verified: `cargo metadata --no-deps` on a workspace with `exclude = ["skip"]` lists only `a` and `b`; the README does not say this today, Task 5 adds it); `two_members_with_the_same_directory_name_are_both_listed`; `the_registered_checks_declare_their_unit` (pins the five).
- [ ] Commit `feat: workspace members and a per-check unit`.

### Task 5: the session checkout

**Files:** create `src/session.rs`; modify `src/worktree.rs` (`RewrittenWorktree` backing), `src/cargo_json.rs` (`build_with` consults the session; `BuildOutput.units`), `src/build_cache/mod.rs` (`git_common_dir` becomes `pub(crate)`; `acquire_for_session`), `src/lib.rs`.

**Interfaces:**
- `build_cache::acquire_for_session(source_repo: &Path) -> CacheChoice` is `acquire` with the lock kept by the caller for as long as it holds the returned `CacheDir`; `Session::open` calls it **once** and keeps the `CacheDir`
  (and so the lock) for the whole run. It also reads the toolchain id from `rustc -vV` in the source directory at open (a member with its own `rust-toolchain.toml` would share the root's key: rare, noted).
  `git_common_dir` is made `pub(crate)` so decision 7's "same repository" test and the cache key use one function.
- `pub(crate) struct Session` owns the checkout, the lock (`CacheDir`) or the temp target directory, and the poison flag as a `Cell<bool>` (`is_poisoned(&self)` takes `&self`). `Session::open(source_repo: &Path) -> anyhow::Result<Session>`.
- `pub(crate) fn with_session<T>(session: &Session, f: impl FnOnce() -> T) -> T` installs the session in a **thread-local** for the duration of `f` and removes it on exit (RAII, panic-safe).
- `pub(crate) fn target_dir() -> Option<PathBuf>` (thread-local): `Some` while a session is active; `build_with` then **does not call `acquire` and does not freshen**, sets `CARGO_TARGET_DIR` to it, and records its cache directory from the session (R5-N1).
- `RewrittenWorktree` keeps `pub root` and `source_repo()` **as plain fields outside the new enum** (the checks read `wt.root` directly), and gains a backing enum: `Throwaway { worktree_path, _scratch: TempDir }` or `Session(Rc<SessionInner>)`.
  Its `Drop` matches on the backing: `Throwaway` runs today's `git worktree remove --force` plus fallback; `Session` only restores the member. **The checkout is removed and the lock released in `SessionInner::drop`**, so a guard that outlives the loop
  keeps the checkout alive. "The same repository" is the canonical `git_common_dir` comparison (decision 7).
- `BuildOutput` gains `units: Vec<UnitReport>` with `UnitReport { package_id: String, target: String, kind: String, fresh: bool }` (R5-S-b: keyed by package id *and* target, because `--all-targets` gives a package several units: lib, test, bin). It is used **only by in-crate tests**; the CLI still prints the one aggregate line.
- `build_stats::record` takes the cache directory from the session when one is active.

**Failing tests first** (unit tests in `session.rs`, tempdir git repositories with tiny path-dependency crates; every cache test runs inside `build_cache::with_config`, Task H):
- `a_session_member_is_restored_byte_for_byte`: rewrite a member's files; drop the guard; `git status --porcelain --untracked-files=no` over the whole checkout is empty and the bytes equal HEAD.
- `restored_files_get_a_new_mtime_and_untouched_files_keep_theirs`.
- `a_restored_member_is_never_served_as_its_rewritten_self` (**the hazard test, in-crate, explicit order**; the CLI cannot produce it): chain `a -> b -> c`; inside `with_session`, analyse `c` (rewrite, `build_all_targets`, drop the guard), then `a`; assert from `BuildOutput.units` that every `c` unit in `a`'s build is **compiled**. **Sabotage, applied and asserted to have applied:** stamp an old mtime on restore (one hour back); the test must then fail (row L).
- `a_session_never_waits_on_its_own_lock` (R5-N1): in a session with the cache configured, N builds complete with **no** "cache busy" note and each in far less than the lock wait; **sabotage:** call `acquire` per build; the test must fail (a 5 s wait and a note).
- `a_failed_restore_is_retried_then_poisons_the_session`; `a_dirty_checkout_after_a_clean_looking_restore_poisons_the_session` (a check wrote a tracked file outside the member); `a_rewritten_root_cargo_lock_does_not_poison_the_session` (R5-S-a: a committed lock out of step with a bumped member version).
- `the_session_is_thread_local`; `without_a_session_create_with_is_unchanged` (existing `worktree.rs` tests untouched); `an_error_inside_a_check_still_restores_the_member`.
- `a_member_nested_in_another_member_is_restored_without_touching_the_outer_one`; `a_root_package_member_restores_without_deleting_a_non_ignored_target_or_other_members_untracked_files` (the R4-B4 measurement: this is why `git clean` is gone).
- `the_session_target_is_outside_the_checkout_when_the_cache_is_busy_or_unusable`: hold the cache lock from the test before `Session::open`; the session builds in its own temp target, never `<checkout>/target`, and says so once.
- `an_overlapping_run_cannot_poison_the_session` (R4-B1, with the arrangement pinned by R5-N3): **the other run rewrites and builds sibling `c`; the session then analyses dependent `a` without having touched `c`** (a unit the session itself just rewrote is compiled regardless, so
  it would pass without the lock). The other run runs **on another thread, outside `with_session`** (otherwise the thread-local would make it a session build too). Assert the other run waited its bound and built uncached with the busy note, and that
  the session's `a` build compiles `c` rather than serving the other run's artifact. **Sabotage:** release the session's lock between builds; the test must fail.
- [ ] Commit `feat: a session checkout for --workspace runs (rewrite in place, restore, never an old mtime, lock held for the run)`.

### Task 6: `run_workspace`, tagging, flag

**Files:** `src/lib.rs`, `src/finding.rs`, `src/main.rs`, `tests/workspace_run.rs` (new), `README.md`.

**Behaviour (each line has a test below):**
1. `--workspace --project <dir>` runs the workspace that contains `<dir>` (`<dir>` may be the root or a member).
2. Repository-unit checks run **once**, with `project_root = workspace root`, outside the session (they build nothing).
3. A session is opened; package-unit checks run once **per member**, in sorted order (decision 8), each through `run_checks_over(.., Only(Package))` with `project_root = member directory`, so allowlist loading and stale-entry findings are exactly those of `--project <member>`.
4. Every finding from a member run gets `member = Some(<package name>)`; the printed line leads with it.
5. An error in one member is reported as `<member>: <existing message>` and the loop **continues**, except that after every member the loop checks `Session::is_poisoned()` and stops, reporting the remaining members as not analysed; the process exits non-zero if any error occurred.
6. A virtual root is fine (never analysed as a package); a root package is a member like any other.
7. The run ends with one summary line: members run, members with errors, and the totals from the `build_stats` accumulator.

**Failing tests first** (`tests/workspace_run.rs`, tempdir git repos, `CODERIPPER_CACHE_DIR` on the child process only):
- `every_member_is_checked_and_findings_name_their_member`.
- `findings_equal_those_of_running_each_member_by_hand`: the finding sets of `--workspace` and of `--project a` plus `--project b` are equal (the property decision 3 buys; it is also the property that proves the session checkout changes no result).
- `a_member_that_does_not_compile_does_not_stop_the_others_and_the_run_fails`.
- `repository_checks_run_once_not_per_member`.
- `members_sharing_a_directory_name_are_told_apart`.
- `a_member_directory_given_to_workspace_runs_the_whole_workspace`.
- `a_dev_dependency_cycle_still_analyses_every_member` (R4-B3): `a` dev-depends on `b`, `b` depends on `a`; both are analysed and report, with no error.
- `members_are_analysed_in_sorted_order`: the CLI prints one stderr progress line per member as it starts (`coderipper: member <name> (i/n)`), which is also what a person watching a long run needs; the test asserts their order.
- `the_siblings_are_compiled_once_per_session` (**the payoff, deterministic**): the in-repo chain `a -> b -> c`; the total compiled units of one `--workspace` run (the CLI's `cache:` line) is **less than** the sum of three separate `--project` runs against the same fresh cache. A relative, aggregate assertion only: the CLI prints one aggregate line, so per-unit claims belong to Task 5's in-crate tests, and the per-check unit counts differ (two `RUSTFLAGS` sets, `--lib` versus `--all-targets`). *This replaces the revision-3 test that pinned the opposite for a per-member loop.*
- The in-repo pin test of PR 1 (`an_in_repo_path_crate_is_still_recompiled...`) stays, because `--project` mode is unchanged.
- CLI: `--workspace` without a Cargo workspace is an error that says so; `--workspace` with `check ci-protection-presence` runs once.
- [ ] Commit `feat: --workspace runs the project-scope checks over every member`.

### Task 7: acceptance on `fuel`, measured and reported (not a CI test)

Run on this box, on the real 43-member workspace in a throwaway clone (never the shared checkout), and **report the measurements; do not assert them in the plan**:
- `check unused-return-values --project fuel/fuel-core`: before (no cache), after (cold cache, then warm). Compare findings to the recorded **reachability 102 / unused-return-values 2** for `fuel-core` (acceptance: identical).
- `check reachability --workspace --project fuel`: total wall time cold and warm, per-member time, cache size, number of members that errored; and the same three checks one after another.
- Compare the `--workspace` findings to those of running each member by hand (acceptance: identical).
- Re-run while another lane is building, to see that neither side waits on the other. Prefer a quiet window if one is announced.
- [ ] The numbers go in the PR body with the ref and the command, and replace the estimates in "Task 0 results".

---

## Review Focus (input classes the tests above do not obviously cover)

1. **A changed file served as fresh** (the measured hazard G): any code path that rewrites a file without bumping its mtime; or a future rewrite that copies a file's old timestamp. Expect a test that fails when the mtime is held back.
2. **Two CodeRipper runs, same repository, same moment:** neither may hang; one may run uncached with a message.
3. **A member that fails to build** must not abort the loop or be silent: errors named by member, non-zero exit.
4. **Cache directory unwritable, disk full mid-build, or `CODERIPPER_CACHE_DIR` pointed at something important** (`C:\Projects`): fall back with a reason; never delete outside the marked root.
5. **Toolchain change or a per-project `rust-toolchain.toml`:** the toolchain id is read in the project directory, so a different toolchain gets a different directory and the old one is prunable.
6. **A workspace with `exclude`, nested workspaces, or a member outside the workspace directory** (the existing `package::prefix_in` error): `--workspace` must say which member, not fail opaquely.

7. **A restored file that is newer than the artifact is correct; one that is older is a stale hit** (row L): the restore must be the last write to the file and must stamp
   "now". Attack: any path that restores through a copy that preserves timestamps (`copy`, `robocopy /COPY:T`, `cp -p`, an archive extract).
8. **Cyclic dev-dependencies between members:** building a member with `--all-targets` can pull in a member that depends on it (fuel has two such groups, of 3 and 7 members). Audit-measured on a fixture: correct, only costlier (the cycle's members recompile once). Covered by `a_dev_dependency_cycle_still_analyses_every_member`.
9. **A check that creates files, or edits tracked files, outside the member's `src/`** (a `build.rs` output into the source tree, a generated file, a write into a sibling): audit-verified that none of the three checks does today, and a restore that scopes to the member would not see an edit elsewhere; the post-restore `git status --untracked-files=no` over the whole checkout is the guard that turns it into poison instead of silent wrong answers. Untracked files are deliberately *not* removed (`git clean` was measured to delete a non-ignored `target/` and other members' files).
10. **Two CodeRipper runs on one repository at once** (a `--workspace` run and anything else): the session holds the cache lock for its whole run (decision 10) and the test in Task 5 forces the interleave; PR 1.1 covers plain runs. Attack: any path that writes into the cache directory without holding its lock.
11. **A restore that fails transiently** (antivirus, an open editor, `index.lock`): retried, then poisons; check that no code path continues analysing a dirty checkout.
12. **`core.autocrlf` / `.gitattributes` eol conversion:** the rewriters read and write the bytes git smudged on checkout, and `git checkout HEAD -- <member>` smudges again, so bytes round-trip on this box (autocrlf=true, audit-checked); a `.gitattributes` eol rule was not tested.

## Not in scope (and why)

- **Parallel members.** Cargo's build lock serialises builds in one target directory anyway; parallelism would need one cache per worker (and, with a session, one session per worker). Measure first.
- **Siblings surviving between runs** (a new session starts with new mtimes): needs content-keyed freshness; not available on stable cargo.
- **The global `~/.cargo/.package-cache` lock** and any `--offline`/`--locked` default: a separate question that needs its own measurement.
- **Unifying the two `RUSTFLAGS` sets** so dependencies build once: measured above as ~41 s of cold cost for a 58-unit crate, once per cache directory. It would change what `reachability` and `unused-parameters` see (`--force-warn` is not neutral), so it is a behaviour change, not an optimisation.
- **A `--scope` list of repositories** (the PM has ruled it out until CireSnave decides).
- **CI for this repository:** the workspace tests use tempdir fixtures with path dependencies, so they need no network and add a few builds of tiny crates. **CI cost added is unmeasured here; the PR bodies will report it, as the coverage PR did.** The existing three build jobs are unchanged.

## Open risk, stated plainly

Measured on the real workspace (Task 0 results): the cache cuts `fuel-core` from 234 s to 90 s and a median member from 208 s to 43 s; what remains is mostly sibling crates.
**The session design's value on fuel is a model, not a measurement**: its savings are the sibling share (76-91% of the remainder) multiplied across 43 members and 3 checks, and it
has only been demonstrated on a 3-crate chain (row K: 5 units versus 6). Task 7 measures it; if the first real `--workspace` run does not show a large gain, the PR description will say
so and PR 2 can be reduced to the loop-plus-tagging without the session. The cache is worth keeping either way (Task 0 results).
