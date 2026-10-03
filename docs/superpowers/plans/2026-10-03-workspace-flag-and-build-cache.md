# `--workspace` and a dependency-build cache — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **STATUS: PLAN FOR APPROVAL (revision 3, after two independent audits). No code has been written for this plan.**
> The audit found four blocking defects, all accepted and fixed below; the largest (B1) is a *measured limit of the idea itself*: an
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
| — | Size of the shared directory after A-G's shared runs | 533 MB for a 58-unit crate, debug profile, two flag sets |

Rows E-G are saved in `cachexp2.out`, H-I in `cachexp3.out` (A-D in `cachexp.out`), beside the scripts.

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

## Where the cache does not help (and what was considered)

- **In-repo path crates are rebuilt per run** (row H). For `--workspace` this means the siblings a member depends on are compiled again
  for every (member, check) pair, because each pair builds in its own new worktree. Registry/git dependencies, which in most
  workspaces are the bulk of the compile time, are not. *Whether that holds for fuel is unmeasured; Task 0 decides whether PR 2 is
  worth building in this form.*
- **In-repo rebuilds do not grow the cache** (audit, from the fingerprint directory after row H): a rebuilt in-repo crate overwrites its own
  unit in place; it does not add one per worktree. So the size cap is about external dependencies and flag sets, not about run count.
