# CodeRipper's three faces: library, `cargo coderipper`, hosted service — Plan

> **Status: PLAN FOR APPROVAL. Plan/doc only; no code, no cargo run (a build-quiet window was in force while this was written).**
> **Nothing is published until the PM approves the library plan and reads the publication back.**

**CireSnave's ruling (2026-10-04, verbatim, relayed by the PM):** *"I have always said CodeRipper should be runnable alongside "cargo clippy" as another check but also stated quite clearly yesterday that I was planning on running it as a hosted service on ThinkersJournal.com. Also building it as a library means it is easier for us to test. All three please. It's first version must at least be the library portion. Everything else is nice bells and whistles at this point."*

**Order of delivery:** (1) the library, (2) the `cargo coderipper` subcommand, (3) the hosted service. Section 1 is the first release; sections 2 and 3 are plans, in smaller detail.

## Facts this plan rests on (read from `main@0.2.12`, 2026-10-04; nothing run)

- crates.io, read-only: `coderipper` and `cargo-coderipper` both **do not exist** (free). No tags, no GitHub releases, no CHANGELOG before 2026-10-03; none of 0.1.0–0.2.12 was ever published.
- `Cargo.toml`: lib + one `[[bin]] coderipper`; **no `rust-version`**; description still says *"dependency staleness"* and *"Runs as a CLI (like clippy) or a server"*; keywords (5) and categories (`development-tools`, `command-line-utilities`) are well-formed; `readme`, `repository`, `license = "MIT OR Apache-2.0"` present; `LICENSE-MIT` and `LICENSE-APACHE` tracked.
- **Dependencies vs crates.io latest** (read-only API): everything matches the latest stable **except `toml`: in use 0.9.12, latest 1.1.6+spec-1.1.0**. CireSnave's standing rule is *"I want all of my projects' dependencies on their most recent versions at all times"*, so this is a prerequisite, not a nicety.
- **True MSRV is Rust 1.89**: `File::try_lock` / `TryLockError` / `File::unlock` (the build-cache lock) are stable from 1.89; the other std APIs in use are older (`is_some_and` 1.70, `set_modified` 1.75). Declared today: nothing. (To be confirmed with `cargo +1.89 check` after the quiet window.)
- **Public surface today is accidental**: `pub mod` for `check`, `checks`, `finding`, `github`, `build_cache`; public structs with all-`pub` fields and no `#[non_exhaustive]` (`Finding`, `CheckContext`, `RunResult`, `WorkspaceRun`, `Location`); public enums without `#[non_exhaustive]` (`Severity`, `Confidence`, `Scope`, `Unit`, `Network`, `Tier`). Adding `Finding.member` in 0.2.12 was already a breaking change for anyone constructing a `Finding`.
- `lib.rs` has **no crate-level docs** (docs.rs would open on an empty page); no `examples/`; no `#![warn(missing_docs)]`.
- README promises **"stale dependencies"** (not built) and describes `coderipper serve` as planned (not built; `serve` exits with "not implemented").
- The checks analyse **committed HEAD** (each build is in a throwaway `git worktree` of HEAD), not the working tree. That is fine for CI and sweeps, and a real limitation for "run it like clippy while I edit".
- Package contents by `git ls-files`: `src` 361 KB (31 files), `tests` 131 KB, **`docs/` 573 KB of internal plans**, `.github`, `codecov.yml`. Far under crates.io's size limit, but the plans do not belong in a published crate.
- The audit-host spec (§7) already says: a hosted instance is **free, for open-source projects**, serves **untrusted/arbitrary public repos**, and *"needs its own pass before that mode ships"* for sandboxing, resource limits and abuse prevention; submission flow, rate limits, and whether results are public are **explicitly undesigned**.

---

## 1. The library — first release

### 1.1 Goal and non-goals

A published crate `coderipper` whose **library** is the product: embed the checks in your own tool or test, run them over a project or a whole cargo workspace, get structured `Finding`s back, and write your own `Check`. The CLI (`coderipper`) ships in the same package but is the *client*, not the promise. **Non-goals for this release:** `cargo coderipper` (section 2), anything hosted (section 3), dependency-staleness (not built, so not mentioned), a stable error enum (see 1.3).

### 1.2 The public API we will promise (the semver surface)

