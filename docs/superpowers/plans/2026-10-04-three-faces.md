# CodeRipper's three faces: library, `cargo coderipper`, hosted service — Plan

> **Status: PLAN FOR APPROVAL (revision 2, after one independent reading-only audit; no cargo was run while this was written).**
> Revision 2 fixes the audit's 4 blocking findings (callers that `non_exhaustive` breaks, types third parties cannot construct, an example the API could not support, a version scheme that contradicted the portfolio rule) and 10 smaller ones.
> **Nothing is published until the PM approves the library plan, names the crates.io account that publishes, and reads the publication back.**

**CireSnave's ruling (2026-10-04, verbatim, relayed by the PM):** *"I have always said CodeRipper should be runnable alongside "cargo clippy" as another check but also stated quite clearly yesterday that I was planning on running it as a hosted service on ThinkersJournal.com. Also building it as a library means it is easier for us to test. All three please. It's first version must at least be the library portion. Everything else is nice bells and whistles at this point."*

**Order of delivery:** (1) the library, (2) the `cargo coderipper` subcommand, (3) the hosted service. Section 1 is the first release; 2 and 3 are plans in smaller detail.

## Facts this plan rests on (read from `main@0.2.12`, 2026-10-04; nothing compiled or run)

- crates.io (read-only API): `coderipper` and `cargo-coderipper` both **do not exist** (free). `git ls-remote --tags` is empty and there was no CHANGELOG before 2026-10-03: none of 0.1.0–0.2.12 was ever published.
- `Cargo.toml`: lib + one `[[bin]] coderipper`; **no `rust-version`**; description still says *"dependency staleness"* and *"Runs as a CLI (like clippy) or a server"*; 5 keywords and categories `development-tools`, `command-line-utilities` (valid slugs); `readme`, `repository`, `license = "MIT OR Apache-2.0"` present; `LICENSE-MIT` and `LICENSE-APACHE` tracked; `Cargo.lock` tracked (15.6 KB; `cargo package` ships it for a crate with a bin, `cargo install` ignores it unless `--locked`).
- **Dependencies vs crates.io latest** (lock vs API): every direct dependency equals the latest stable **except `toml`: lock 0.9.12, latest 1.1.6** (`toml = "0.9"` cannot reach 1.x; the only use is `toml::from_str` at `src/allowlist.rs:54`). CireSnave's standing rule: *"I want all of my projects' dependencies on their most recent versions at all times."* So this is a prerequisite. Transitive dependencies are not checked (`cargo update --dry-run` needs cargo).
- **True MSRV is Rust 1.89** (about 90% sure of the stabilisation version, to be confirmed by `cargo +1.89 check`): `File::try_lock` / `File::unlock` / `TryLockError` (the build-cache lock) are the newest std APIs in use; the others are older (`Option::is_none_or` 1.82, let-else 1.65, `set_modified` 1.75, `is_some_and` 1.70, `OnceLock` 1.70). Dependency MSRVs on crates.io are all at or below 1.89 (toml 1.1.6: 1.85; clap: 1.85; syn: 1.71).
- **The public surface is accidental**: `pub mod` for `check`, `checks`, `finding`, `github`, `build_cache`; public structs with all-`pub` fields and no `#[non_exhaustive]` (`Finding`, `CheckContext`, `Location`, `RunResult`, `WorkspaceRun`, `CacheConfig`, `ApiError`); public enums without it (`Severity`, `Confidence`, `Scope`, `Unit`, `Network`, `Tier`, `FindingError`). Adding `Finding.member` in 0.2.12 was already a breaking change for anyone constructing a `Finding`. `lib.rs` has **no crate docs**; no `examples/`; no `missing_docs`.
- **No public entry point runs a user's own `Check` through the host**: `run_checks` is hard-wired to the five registered checks; `run_checks_over` and `run_workspace_over` are private. A user can call `check.run(ctx)` directly, which skips `Finding::validate` and the allowlist.
- README: line 5 promises **"stale dependencies"** (not built); line 20 describes `coderipper serve` as planned (it exits with "server mode is not implemented yet"); the `## Status` section says "three checks are implemented" (five are registered) and points into `docs/superpowers/`; the CLI's own `about` string says "or as a server".
- The checks analyse **committed HEAD** (each build is a throwaway `git worktree add --detach … HEAD`), not the working tree.
- Package contents by `git ls-tree -l`: `src` 361 KB (31 files), `tests` 131 KB (13), **`docs/` 573 KB of internal plans**, `.github`, `codecov.yml`.
- The audit-host spec §7: a hosted instance is **free, for open-source projects**, serves **untrusted/arbitrary public repos**, and its sandboxing, resource limits and abuse prevention *"need their own pass before that mode ships"*; submission flow, rate limits and public-vs-submitter-only results are **explicitly undesigned**.
- Timing/size figures used in section 3, with their sources: a small crate cold build **50–54 s** (`cachexp.out`, 58 units); `fuel-core` cold **233.7 s** (`t0.out`, one run, machine at 77–95% CPU from other lanes); `fuel-core`'s target directory was **1553 MB** (a `du` line printed during cleanup, not saved to a file).