- **Rejected: normalising mtimes** (set every unrewritten file's mtime to a fixed old value so siblings look fresh). It is unsafe:
  cargo would then treat changed content as fresh whenever its timestamp is older than an artifact, e.g. build commit X, switch
  to an older commit, switch back. Row G is that failure. A content-based freshness check (`-Zchecksum-freshness`) is nightly-only.
- **Considered, not planned: one session worktree per `--workspace` run**, rewriting each member in place and restoring its bytes
  *and* mtime afterwards, so unrewritten siblings keep stable mtimes and stay fresh. It is a larger change to `RewrittenWorktree`
  (which today owns one throwaway checkout per check run). Revisit only if Task 0 shows siblings dominate.
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

## File Structure

| File | Responsibility |
|---|---|
| `src/build_cache.rs` (new) | where the cache lives; the key; the bounded lock; pruning; `CacheStats`. No cargo knowledge. |
| `src/cargo_json.rs` | `build_with` asks `build_cache` for a target directory and sets `CARGO_TARGET_DIR`; counts `compiler-artifact` `fresh` flags into `BuildOutput` and into the process-wide `build_stats`. |
| `src/worktree.rs` | `RewrittenWorktree` exposes its `source_repo` (it already stores it) so a build can key the cache by the *source* repository, not the throwaway checkout. |
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
- **The route from a build to the user** (the audit's B2): `Check::run` is unchanged, so builds report into a process-wide
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

# PR 2 — `--workspace`

### Task 4: members and `Check::unit()`

**Files:** `src/package.rs`, `src/check.rs`, the five checks, `src/lib.rs`.

- `package::workspace_members(dir) -> anyhow::Result<(PathBuf, Vec<Package>)>`: `(workspace_root, members)` from the existing `metadata()` (`cargo metadata --no-deps` lists the workspace's packages; directories under `[workspace] exclude` are not members and are not listed: verified, and README will say so). Sorted by path for a deterministic order.
- `Check::unit()`: `Package` for `reachability`, `unused-return-values`, `unused-parameters`; **`Repository` for `version-consistency` and `ci-protection-presence`** (the first already analyses a whole workspace and answers `Info` from a member; the second reads the GitHub settings of the repository and would repeat one finding per member).
- **The audit's B3:** `run_checks_over(checks, ctx, tier, only_check_id)` runs every check of the tier, so it cannot by itself run
  "package checks per member, repository checks once". It gains a fifth parameter `units: UnitFilter` (`Any` | `Only(Unit)`); `run_checks`
  passes `Any` and behaves as today, and `run_workspace` calls it with `Only(Repository)` once and `Only(Package)` per member. A named
  check (`check <id> --workspace`) is run only in the unit it declares.
- **Stale-entry dedupe (audit S11):** `stale_findings` reports unknown-check entries regardless of which checks ran, and on a workspace whose root is
  itself a package, the repository-unit run and the root member's run load the same `.coderipper.toml`. `run_workspace` therefore
  keeps the set of allowlist files already judged and drops a second identical stale/unknown finding for the same file and entry.
- [ ] Failing tests: `a_package_unit_filter_skips_repository_checks_and_the_reverse` (drives `run_checks_over` with fakes);
  `a_root_package_workspace_reports_an_unknown_check_entry_once`; `members_of_a_virtual_workspace_are_listed_sorted`; `a_root_package_workspace_lists_the_root_too`; `an_excluded_directory_is_not_a_member` (verified: `cargo metadata --no-deps` on a workspace with `exclude = ["skip"]` lists only `a` and `b`; the README does not say this today, Task 5 adds it); `two_members_with_the_same_directory_name_are_both_listed`; `the_registered_checks_declare_their_unit` (pins the five).
- [ ] Commit `feat: workspace members and a per-check unit`.

### Task 5: `run_workspace`, tagging, flag

**Files:** `src/lib.rs`, `src/finding.rs`, `src/main.rs`, `tests/workspace_run.rs` (new), `README.md`.

**Behaviour (each line has a test below):**
1. `--workspace --project <dir>` runs the workspace that contains `<dir>` (`<dir>` may be the root or a member).
2. Repository-unit checks run **once**, with `project_root = workspace root`.
3. Package-unit checks run once **per member**, in sorted order, each through the existing `run_checks_over` with `project_root = member directory`, so allowlist loading and stale-entry findings are exactly those of `--project <member>`.
4. Every finding from a member run gets `member = Some(<package name>)`; the printed line leads with it.
5. An error in one member (it does not compile, an unparsable file) is reported as `<member>: <existing message>` and the loop **continues**; the process exits non-zero if any error occurred (unchanged rule: a check error fails the run).
6. A virtual root is fine (it is never itself analysed as a package); a root package is a member like any other.
7. The run ends with one summary line: members run, members with errors, and the totals from the `build_stats` accumulator (Task 2), so a user can see the cache working. It is on stderr, like the cache line.

**Failing tests first** (`tests/workspace_run.rs`, tempdir git repos, `CODERIPPER_CACHE_DIR` per test):
- `every_member_is_checked_and_findings_name_their_member`: virtual workspace with members `a`, `b` each containing one dead private function; findings for both, labelled `a` and `b`.
- `findings_equal_those_of_running_each_member_by_hand`: the finding sets of `--workspace` and of `--project a` plus `--project b` are equal (the property decision 3 buys).
- `a_member_that_does_not_compile_does_not_stop_the_others_and_the_run_fails`: member `c` has a syntax error; `a` and `b` still report; exit non-zero; the error line names `c`.
- `repository_checks_run_once_not_per_member`: `version-consistency` finding count equals what `--project <root>` gives (not multiplied by members).
- `members_sharing_a_directory_name_are_told_apart`: `x/util` and `y/util`.
- `a_member_directory_given_to_workspace_runs_the_whole_workspace`.
- `the_second_member_reuses_an_external_dependency_the_first_built` (the cache payoff, deterministic): three members all depending on one path crate **outside the repository** (absolute path); assert **relative properties, not an exact count**: the stderr `cache:` line of the run reports the external crate **fresh** for the second and third member, and the total compiled units for a 3-member run is **less than three times** a 1-member run's. (An exact formula is wrong: `unused-return-values` builds with a second `RUSTFLAGS` set, so an external dependency compiles once *per flag set* (row C), and `reachability` builds `--lib` while the others build `--all-targets`, so units per member per check differ. The per-check unit counts are not derived here, so the plan does not assert them.) This is the test that answers "how is it tested without a 43-member fuel run". A companion test with the shared crate **inside** the repository asserts the opposite (not fresh; pins row H).
- CLI: `--workspace` without a Cargo workspace is an error that says so; `--workspace` with `check ci-protection-presence` runs once.
- [ ] Commit `feat: --workspace runs the project-scope checks over every member`.

### Task 6: acceptance on `fuel`, measured and reported (not a CI test)

Run on this box, on the real 43-member workspace, and **report the measurements; do not assert them in the plan**:
- `check unused-return-values --project fuel/fuel-core` before (uncached, today) and after (cold cache, then warm cache): wall time and cache directory size. Compare findings to the recorded **reachability 102 / unused-return-values 2** for `fuel-core` (acceptance: identical).
- `check reachability --workspace --project fuel`: total wall time cold and warm, per-member time, cache size, number of members that errored.
- Re-run while another lane is building in the same checkout, to see that neither side waits on the other.
- [ ] The numbers go in the PR body, with the ref and the command.

---

## Review Focus (input classes the tests above do not obviously cover)

1. **A changed file served as fresh** (the measured hazard G): any code path that rewrites a file without bumping its mtime; or a future rewrite that copies a file's old timestamp. Expect a test that fails when the mtime is held back.
2. **Two CodeRipper runs, same repository, same moment:** neither may hang; one may run uncached with a message.
3. **A member that fails to build** must not abort the loop or be silent: errors named by member, non-zero exit.
4. **Cache directory unwritable, disk full mid-build, or `CODERIPPER_CACHE_DIR` pointed at something important** (`C:\Projects`): fall back with a reason; never delete outside the marked root.
5. **Toolchain change or a per-project `rust-toolchain.toml`:** the toolchain id is read in the project directory, so a different toolchain gets a different directory and the old one is prunable.
6. **A workspace with `exclude`, nested workspaces, or a member outside the workspace directory** (the existing `package::prefix_in` error): `--workspace` must say which member, not fail opaquely.

## Not in scope (and why)

- **Parallel members.** Cargo's build lock serialises builds in one target directory anyway; parallelism would need one cache per worker. Measure first.
- **The global `~/.cargo/.package-cache` lock** and any `--offline`/`--locked` default: a separate question that needs its own measurement.
- **Unifying the two `RUSTFLAGS` sets** so dependencies build once: measured above as ~41 s of cold cost for a 58-unit crate, once per cache directory. It would change what `reachability` and `unused-parameters` see (`--force-warn` is not neutral), so it is a behaviour change, not an optimisation.
- **A `--scope` list of repositories** (the PM has ruled it out until CireSnave decides).
- **CI for this repository:** the workspace tests use tempdir fixtures with path dependencies, so they need no network and add a few builds of tiny crates. **CI cost added is unmeasured here; the PR bodies will report it, as the coverage PR did.** The existing three build jobs are unchanged.

## Open risk, stated plainly

The cache's value on the real workspace is **inferred from a 58-unit crate with no in-repo path dependencies**. There, a warm run costs
the package's own compile (13-17 s for `coderipper` itself, rows E-F) instead of the dependency build (50-54 s cold); the
1.4-3.9 s of rows B-D is the best case and is **not** what a real run sees. Fuel differs in ways that cut against the cache: 43
members that depend on each other by path (rebuilt per run, row H), build scripts, git dependencies. **Task 0 is the measurement that decides
whether PR 2 is worth building as designed; Task 6 decides whether the cache stays.** The PR descriptions will say what they showed.