| Area | Stable (documented, doc-tested, covered by the promise) | Not promised (`#[doc(hidden)]` or `pub(crate)`) |
|---|---|---|
| Running | `run_checks`, `run_workspace`, `registered_checks`, `RunResult`, `WorkspaceRun` | `UnitFilter` (host-internal), `run_*_over` |
| Checks | `trait Check` (`id`, `scope`, `network`, `tier`, `unit`, `run`), `CheckContext`, `Scope`, `Network`, `Tier`, `Unit`; the five concrete checks as unit structs / `new()` | the check modules' internals |
| Findings | `Finding`, `Severity`, `Confidence`, `Location`, `FindingError`, `Finding::validate`, **the JSON shape of `Finding`** (serde) | |
| Faking GitHub | `github::{Github, ApiError, GhCli}` (so `CiProtectionPresenceCheck::with_api` is testable offline, which is the "easier for us to test" payoff) | `parse_github_remote`, `origin_url`, `RepoRef` (internal helpers) |
| Cache | `build_cache::{CacheConfig, set_cache_config}` (an embedder, e.g. the hosted service, must control it) | `config_from_env`, `status`, `prune_to_cap`, `BuildStats`, `render_stats`, `take_stats`, `RepoDirInfo`, `MARKER`: CLI plumbing |
| Files | the **`.coderipper.toml` allowlist format** (a user-facing contract, documented in the README and the crate docs) | |

**Shaping changes (all breaking relative to 0.2.12, which is why the first release is 0.3.0; see 1.7):**
1. `#[non_exhaustive]` on `Finding`, `CheckContext`, `Location`, `RunResult`, `WorkspaceRun` and on `Severity`, `Confidence`, `Scope`, `Unit`, `Network`, `Tier`, so adding a field or a variant later is not a breaking change.
2. Constructors so third parties can still build the non-exhaustive structs: `CheckContext::new(project_root, portfolio_root)`; `Finding::new(check_id, severity, confidence, project, summary, detail)` plus chained setters (`.location(..)`, `.subject(..)`, `.positive_control(..)`, `.member(..)`). Code inside the crate keeps using literals.
3. Hide the plumbing: `UnitFilter` and the CLI half of `build_cache` become `pub(crate)` or `#[doc(hidden)]`; the bin uses a small `#[doc(hidden)] pub mod __cli` (or the same items behind the `cli` feature) so the *documented* surface stays small.
4. A **`cli` feature (default on)** gating `clap` and the binary (`[[bin]] required-features = ["cli"]`): embedders write `coderipper = { version = "0.3", default-features = false }` and do not compile `clap`; `cargo install coderipper` still works unchanged.
5. `#![warn(missing_docs)]` and `#![deny(rustdoc::broken_intra_doc_links)]`.

**Semver promise, stated in the README and crate docs:** pre-1.0, a breaking change to anything in the stable column bumps the **second** number (0.3 → 0.4) and is listed under "Breaking" in the CHANGELOG; additions bump the third (0.3.0 → 0.3.1), per CireSnave's rule ("pre-1.0, 0.n would be n as the major version"). The `.coderipper.toml` format and the `Finding` JSON shape follow the same rule. Enforced from the *second* release by `cargo-semver-checks` in CI against the previous published version (it needs a published baseline, so not for 0.3.0). **Stated limit:** public signatures expose `anyhow::Error` (`Check::run`), so semver is coupled to `anyhow` 1.x; a typed error enum is a later, breaking, change we will make deliberately.

### 1.3 What must be documented

- **Crate-level docs** (`//!` in `lib.rs`): what it is, the three-line quick start, the concepts (check, project vs workspace, `Unit`, tiers), the **"analyses committed HEAD"** limitation, the build cache and `CODERIPPER_CACHE*`, the allowlist format, the semver promise.
- **Every public item** (enforced by `missing_docs`), with examples on the entry points.
- **README rewritten to what is built** (exact edits in 1.5). **CHANGELOG** (exists) gets a 0.3.0 entry with a "Breaking" list.

### 1.4 Examples and tests-as-documentation (the testability payoff)

All three run **offline in CI** (no network, no `gh`):
1. `examples/run_on_a_project.rs`: `run_checks` / `run_workspace` over a path, print findings (`no_run` in docs; it needs git and cargo).
2. `examples/custom_check.rs`: implement `Check` for a toy rule and run it through `run_checks_over`-equivalent public API on a temp dir; a real, runnable doctest.
3. `examples/fake_github.rs`: `CiProtectionPresenceCheck::with_api(Box::new(canned))`, the offline way to test a check that normally calls GitHub. This is the concrete answer to "building it as a library means it is easier for us to test".

### 1.5 `cargo publish` prerequisites (the full list)

**Metadata (`Cargo.toml`):**
- `description` rewritten: *"Audit a Rust project or workspace for dead code (including `pub` items), unused parameters and return values, version drift, and missing CI protection. Use as a library or a CLI."* **No "dependency staleness", no "server".**
- `rust-version = "1.89"` (after confirming with `cargo +1.89 check`) and an **MSRV CI job**.
- `exclude = ["docs/", ".github/", "codecov.yml"]` (keep `tests/`, `README.md`, `CHANGELOG.md`, both licence files); review with `cargo package --list`.
- keywords (≤5) and categories as today (both valid slugs); optionally add `documentation`/`homepage` only if they point somewhere real.
- `[features] default = ["cli"]`, `cli = ["dep:clap"]`; `[[bin]] required-features = ["cli"]`.
- **Dependencies at the latest versions: bump `toml` 0.9 → 1.1** (the allowlist parser; separate PR, first). Confirm the rest with `cargo update --dry-run` after the quiet window.