---

## 1. The library — first release

### 1.1 Goal and non-goals

A published crate `coderipper` whose **library** is the product: embed the checks in your own tool or test, run them over a project or a cargo workspace, get structured `Finding`s back, and write your own `Check`. The `coderipper` CLI ships in the same package as the *client*, not the promise. **Non-goals:** `cargo coderipper` (section 2), anything hosted (section 3), dependency staleness (not built, so not mentioned anywhere public), a typed error enum (see the limit in 1.2).

### 1.2 The public API we will promise (the semver surface)

**Complete classification of what is public today** (every item reachable from `src/lib.rs`; the audit checked the table against the code):

| Area | Stable (documented, tested, promised) | Not promised (`#[doc(hidden)]` or `pub(crate)`) |
|---|---|---|
| Running | `run_checks`, **new `run_checks_with(&[Box<dyn Check>], &CheckContext, Tier, Option<&str>) -> RunResult`** (the host's validation, allowlist and suppression over checks the caller supplies; `run_checks` becomes a thin wrapper over it), `run_workspace`, `registered_checks`, `RunResult`, `WorkspaceRun` | `UnitFilter`, `run_checks_over`, `run_workspace_over` (already private) |
| Checks | `trait Check` (`id`, `scope`, `network`, `tier`, `unit`, `run`), `CheckContext`, `Scope`, `Network` (and `Network::tier`), `Tier`, `Unit`; the five checks (`ReachabilityCheck`, `UnusedReturnValuesCheck`, `UnusedParametersCheck`, `VersionConsistencyCheck` are unit structs; `CiProtectionPresenceCheck` has `new()`, `Default` and `with_api`) | `ci_protection_presence::SETTINGS_FILE` and the other check-module constants |
| Findings | `Finding`, `Severity`, `Confidence`, `Location`, `FindingError`, `Finding::validate`, **the JSON shape of `Finding`** (serde) | |
| Faking GitHub | `github::{Github, ApiError, GhCli}` (so `CiProtectionPresenceCheck::with_api` is testable offline: the "easier for us to test" payoff) | `parse_github_remote`, `origin_url`, `RepoRef` |
| Cache | `build_cache::{CacheConfig, set_cache_config}` (an embedder, e.g. the hosted service, must control it) | `config_from_env`, `status`, `prune_to_cap`, `BuildStats`, `render_stats`, `take_stats`, `RepoDirInfo`, `MARKER`: CLI plumbing the bin uses |
| Files | the **`.coderipper.toml` allowlist format** (documented in the README and the crate docs) | |

**Shaping changes (all breaking relative to 0.2.12):**
1. `#[non_exhaustive]` on `Finding`, `CheckContext`, `Location`, `RunResult`, `WorkspaceRun`, `CacheConfig`, `ApiError` and on `Severity`, `Confidence`, `Scope`, `Unit`, `Network`, `Tier`, `FindingError`, so adding a field or variant later is not breaking.
2. **A constructor for every one of them that outsiders must build** (the audit's B2): `CheckContext::new(project_root, portfolio_root)`; `Finding::new(check_id, severity, confidence, project, summary, detail)` with chained setters (`.location(Location)`, `.subject(..)`, `.positive_control(..)`, `.member(..)`); `Location::new(file, line)`; `CacheConfig::new(root)` with `.wait(Duration)` and `.max_bytes(u64)`; `ApiError::new(status, message)` (for fakes). Code inside the crate keeps using struct literals.
3. **Migrate the callers `non_exhaustive` breaks** (the audit's B1): `src/main.rs:172` (the bin is a separate crate) and **13 `CheckContext {` literals in 7 integration-test files** (`tests/allowlist.rs`, `ci_protection.rs`, `reachability.rs`, `unused_parameters.rs`, `unused_return_values.rs`, `version_consistency.rs`, `workspace.rs`) switch to `CheckContext::new`. `tests/` has no `Finding` or `Location` literals.
4. Hide the plumbing: `UnitFilter` becomes `pub(crate)`; the CLI half of `build_cache` becomes `#[doc(hidden)]` (the bin keeps using it).
5. A **`cli` feature (default on)** gating `clap` and the binary (`[[bin]] required-features = ["cli"]`): embedders write `coderipper = { version = "0.3", default-features = false }`; `cargo install coderipper` still works. **Tests that run the binary** (`tests/cli.rs`, `build_cache.rs`, `workspace_run.rs` use `Command::cargo_bin("coderipper")`) are gated with `#![cfg(feature = "cli")]`; the no-default-features CI job runs `cargo check` and `cargo test --lib`.
6. `#![warn(missing_docs)]` and `#![deny(rustdoc::broken_intra_doc_links)]` (the code has many intra-doc links; whether any are broken is unverified until `cargo doc` runs).

**Semver promise, stated in the README and crate docs:** pre-1.0, a breaking change to anything in the stable column bumps the **second** number (0.3 → 0.4) and is listed under "Breaking" in the CHANGELOG; additions bump the third. The `.coderipper.toml` format and the `Finding` JSON shape follow the same rule and are enforced by **golden-file tests** (a pinned `Finding` JSON sample and parser tests for the allowlist), because `cargo-semver-checks` covers the Rust API only. `cargo-semver-checks` runs in CI from the first release (it accepts a git revision as the baseline, so it does not have to wait for a published one; flag names from memory, to be confirmed). **Stated limit:** public signatures expose `anyhow::Error` (`Check::run`), so semver is coupled to `anyhow` 1.x; a typed error enum is a later, deliberate, breaking change.

### 1.3 What must be documented

- **Crate-level docs** (`//!` in `lib.rs`): what it is, a short quick start, the concepts (check, project vs workspace, `Unit`, tiers), the **"analyses committed HEAD"** limitation, the build cache and `CODERIPPER_CACHE*`, the allowlist format, the semver promise.
- **Every public item** (enforced by `missing_docs`).
- **README rewritten to what is built.** Exact edits (the audit's S1): line 5 drop "stale dependencies"; line 20 reword `serve` to a roadmap sentence that does not read as a feature (or drop it from the crate README); the `## Status` section (three checks → five, and no links into `docs/`, which the package excludes); add "Use as a library" with the quick start; link the CHANGELOG. There is **no** "Known issue/Fixed" heading (a bullet, "Fixed in 0.2.9", lives in the cache section): leave it. The CLI's own `about` string drops "or as a server".

### 1.4 Examples (the testability payoff)

Built and run by `cargo test --examples` / `cargo test` in CI; none needs the network:
1. `examples/run_on_a_project.rs`: `run_checks` / `run_workspace` over a path, print findings (needs git and cargo; the crate docs show it as `no_run`).
2. `examples/custom_check.rs`: **implement `Check` and run it through the new `run_checks_with`**, so the example exercises validation and the allowlist; the same code appears as a real doctest in the crate docs. (The first draft of this plan promised this with no public API able to do it: that is why `run_checks_with` exists.)
3. `examples/fake_github.rs`: `CiProtectionPresenceCheck::with_api(Box::new(canned))` against a temp git repository with an `origin` remote (the check still shells out to `git remote get-url origin`, so the example needs **git, not the network**).

### 1.5 `cargo publish` prerequisites (the full list)

**Metadata (`Cargo.toml`):**
- `description`: *"Audit a Rust project or workspace for dead code (including `pub` items), unused parameters and return values, version drift, and missing CI protection. Use it as a library or a CLI."* **No "dependency staleness", no "server".**
- `rust-version = "1.89"` (after `cargo +1.89 check`) and an **MSRV CI job**.
- `exclude = ["docs/", ".github/", "codecov.yml", "tests/"]`. **Why `tests/`:** `tests/reachability.rs` points at `tests/fixtures/dead-pub-fn`, which has its own `Cargo.toml`; Cargo omits subdirectories that contain their own manifest from the package (from memory, to be confirmed with `cargo package --list`), so that test would fail when run from the packaged crate. Excluding `tests/` entirely is the simple, honest choice; the examples and doctests ship instead.
- keywords (≤5) and categories as today; add `development-tools::cargo-plugins` when `cargo-coderipper` ships (section 2); the dry-run validates slugs.
- `[features] default = ["cli"]`, `cli = ["dep:clap"]`; `[[bin]] required-features = ["cli"]`.
- **Dependencies at their latest versions: `toml` 0.9 → 1.1** (its own PR, first). Everything else is already latest (direct); transitive state confirmed with `cargo update --dry-run` after the quiet window.

**Licences and provenance (CireSnave: "I am not a plageurist"):** both licence files ship; no vendored third-party source in the tree (`git ls-files` shows `src/`, `tests/`, `docs/`, CI config and top-level files); dependency licences checked with `cargo deny check licenses` after the window.

**docs.rs:** default features, no network, nothing special; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` in CI as the local proxy.

**CI additions:** `cargo test --doc` and `--examples`; the doc job above; the MSRV job (`cargo +1.89 check`, with and without default features, and `cargo test --lib --no-default-features`); a non-publishing **package job** (`cargo package --list` and `cargo publish --dry-run`); `cargo-semver-checks`.

**Release steps, and who is acting (the audit's S10):** `cargo publish` is irreversible (a yank hides a version, it never removes it) and acts as whoever owns the crates.io token. CLAUDE.md's identity rule is written for `gh`, so **the PM must name the crates.io account that publishes and authorise each publish explicitly; no lane holds or uses the token.** Steps: `cargo publish --dry-run`; `cargo publish`; read back `https://crates.io/api/v1/crates/coderipper` (version, `rust_version`, features, licence, not yanked); read the docs.rs build status; `cargo install coderipper --version <v>` into a clean directory and run `coderipper --help`; tag `v<v>` and a GitHub release whose notes are the CHANGELOG entry.

### 1.6 Work breakdown, with the version rule applied (the audit's B4)

The portfolio rule is that **every pushed change changes the version** and the PM allocates the number at gate time. So the plan does not pin a number into PRs that land in between.
1. **PR 1, dependencies:** `toml` 1.x (+ `cargo update`). Version: the next patch (0.2.13).
2. **PR 2, the first-release PR:** the API shaping (1.2), docs, examples, README, description, metadata, `exclude`, `rust-version`, the CI jobs, and the CHANGELOG entry. One cohesive PR in separate commits, because the docs and examples are written against the shaped API. Version: **0.3.0** (breaking relative to 0.2.13).
   Red-first evidence: a test crate that builds a `Finding`, `Location`, `CacheConfig` and `CheckContext` through the constructors and matches each enum with a wildcard arm; `compile_fail` doctests proving a struct literal of a `non_exhaustive` type is rejected outside the crate; the `run_checks_with` example run as a test; the golden-file tests.
3. **Release (PM):** publish **the version `main` carries**, expected **0.3.0** if nothing else lands in between. If an unrelated change lands first it becomes 0.3.1 and that is what is published; the release steps say "the version on main", never a literal. *Alternative if PR 2 is too large to review:* split docs/CI into follow-up PRs, accepting that the first published version is then 0.3.1 or 0.3.2.

### 1.7 Version number: recommend **0.3.0**

Not 0.2.13: (a) the shaping is a breaking change by CireSnave's rule (*"If it is a breaking change, the major version should change"*; pre-1.0 that is the second number); (b) 0.2.x was never published, so nobody is broken; (c) publishing 0.2.13 would freeze today's accidental surface. Not 1.0: the promise is "pre-1.0, deliberately", and checks are still being added.

### 1.8 Not claimed yet (needs a build, after the quiet window)

MSRV 1.89; `toml` 1.x compiling with only small changes; `cargo package --list` contents and size (including whether the nested fixture is dropped); the docs.rs build and `broken_intra_doc_links`; `cargo deny` results; that `tests` gated on `cli` behave under `--no-default-features`; transitive dependency freshness.

---

## 2. `cargo coderipper` — clippy-style invocation

**Shape.** A second binary `cargo-coderipper` **in the same package** (`cargo install coderipper` installs every `[[bin]]`, so both `coderipper` and `cargo-coderipper`; no second crate to publish). Cargo runs `cargo-coderipper coderipper <args>`; the standard clap pattern handles the extra argument. A thin wrapper over `run_checks` / `run_workspace`.

**Grammar, mapped onto the existing CLI** (the audit's S4). Today: `coderipper fast|sweep|check <id>|cache|serve`, flags `--project`, `--workspace`, `--portfolio-root`.

| `cargo coderipper …` | means |
|---|---|
| (no subcommand) | `coderipper fast` over the current package |
| `--workspace` | the same flag as today |
| `--manifest-path <path>` | cargo's spelling of `--project <dir>` (the manifest's directory); `--project` stays for the existing CLI |
| `-p`, `--package <name>` | one workspace member (resolved through `workspace_members`) |
| `--sweep` | `coderipper sweep` (adds the network tier) |
| `--check <id>` | `coderipper check <id>` |
| `--portfolio-root`, `--no-cache` | as today / `CODERIPPER_CACHE=off` |

**Smallest useful first version (one PR, after the library release):** the table above, `--message-format human|json`, `--deny <severity>`, the exit codes, docs, tests.

- **`--message-format json`**: one object per line with a `"reason"` field in cargo's style: `{"reason":"coderipper-finding","finding":{..}}` and a final `{"reason":"coderipper-summary",..}`; the `Finding` JSON shape is part of the library promise. **Not claimed:** editor integration. rust-analyzer reads cargo's own `compiler-message` diagnostics and (from memory) ignores other `reason`s, so these findings would not appear in an editor; a `compiler-message`-shaped output is a possible later flag, not in v1. `short` (`path:line: severity: summary [check]`) follows.
- **Exit codes** (cargo-style; **not** clippy's 101, which is the compiler-failure code): **0** ran, nothing at or above the deny level; **1** findings at or above `--deny <severity>` (the `-D warnings` analogue; **default: never**, so findings alone exit 0, like clippy's warnings); **2** usage error (clap's own); **3** a check could not run (today any such error exits 1). **This is also where "a High finding exits 0" gets decided**: recommend keeping that default and documenting `--deny medium` for CI, because failing by default would break the first run on every project. Changing the existing `coderipper` binary's codes is a behaviour change to a pre-1.0 CLI and, by the same rule, bumps the second number (**0.4.0**), not 0.3.x.
- **Config:** the existing `.coderipper.toml` allowlist plus `CODERIPPER_*` environment variables; no new file in v1.
- **Alongside clippy:** `cargo clippy --all-targets -- -D warnings && cargo coderipper --workspace --deny medium`, a CI snippet.

**Honest limits to state up front:** it analyses **committed HEAD** (uncommitted or unsaved edits are not seen; a working-tree snapshot mode is a later option); it compiles your project in a throwaway checkout, so the first run is slow and later runs fast (the build cache); it is a periodic/CI tool before it is an on-save tool.

**Tests:** `assert_cmd` over fixtures (exit codes per case, the JSON schema pinned, `--workspace` and `-p`), reusing the fixtures of `tests/workspace_run.rs`.

---

## 3. Hosted service on ThinkersJournal.com — the decision paragraph

*(No build, no architecture beyond the choice. The spec is explicit that this needs its own design pass; this is that pass's front page.)*

**The problem is not hosting; it is running someone else's code.** Three of the five checks (`reachability`, `unused-return-values`, `unused-parameters`) compile the target project, and compiling a Rust project runs its `build.rs` and procedural macros: arbitrary code execution by the repository being scanned. Cloudflare Workers run in V8 isolates and cannot spawn cargo (about 95% sure), and even where something can, a free public service that executes strangers' build scripts needs a real sandbox before it needs a UI. The other two checks run no repository code: `version-consistency` calls `cargo metadata --no-deps` (resolves nothing, runs no build scripts or proc-macros) and `ci-protection-presence` reads the GitHub API.

**Options, from least to most risk:**
- **A. Hosted results, not hosted compute.** A GitHub Action (or `cargo coderipper --message-format json` in the user's own workflow) runs in the **user's** CI and uploads its JSON to ThinkersJournal, authenticated by the repository's GitHub OIDC token; Workers store and render public pages. No untrusted *code* runs on our infrastructure, and it is free for open source by construction (GitHub Actions is free for public repos). **But untrusted *data* does:** the OIDC token proves *which repository* uploaded, not that coderipper ran or that the JSON is honest, so findings are attacker-controlled text (escape on render, validate the schema, size caps, forgery handling), and pull requests from forks get no `id-token`. Limit: the user adds a workflow; no "paste a URL and scan".
- **B. Scan-by-URL on our compute.** Workers do auth, quota and a queue; each job runs in an **ephemeral isolated runner** (a microVM or gVisor container per job, destroyed after) with no secrets, egress limited to crates.io through a proxy, CPU/memory/time caps, source fetched as a pinned-SHA tarball, JSON-only output with a size cap, per-repository caches only. Safe if done properly, and a project in itself.
- **C. Cloudflare-native containers behind Workers**, if the account has them (existence and limits not checked): B's shape on Cloudflare.
- **D. The two safe checks on Workers now**, after a port off `cargo metadata` and the `gh` CLI (manifest parsing and `fetch`): a cheap, safe "light tier" while A or B is built.

**Recommendation:** **A first** (it removes the code-execution trust boundary and delivers a free public hosted product soonest), **D opportunistically**, **B only after a sandbox design pass and an abuse policy**, because it is the only option that needs both. Compute cost for B is dominated by build minutes: a small crate built cold in about a minute (50–54 s, `cachexp.out`) and `fuel-core` in about four (233.7 s, one run on a loaded machine; 1.5 GB of artifacts), so budget minutes of multi-core compute per scan, more for big workspaces. **I did not look up current prices; turning that into dollars needs a price per vCPU-minute from the provider CireSnave prefers.** Auth uses `auth-framework` when ready; until then A needs only GitHub OIDC.

**What I need from CireSnave:** (1) Is "hosted" a **results dashboard fed from the user's CI** (A), **scan-by-URL** (B), or both? (2) Results **public**, or visible only to the submitter? (3) A cost ceiling per scan / per month, and which provider account is available for runners (Cloudflare Containers, Fly, a VM)? (4) Is a login required (auth-framework timing), or anonymous with rate limits? (5) Terms: are we willing to run strangers' build scripts on our infrastructure at all (B), given the liability? (6) Retention: how long do results live?

---

## Risks and what could change this plan

- `toml` 1.x may need more than a version bump in the allowlist parser (unverified); its own PR, so it cannot block the rest.
- The MSRV may rise if a dependency requires a newer Rust than 1.89 (the MSRV job catches it).
- If CireSnave wants the library to promise **more** than the table in 1.2 (a typed error enum before the first release, say), PR 2 grows and the first release moves; I recommend shipping the smaller promise and widening it deliberately.
- PR 2 is large by design; if review is the constraint, 1.6 gives the split and its cost (a first published version of 0.3.1 or 0.3.2).