**README must match what is built.** Exact edits: line 5, remove "stale dependencies" from the list of what it finds; line 20, reword the `serve` mention to a roadmap sentence that does not read as a feature (or drop it from the crate README); add "Use as a library" with the quick start; link the CHANGELOG; keep the "Workspaces", "Build cache" and "Known issue/Fixed" sections.

**Licences and provenance (CireSnave: "I am not a plagiarist"):** both licence files ship (`LICENSE-MIT`, `LICENSE-APACHE`); `git ls-files` shows no vendored third-party source (only `src/`, `tests/`, `docs/`, CI config); dependency licences to be checked with `cargo deny check licenses` / `cargo tree` after the window. Credit: the README names the ideas it builds on only where it borrows them (none vendored).

**docs.rs:** the lib builds with default features and no network; no special `[package.metadata.docs.rs]` needed beyond optionally `all-features = false`; verify with `cargo doc --no-deps` under `RUSTDOCFLAGS="-D warnings"` in CI (a local proxy for the docs.rs build).

**CI additions (a PR of their own):** `cargo test --doc`; `cargo doc --no-deps` with warnings denied; MSRV job (`cargo +1.89 check --no-default-features` and with defaults); a non-publishing **package job** (`cargo package --list` and `cargo publish --dry-run`) so a metadata mistake fails a PR, not the release.

**Release steps (yours, with read-back):** `cargo publish --dry-run`, then `cargo publish` with CireSnave's token; read back `https://crates.io/api/v1/crates/coderipper` (version, `rust_version`, features, license, no yanked); wait for the docs.rs build and read its status; `cargo install coderipper --version 0.3.0` on a clean directory and run `coderipper --help`; tag `v0.3.0` and a GitHub release whose notes are the CHANGELOG entry.

### 1.6 Work breakdown (each a PR, tests first where code changes)

1. **Dependencies:** `toml` 1.x (+ `cargo update`), nothing else changes.
2. **API shaping** (the breaking PR; 0.3.0): `non_exhaustive`, constructors, hiding, the `cli` feature, `missing_docs`. Red-first evidence: a test crate (an `examples/` target or a `tests/` file) that constructs a `Finding` / `CheckContext` via the new constructors and matches an enum with a wildcard arm; a `compile_fail` doctest proving a struct literal of a `non_exhaustive` type is rejected outside the crate.
3. **Docs, examples, README, description, metadata, MSRV, `exclude`.**
4. **Release CI** (doc/MSRV/package jobs).
5. **Release** (you): the steps above. CHANGELOG 0.3.0 entry is written in PR 3.

### 1.7 Version number: recommend **0.3.0**

Not 0.2.13. Reasons: (a) the shaping PR is a breaking change to the public API by CireSnave's rule ("if it is a breaking change, the major version should change"; pre-1.0 that is the second number), so the next version after 0.2.12 *is* 0.3.0 by the portfolio's own rule; (b) 0.2.x was never published, so nobody is broken by the jump; (c) publishing 0.2.13 would freeze today's accidental surface. Not 1.0: the promise is "pre-1.0, deliberately", and the checks are still being added.

### 1.8 What I am not claiming (to be verified after the quiet window)

MSRV 1.89 (`cargo +1.89 check`); that `toml` 1.x needs only small code changes; the `cargo package --list` contents and size; the docs.rs build; `cargo deny` licence results; that `development-tools::cargo-plugins` is an accepted category slug (section 2) — the dry-run validates metadata.

---

## 2. `cargo coderipper` — clippy-style invocation

**Shape.** A second binary `cargo-coderipper` **in the same package** (`cargo install coderipper` installs both `coderipper` and `cargo-coderipper`; no second crate to publish). Cargo runs `cargo-coderipper coderipper <args>`; the standard clap pattern handles the extra `coderipper` argument. It is a thin wrapper over the library: `run_checks` / `run_workspace`.

**Smallest useful first version (one PR):**
- `cargo coderipper` = the **fast** tier over the current package, `--workspace` for all members, `-p/--package <name>`, `--manifest-path <path>`, `--sweep` for the network tier, `--check <id>` for one check.
- `--message-format human|json` (**json** = one object per line with a `"reason"` field like cargo's own: `{"reason":"coderipper-finding","finding":{..}}` and a final `{"reason":"coderipper-summary",..}`; the `Finding` JSON shape is part of the library's promise). `short` (`path:line: severity: summary [check]`) follows.
- **Exit codes**, clippy-like: **0** ran, nothing at or above the deny level; **1** findings at or above `--deny <severity>` (the `-D warnings` analogue; **default: never**, so findings alone exit 0, like clippy warnings); **2** usage/config error; **3** a check could not run (today any error exits 1; this is a breaking change that rides in 0.3.x). **This is also where "a High finding exits 0" gets decided**: recommend keeping the default and documenting `--deny medium` for CI, because making findings fail by default would break the first run on every project.
- Config: the existing `.coderipper.toml` allowlist plus `CODERIPPER_*` environment variables; **no new config file** in the first version.
- Documented usage "alongside clippy": `cargo clippy --all-targets -- -D warnings && cargo coderipper --workspace --deny medium`; a CI snippet; the rust-analyzer `check.overrideCommand` line using `--message-format json`.

**Honest limits to state up front:** it analyses **committed HEAD** (so unsaved or uncommitted edits are not seen; a "working-tree snapshot" mode is a later option, not in v1); it compiles your project in a throwaway checkout, so the first run is slow and later runs are fast (the build cache); it is a periodic/CI tool before it is an on-save tool.

**Tests:** `assert_cmd` over fixtures (exit codes per case, the JSON schema pinned by a test, `--workspace` and `-p`), reusing the fixtures of `tests/workspace_run.rs`.

---

## 3. Hosted service on ThinkersJournal.com — the decision paragraph

*(No build, no architecture beyond the choice. The spec is explicit that this needs its own design pass; this is that pass's front page.)*

**The problem is not hosting; it is running someone else's code.** Three of the five checks compile the target project, and compiling a Rust project runs its `build.rs` and procedural macros: arbitrary code execution by the repository being scanned. Cloudflare Workers cannot run cargo at all, and even where something can, a free public service that executes strangers' build scripts needs a real sandbox before it needs a UI.

**Options, from least to most risk:**
- **A. Hosted results, not hosted compute.** A GitHub Action (or just `cargo coderipper --message-format json` in the user's own workflow) runs in the **user's** CI and uploads its JSON to ThinkersJournal, authenticated by the repository's GitHub OIDC token; Workers store and render public pages. No untrusted code on our infrastructure; free for open source by construction (GitHub Actions is free for public repos); cost to us is storage and Worker requests. Limit: the user adds a workflow; no "paste a URL and scan".
- **B. Scan-by-URL on our compute.** Workers do auth, quota and a queue; each job runs in an **ephemeral isolated runner** (a microVM or gVisor container per job, destroyed after) with no secrets, egress limited to crates.io via a proxy, CPU/memory/time caps, source fetched as a pinned-SHA tarball, JSON-only output with a size cap, per-repository caches only. Heavy, safe if done properly, and a project in itself.
- **C. Cloudflare-native containers behind Workers** (if the account has them): B's shape on Cloudflare; instance size, duration limits and price not checked.
- **D. Safe checks on Workers now.** `version-consistency` (reads manifests) and `ci-protection-presence` (GitHub API) execute no code; they could run on Workers/WASM after a port off `cargo metadata` and the `gh` CLI. A cheap, safe "light tier" while A or B is built.

**Recommendation:** **A first** (it removes the trust-boundary problem and delivers a free, public hosted product soonest), **D opportunistically**, **B only after a sandbox design pass and abuse policy**, because it is the only option that needs one. Compute cost for B is dominated by build minutes: measured here, a small crate builds in about a minute and `fuel-core` took ~4 minutes on a loaded box (1.5 GB of artifacts), so budget minutes of multi-core compute per scan, more for big workspaces. **I did not look up current prices; turning that into dollars needs a price per vCPU-minute from the provider CireSnave prefers.** Auth uses `auth-framework` when it is ready; until then A needs only GitHub OIDC.

**What I need from CireSnave:** (1) Is "hosted" a **results dashboard fed from the user's CI** (A), **scan-by-URL** (B), or both? (2) Results **public**, or visible only to the submitter? (3) A cost ceiling per scan / per month, and which provider account is available for runners (Cloudflare Containers, Fly, a VM)? (4) Is a login required (auth-framework timing), or anonymous with rate limits? (5) Terms: are we willing to run strangers' build scripts on our infrastructure at all (B), given the liability? (6) Retention: how long do results live?

---

## Risks and what could change this plan

- `toml` 1.x may need more than a version bump in the allowlist parser (unverified); it is its own PR so it cannot block the rest.
- The MSRV may rise if a dependency requires a newer Rust than 1.89 (checked in the MSRV CI job).
- If CireSnave wants the library to promise **more** than the table in 1.2 (for example a typed error enum before 0.3.0), the shaping PR grows and the first release moves; I recommend shipping the smaller promise and widening it deliberately.
