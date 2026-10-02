# Workspace members as projects (slice 1) — Implementation Plan

> **IMPLEMENTATION STATUS (2026-10-02, post-implementation, post-review): this plan was executed on branch
> `feat/workspace-member-projects` (rebased onto `main` after #11 merged). A fresh whole-branch review then found
> one Important defect and three message problems, fixed in the last commit; the text below is kept as the
> historical record and is NOT edited to match. Where it disagrees with the code, trust the code.**
> 1. **Design decision 4 and "What running it taught" item 3 (drop out-of-package WARNINGS by file path) were
>    wrong.** The filter decided "inside the package" by file-name prefix. For a root package the prefix is empty, so
>    every other package cargo builds counted as inside; for a member, a member nested below it (`b/inner`) passed
>    `starts_with("b/")`; and a workspace's `default-members` are built along the way too. reachability reported the
>    dead code of a path dependency / nested member / default member as the project's own. The root-package case also
>    existed before this branch, so it was not purely a regression, but the doc comment claimed it was handled.
>    **Now:** `cargo_json` keeps only the diagnostics of THIS package's own compilation, identified by cargo's
>    `package_id` on each message (`package::locate` supplies the id), plus every ERROR from anywhere (a broken
>    dependency must still fail the run). Only the workspace prefix stripping from the plan remains. Three new
>    end-to-end tests failed first.
> 2. The build now passes `-p <package name>` so `default-members` are not built needlessly. This is an
>    optimisation: the package-id filter alone already gives correct results (a sabotage without `-p` fails no test).
> 3. Messages: a member outside its workspace root, a directory absent from HEAD ("is it committed?"), and a
>    directory that is not a package now say so and name the directory the USER passed, not the throwaway checkout.
>    Behaviour change: `--project <repo>/src` used to analyze the whole repository and is now refused.
> Acceptance on `fuel-core` was re-run on the final code: reachability 102 findings, unused-return-values 2.
> Not fixed: two extra `cargo metadata` calls per run (~0.1 s each).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let all three checks run on one member of a cargo workspace — `coderipper check <id> --project fuel/fuel-core`
— so they can run on `fuel` for real. Today they cannot: they assume the project directory is the repository root and
read `src/` there.

**Architecture:** A *project* stays what it is — one package. The change is where "the package" is found. The
throwaway worktree still checks out the **whole** repository (path dependencies must resolve), but its `root` becomes the
**package's directory inside the checkout**; every check already reads, rewrites and builds under `wt.root`, so member
support falls out of that one move. Two supporting pieces: resolve the package's location (git prefix, workspace prefix,
refuse a directory that is not a package), and make the build's diagnostics package-relative and package-only (cargo names
files relative to the *workspace* root, and a sibling member built as a dependency emits warnings of its own).

**Tech Stack:** Rust 2021, no new dependencies (`serde_json` is already used).

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §1 (scopes: `Project`) — no spec text changes; README gains a "Workspaces" section.
**Scope, per the PM (2026-10-02): the smallest useful slice** — one member per run. Out of scope, listed as follow-ups:
running every member in one command (`--workspace`), and sharing a dependency build cache between runs.

**Stacked on milestone D.** This branch uses `cargo_json` for reachability's diagnostics and `foreign.rs`'s `cargo metadata`
call, both from the reachability-on-lib+bin PR (#11). `git fetch origin`: if #11 has merged, branch from `origin/main`;
otherwise from `origin/feat/reachability-lib-plus-bin`, and after #11 merges rebase onto `main`
(`git rebase --onto origin/main <old base tip>`; the portfolio's repos squash-merge).

## What running it taught (verified on throwaway workspaces and on `fuel` itself, not assumed)

1. **The premises hold** (rustc/cargo 1.98.1, git on Windows): `git rev-parse --show-prefix` prints `a/` from a nested
   directory and an empty line at the top; `git worktree add` run from a nested directory checks out the *whole* repository;
   `cargo build` run from a member directory compiles that package and its dependencies, not its siblings; and cargo names
   diagnostic files **relative to the workspace root** (`b\src\lib.rs`), not to the package.
2. **No check needs to change.** All three read, rewrite and build under `wt.root`. Pointing `wt.root` at the package
   directory is the whole feature for them; the other 114+ existing tests pass unmodified.
3. **Siblings leak warnings.** Under `--cap-lints=warn` (all three checks use it) a path-dependency member that is built
   as a dependency warns too, and its file names are *not* under the member's prefix. Without dropping them, reachability
   would report another package's dead code under that package's path. The unit test for this fails before the fix.
4. **An error from a dependency must still break the build**, wherever it is — only *warnings* from outside the package are
   dropped (`a_dependency_that_does_not_compile_still_breaks_the_build`).
5. **Two different prefixes.** The member's path in the *checkout* (git prefix, e.g. `proj/m/`) differs from its path in the
   *workspace* (cargo's base, e.g. `m/`) when the workspace is below the repository root; a test builds exactly that layout.
6. **The tests are real:** the 8 workspace tests (7 end-to-end + the monorepo layout) fail on the base commit (7/7 measured
   there) and pass with the change; four sabotages each fail their named tests.
7. **Acceptance on `fuel` (read-only clone, `fuel-core`, real CLI, this branch):** `reachability` reports **102** findings in 104 s,
   exactly the count a scratch harness produced earlier for the same crate and code (an independent route to the same number);
   `unused-parameters` is clean in 128 s; `unused-return-values` reports 2 (`remove` discarded at all 15 of its call sites,
   `refresh_decode_session` at 1 of 2) in 146 s. Every run builds the crate's dependencies from scratch in its own worktree.

## Design decisions (rulings, with what each costs if wrong)

1. **A project is one package.** `--project` takes a member's directory, or a workspace root that is itself a package. A
   *virtual* workspace root is refused with the member names (`... pass --project <member directory>. Workspace members: a, b`);
   it is not silently expanded. *Cost if wrong:* a user has to name each member; `--workspace` is the follow-up.
2. **`RewrittenWorktree.root` is the package directory; a new private `worktree_path` is the whole checkout** (what
   `git worktree remove` takes). The rewrite closure still receives `src/...` paths relative to the package. *Cost:* any code
   that assumed `root` was the checkout's top breaks — none does (checked by the unchanged suite).
3. **Findings carry package-relative paths and `.coderipper.toml` is read from the member's directory.** The same shape a
   single-package repository gives today, so allowlist entries and `Finding.location` mean the same everywhere. `Finding.project`
   stays the project directory's name. *Cost if wrong:* a consumer merging findings from several members must add the member
   itself (`--workspace` will).
4. **Only warnings from outside the package are dropped; errors are kept.** (Finding 3 and 4 above.)
5. **Project scope stays project scope.** A `pub` item used only by a sibling member has no callers *in this package*, and
   reachability says so; a test pins it (`a_project_scope_check_does_not_count_a_sibling_members_use`). Whether another
   crate uses a `pub` item is the portfolio-scope sub-pass (§3 Sub-pass B). *Cost:* on `fuel`, 58 of reachability's 102
   findings in `fuel-core` are names that sibling crates use — noise for a member run until Sub-pass B exists.
6. **No dependency-build sharing between runs.** Cargo's fingerprints include the path of a path dependency and every run
   uses a fresh worktree, so sharing a target directory would only save the registry dependencies and risks cross-run staleness.
   *Cost:* ~2 minutes per check per large member; measured above.
7. **Three small `cargo metadata --no-deps` calls** (worktree creation, diagnostics, `foreign.rs`) rather than threading one
   result through every layer. *Cost:* ~3 extra process spawns per run, negligible next to the build.

## Global Constraints

- Rust edition 2021; CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --all-targets`,
  `cargo test --no-fail-fast` on **ubuntu, windows and macos**. The branch tip must pass all four.
- **A test must never point a check at the shared checkout** (`git worktree add` writes into the repo's `.git`). Use throwaway
  fixtures: `tests/common/mod.rs::git_repo_with`, or the `workspace_repo` helper the new tests define.
- Single-package repositories must behave exactly as before (git prefix `""`, workspace prefix `""`): every pre-existing test
  passes unmodified.
- Do **not** bump any version in `Cargo.toml`: the portfolio PM allocates it at gate time.
- Clone with `-c core.autocrlf=false` on this Windows box; never `checkout` in a shared tree. On Windows git may check files out
  with CRLF, so a test must not compare a checked-out file byte-for-byte.
- Backslash literals via a shell heredoc can be dropped or doubled here — use the editor/Write tool. In Python helper scripts
  `\a` becomes a BEL character.
- Commit trailers (this lane): end each commit message with
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. Do not run `gh auth switch` (the lane account
  cannot open or merge PRs here; the PM does).

## Review Focus

1. **A member's results must be about that member only**: a sibling's warnings (built as a dependency) must not become
   findings, and paths must be package-relative. → `a_member_build_reports_package_relative_paths_and_only_its_own_diagnostics` (Task 3), `unused_parameters_on_a_member_reports_that_package_only_with_package_relative_paths`, `unused_return_values_on_a_member_reports_that_package_only`, `reachability_on_a_member_reports_that_package_only` (Task 4).
2. **A broken dependency must still fail the run, not report clean.** → `a_dependency_that_does_not_compile_still_breaks_the_build` (Task 3).
3. **Only the member is rewritten; the whole checkout is removed and unregistered** on drop and when a directory is refused. → `a_workspace_member_is_the_root_and_its_siblings_are_not_rewritten`, `dropping_a_member_worktree_removes_the_whole_checkout_and_unregisters_it`, `a_virtual_workspace_root_is_refused_with_the_members_and_leaves_nothing_behind` (Task 2).
4. **A directory that is not a package is refused with an actionable message**, by every check and the CLI. → `a_virtual_workspace_root_is_refused_with_the_member_names` (Task 1), `a_virtual_workspace_root_is_refused_by_every_check_and_names_the_members`, `a_virtual_workspace_root_asks_for_a_member` (Tasks 4).
5. **The allowlist is the member's own.** → `the_allowlist_is_the_members_own` (Task 4).
6. **Layouts**: a workspace root that is itself a package; a workspace below the repository root (two different prefixes). → `a_workspace_root_that_is_itself_a_package_is_analyzed_as_that_package`, `a_workspace_below_the_repository_root_uses_both_prefixes_correctly` (Task 4).

**Known gaps accepted for this slice (documented):** no `--workspace` (one member per run); no dependency-build sharing; findings
from several members are not merged for the caller; a member whose package directory is not the directory a user would think of
(`[lib]`/`[[bin]]` targets outside the package directory) is analyzed by its `src/` only, as before.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/package.rs` (new) | Where a package lives: git prefix, workspace prefix, "is this a package directory", member names |
| `src/worktree.rs` | `root` = the package directory inside the checkout; `worktree_path` = the checkout; refuse non-packages |
| `src/cargo_json.rs` | Diagnostics made package-relative and package-only |
| `tests/workspace.rs` (new), `tests/cli.rs` | End-to-end and CLI |

Baseline before starting (branch tip of #11, `origin/feat/reachability-lib-plus-bin` at `4e77bb9`): `cargo test` shows 114 lib
tests, 8 in `tests/allowlist.rs`, 8 in `tests/cli.rs`, 20 in `tests/reachability.rs`, 11 in `tests/unused_parameters.rs`, 13 in
`tests/unused_return_values.rs`.

---

### Task 1: Where a package lives

**Files:** Create `src/package.rs`; modify `src/lib.rs` (declare it).

**Interfaces:**
- Produces: `package::git_prefix(dir: &Path) -> anyhow::Result<String>` (`""` at the repository root, `"fuel-core/"` below it);
  `package::Package { name: String, dir: PathBuf }`; `package::metadata(dir) -> anyhow::Result<Metadata { workspace_root, packages }>`
  (`cargo metadata --no-deps`, run in `dir`); `package::require_package(dir: &Path) -> anyhow::Result<Package>` (the package whose
  own directory is `dir`; otherwise an error naming the workspace members); `package::workspace_prefix(dir: &Path) -> anyhow::Result<String>`
  (`""` or `"b/"`, relative to the workspace root, which is the base cargo uses for diagnostic file names).

- [ ] **Step 1: Declare the module** — in `src/lib.rs` add `pub(crate) mod package;` after `pub mod finding;`.

- [ ] **Step 2: Write the tests first.** Create `src/package.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from the full file in Step 3.
  Run: `cargo test --lib package`
  Expected: FAIL to compile — `cannot find function git_prefix / require_package / workspace_prefix`.

- [ ] **Step 3: Replace the file with the full implementation:**

````rust
// src/package.rs
//! Where a project's package lives: inside its git repository, and inside its cargo workspace.
//!
//! A project is the directory the user passes (`--project`). It is usually a whole repository with
//! one package at its root, but it may be a workspace MEMBER (`fuel/fuel-core`) or the root package of
//! a workspace. Checks analyze exactly one package: they rewrite and read its `src/`, build only it
//! (`cargo build` run from its directory builds the package and its dependencies, not its siblings),
//! and read `.coderipper.toml` from its directory.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `dir`'s path inside its git repository: `""` at the repository root, `"fuel-core/"` below it
/// (forward slashes, trailing slash), as `git rev-parse --show-prefix` prints it.
pub(crate) fn git_prefix(dir: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-prefix"])
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{dir:?} is not inside a git repository: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/"))
}

#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub name: String,
    pub dir: PathBuf,
}

pub(crate) struct Metadata {
    pub workspace_root: PathBuf,
    /// Every workspace member (`cargo metadata --no-deps`).
    pub packages: Vec<Package>,
}

pub(crate) fn metadata(dir: &Path) -> anyhow::Result<Metadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "cargo metadata failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages = json["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(Package {
                name: p["name"].as_str()?.to_string(),
                dir: Path::new(p["manifest_path"].as_str()?)
                    .parent()?
                    .to_path_buf(),
            })
        })
        .collect();
    Ok(Metadata {
        workspace_root: PathBuf::from(json["workspace_root"].as_str().unwrap_or_default()),
        packages,
    })
}

fn same_dir(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// The package whose own directory is `dir`. A virtual workspace root, or any directory that is not
/// a package's, is an error that names the members to choose from.
pub(crate) fn require_package(dir: &Path) -> anyhow::Result<Package> {
    let meta = metadata(dir)?;
    if let Some(package) = meta.packages.iter().find(|p| same_dir(&p.dir, dir)) {
        return Ok(package.clone());
    }
    let members: Vec<&str> = meta.packages.iter().map(|p| p.name.as_str()).collect();
    anyhow::bail!(
        "{dir:?} is not a package directory (a virtual workspace root, or a directory inside one); \
         pass --project <member directory>. Workspace members: {}",
        members.join(", ")
    )
}

/// `dir` relative to its cargo workspace root: `""` when it is the root, `"b/"` for a member.
/// cargo reports diagnostic file names relative to the workspace root, so this is the prefix to strip.
pub(crate) fn workspace_prefix(dir: &Path) -> anyhow::Result<String> {
    let meta = metadata(dir)?;
    let root = meta.workspace_root.canonicalize()?;
    let relative = dir.canonicalize()?;
    let relative = relative.strip_prefix(&root)?;
    let text = relative.to_string_lossy().replace('\\', "/");
    Ok(if text.is_empty() {
        text
    } else {
        format!("{text}/")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (name, contents) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    fn pkg(name: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    }

    /// A virtual workspace with members `a` and `b`, as its own git repository.
    fn virtual_workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            &[
                (
                    "Cargo.toml",
                    "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
                ),
                ("a/Cargo.toml", &pkg("a")),
                ("a/src/lib.rs", "pub fn f() {}\n"),
                ("b/Cargo.toml", &pkg("b")),
                ("b/src/lib.rs", "pub fn g() {}\n"),
            ],
        );
        for args in [
            vec!["init", "-q"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "i",
            ],
        ] {
            Command::new("git")
                .args(args)
                .current_dir(tmp.path())
                .status()
                .unwrap();
        }
        tmp
    }

    #[test]
    fn the_git_prefix_is_empty_at_the_root_and_the_relative_path_below_it() {
        let ws = virtual_workspace();
        assert_eq!(git_prefix(ws.path()).unwrap(), "");
        assert_eq!(git_prefix(&ws.path().join("a")).unwrap(), "a/");
    }

    #[test]
    fn a_directory_outside_any_git_repository_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(git_prefix(tmp.path()).is_err());
    }

    #[test]
    fn a_member_directory_is_a_package_and_its_workspace_prefix_is_its_path() {
        let ws = virtual_workspace();
        let b = ws.path().join("b");
        assert_eq!(require_package(&b).unwrap().name, "b");
        assert_eq!(workspace_prefix(&b).unwrap(), "b/");
    }

    #[test]
    fn a_virtual_workspace_root_is_refused_with_the_member_names() {
        let ws = virtual_workspace();
        let err = require_package(ws.path()).unwrap_err().to_string();
        assert!(err.contains("not a package directory"), "{err}");
        assert!(err.contains("a, b") || err.contains("b, a"), "{err}");
    }

    #[test]
    fn a_workspace_root_that_is_itself_a_package_is_a_package() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            &[
                (
                    "Cargo.toml",
                    &format!("{}\n[workspace]\nmembers = [\"sub\"]\n", pkg("root")),
                ),
                ("src/lib.rs", ""),
                ("sub/Cargo.toml", &pkg("sub")),
                ("sub/src/lib.rs", ""),
            ],
        );
        assert_eq!(require_package(tmp.path()).unwrap().name, "root");
        assert_eq!(workspace_prefix(tmp.path()).unwrap(), "");
        assert_eq!(workspace_prefix(&tmp.path().join("sub")).unwrap(), "sub/");
    }
}
````

- [ ] **Step 4: Run the tests.** Run: `cargo test --lib package::`
  Expected: PASS, 5 tests.

- [ ] **Step 5: Format and commit.** `cargo fmt && git add -A src && git commit -m "feat: locate a package inside its git repository and cargo workspace"` (+ trailers). Expect `dead_code` warnings until Task 2; do not clippy-gate this commit.

---

### Task 2: The worktree's root is the package

**Files:** Modify `src/worktree.rs`.

**Interfaces:**
- Consumes: `package::{git_prefix, require_package}`.
- Produces: `RewrittenWorktree::create_with(project_root, rewrite)` where `project_root` is the package's directory; `RewrittenWorktree.root` is the package directory inside the checkout (the checkout's top for a one-package repository). The closure receives `src/...` paths relative to the package. A directory that is not a package is an `Err`, and the worktree it had created is removed again.

- [ ] **Step 1: Write the failing tests first.** Apply this diff to `src/worktree.rs`'s `mod tests` (it adds a `workspace_repo` helper and three tests; they use the existing `pub_to_pub_crate` helper):

````diff
--- a/src/worktree.rs
+++ b/src/worktree.rs
@@ -216,6 +216,127 @@
         );
     }
 
+    /// A two-member virtual workspace (`a`, `b`) as its own committed git repository.
+    fn workspace_repo() -> tempfile::TempDir {
+        let tmp = tempfile::tempdir().unwrap();
+        let files = [
+            (
+                "Cargo.toml",
+                "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
+            ),
+            (
+                "a/Cargo.toml",
+                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
+            ),
+            ("a/src/lib.rs", "pub fn in_a() {}\n"),
+            (
+                "b/Cargo.toml",
+                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
+            ),
+            ("b/src/lib.rs", "pub fn in_b() {}\n"),
+        ];
+        for (name, contents) in files {
+            let path = tmp.path().join(name);
+            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
+            std::fs::write(path, contents).unwrap();
+        }
+        for args in [
+            vec!["init", "-q"],
+            vec!["add", "-A"],
+            vec![
+                "-c",
+                "user.email=t@t",
+                "-c",
+                "user.name=t",
+                "commit",
+                "-q",
+                "-m",
+                "init",
+            ],
+        ] {
+            Command::new("git")
+                .args(args)
+                .current_dir(tmp.path())
+                .status()
+                .unwrap();
+        }
+        tmp
+    }
+
+    #[test]
+    fn a_workspace_member_is_the_root_and_its_siblings_are_not_rewritten() {
+        let repo = workspace_repo();
+        let mut seen = Vec::new();
+        let wt = RewrittenWorktree::create_with(&repo.path().join("b"), |file, source| {
+            seen.push(file.to_string());
+            pub_to_pub_crate(file, source)
+        })
+        .unwrap();
+
+        // the closure sees paths relative to the PACKAGE, and only its files
+        assert_eq!(seen, vec!["src/lib.rs"]);
+        // `root` is the package directory inside the checkout...
+        assert!(wt.root.ends_with("b"), "{:?}", wt.root);
+        let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
+        assert!(rewritten.contains("pub(crate) fn in_b"));
+        // ...the sibling is checked out (it may be a path dependency) but untouched...
+        let sibling = std::fs::read_to_string(wt.root.join("../a/src/lib.rs")).unwrap();
+        // (git may check the file out with CRLF line endings on Windows)
+        assert!(
+            sibling.contains("pub fn in_a()") && !sibling.contains("pub(crate)"),
+            "{sibling:?}"
+        );
+        // ...and the caller's own tree is untouched too
+        let original = std::fs::read_to_string(repo.path().join("b/src/lib.rs")).unwrap();
+        assert_eq!(original, "pub fn in_b() {}\n");
+    }
+
+    #[test]
+    fn dropping_a_member_worktree_removes_the_whole_checkout_and_unregisters_it() {
+        let repo = workspace_repo();
+        let top = {
+            let wt =
+                RewrittenWorktree::create_with(&repo.path().join("b"), pub_to_pub_crate).unwrap();
+            wt.root.parent().unwrap().to_path_buf()
+        };
+        assert!(
+            !top.exists(),
+            "the whole worktree, not just the member, must be removed"
+        );
+        let output = Command::new("git")
+            .args(["worktree", "list", "--porcelain"])
+            .current_dir(repo.path())
+            .output()
+            .unwrap();
+        let listing = String::from_utf8_lossy(&output.stdout);
+        assert_eq!(
+            listing.matches("worktree ").count(),
+            1,
+            "dangling entry:\n{listing}"
+        );
+    }
+
+    #[test]
+    fn a_virtual_workspace_root_is_refused_with_the_members_and_leaves_nothing_behind() {
+        let repo = workspace_repo();
+        let err = RewrittenWorktree::create_with(repo.path(), pub_to_pub_crate)
+            .err()
+            .expect("must be refused")
+            .to_string();
+        assert!(err.contains("not a package directory"), "{err}");
+        let output = Command::new("git")
+            .args(["worktree", "list", "--porcelain"])
+            .current_dir(repo.path())
+            .output()
+            .unwrap();
+        let listing = String::from_utf8_lossy(&output.stdout);
+        assert_eq!(
+            listing.matches("worktree ").count(),
+            1,
+            "dangling entry:\n{listing}"
+        );
+    }
+
     #[test]
     fn the_worktree_directory_is_removed_when_dropped() {
         let tmp = tempfile::tempdir().unwrap();
````

  Run: `cargo test --lib worktree`
  Expected: FAIL — 3 of the 8 worktree tests (`a_workspace_member_is_the_root_and_its_siblings_are_not_rewritten`, `dropping_a_member_worktree_removes_the_whole_checkout_and_unregisters_it`, `a_virtual_workspace_root_is_refused_with_the_members_and_leaves_nothing_behind`); the five existing ones pass.

- [ ] **Step 2: Implement.**

````diff
--- a/src/worktree.rs
+++ b/src/worktree.rs
@@ -2,20 +2,29 @@
 use std::process::Command;
 
 pub struct RewrittenWorktree {
+    /// The PACKAGE's directory inside the checkout: the checkout's root for a repository with one
+    /// package at its top, `<checkout>/fuel-core` for a workspace member. Everything a check reads,
+    /// rewrites or builds is under it.
     pub root: PathBuf,
+    /// The whole checkout, which is what `git worktree remove` takes.
+    worktree_path: PathBuf,
     source_repo: PathBuf,
     _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
 }
 
 impl RewrittenWorktree {
-    /// Creates a detached worktree of `project_root`'s HEAD and rewrites every `.rs` file under its
-    /// `src/` with `rewrite(relative_path, source)`. `relative_path` is `src/...` with forward
-    /// slashes. Files are visited in sorted order so a stateful `rewrite` (one that hands out ids)
+    /// Creates a detached worktree of the git repository containing `project_root` at HEAD and
+    /// rewrites every `.rs` file under the PACKAGE's `src/` with `rewrite(relative_path, source)`.
+    /// `project_root` is the package's directory: the repository root, or a workspace member below
+    /// it (the whole repository is checked out, so path dependencies exist, but only the package is
+    /// rewritten). `relative_path` is `src/...`, relative to the package, with forward slashes. A
+    /// directory that is not a package (a virtual workspace root) is refused. Files are visited in sorted order so a stateful `rewrite` (one that hands out ids)
     /// is deterministic. A rewrite error drops the worktree again before returning.
     pub fn create_with<F>(project_root: &Path, mut rewrite: F) -> anyhow::Result<Self>
     where
         F: FnMut(&str, &str) -> anyhow::Result<String>,
     {
+        let prefix = crate::package::git_prefix(project_root)?;
         let scratch = tempfile::tempdir()?;
         let wt_path = scratch.path().join("wt");
 
@@ -30,16 +39,18 @@
         // Build the guard BEFORE rewriting: if a rewrite fails, `Drop` still unregisters the
         // worktree from the source repo instead of leaving a dangling `git worktree list` entry.
         let guard = Self {
-            root: wt_path.clone(),
+            root: wt_path.join(&prefix),
+            worktree_path: wt_path,
             source_repo: project_root.to_path_buf(),
             _scratch: scratch,
         };
-
-        let mut files = walk_rs_files(&wt_path.join("src"))?;
+        crate::package::require_package(&guard.root)?;
+
+        let mut files = walk_rs_files(&guard.root.join("src"))?;
         files.sort();
         for entry in files {
             let relative = entry
-                .strip_prefix(&wt_path)?
+                .strip_prefix(&guard.root)?
                 .to_string_lossy()
                 .replace('\\', "/");
             let source = std::fs::read_to_string(&entry)?;
@@ -58,7 +69,7 @@
         // finding: in a shared checkout that dangling entry is an unwanted write into a shared .git.
         let removed = Command::new("git")
             .args(["worktree", "remove", "--force"])
-            .arg(&self.root)
+            .arg(&self.worktree_path)
             .current_dir(&self.source_repo)
             .status()
             .map(|s| s.success())
@@ -66,7 +77,7 @@
         if !removed {
             // Source repo may itself be gone (e.g. a test's own tempdir already dropped) -- fall
             // back to a plain directory removal so we don't leak disk space either way.
-            let _ = std::fs::remove_dir_all(&self.root);
+            let _ = std::fs::remove_dir_all(&self.worktree_path);
         }
     }
 }
````

- [ ] **Step 3: Run the suite.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 122 lib tests (114 + 5 package + 3 worktree), every integration count unchanged.
  The unchanged integration suites passing is the proof that single-package repositories behave as before.

- [ ] **Step 4: Prove two guards can fail.** (a) Change `root: wt_path.join(&prefix),` to `root: wt_path.clone(),`: the member-worktree tests and (later) the CLI and workspace tests must FAIL. (b) Delete the `crate::package::require_package(&guard.root)?;` line: `a_virtual_workspace_root_is_refused_with_the_members_and_leaves_nothing_behind` must FAIL. Revert each.

- [ ] **Step 5: Commit** — `git add -A src && git commit -m "feat: the worktree's root is the package directory, the whole repository is checked out"` (+ trailers).

---

### Task 3: Diagnostics are package-relative and package-only

**Files:** Modify `src/cargo_json.rs`.

**Interfaces:**
- Consumes: `package::workspace_prefix`.
- Produces: `cargo_json::build_all_targets` / `build_lib_only` (signatures unchanged; `root` is the package directory) now return diagnostics whose span file names are package-relative, with every **non-error** diagnostic whose primary span is outside the package dropped; errors are kept wherever they are.

- [ ] **Step 1: Write the failing tests first.** Apply this diff to `src/cargo_json.rs`'s `mod tests` (a `workspace` fixture helper and two tests):

````diff
--- a/src/cargo_json.rs
+++ b/src/cargo_json.rs
@@ -245,6 +245,64 @@
         );
     }
 
+    fn workspace(a_lib: &str, b_lib: &str) -> tempfile::TempDir {
+        let tmp = tempfile::tempdir().unwrap();
+        let files = [
+            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n"),
+            ("a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
+            ("a/src/lib.rs", a_lib),
+            (
+                "b/Cargo.toml",
+                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\" }\n",
+            ),
+            ("b/src/lib.rs", b_lib),
+        ];
+        for (name, contents) in files {
+            let path = tmp.path().join(name);
+            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
+            std::fs::write(path, contents).unwrap();
+        }
+        tmp
+    }
+
+    #[test]
+    fn a_member_build_reports_package_relative_paths_and_only_its_own_diagnostics() {
+        // `a` warns too (it is built as b's dependency); that warning is not b's.
+        let ws = workspace(
+            "pub fn a_warn(x: i32) {}\n",
+            "pub fn b_warn(y: i32) { a::a_warn(1); }\n",
+        );
+        let out = build_all_targets(&ws.path().join("b"), CAP_LINTS).unwrap();
+        let unused: Vec<_> = out
+            .diagnostics
+            .iter()
+            .filter(|d| d.code.as_deref() == Some("unused_variables"))
+            .map(|d| {
+                let span = d.primary_span().unwrap();
+                (span.file.clone(), d.message.clone())
+            })
+            .collect();
+        // (`--all-targets` compiles the lib twice, so the same warning can appear twice)
+        assert!(!unused.is_empty(), "b's own warning must be reported");
+        assert!(
+            unused
+                .iter()
+                .all(|(file, message)| file == "src/lib.rs" && message.contains('y')),
+            "{unused:?}"
+        );
+    }
+
+    #[test]
+    fn a_dependency_that_does_not_compile_still_breaks_the_build() {
+        // Only WARNINGS from outside the package are dropped; an error anywhere must still count.
+        let ws = workspace(
+            "pub fn a_broken() { let x: i32 = \"no\"; }\n",
+            "pub fn b() {}\n",
+        );
+        let out = build_all_targets(&ws.path().join("b"), CAP_LINTS).unwrap();
+        assert!(out.is_broken(), "{:?}", out.diagnostics.len());
+    }
+
     #[test]
     fn non_message_lines_and_garbage_are_skipped() {
         let stdout = format!("{{\"reason\":\"compiler-artifact\"}}\nnot json\n{LINE}\n");
````

  Run: `cargo test --lib cargo_json`
  Expected: FAIL — `a_member_build_reports_package_relative_paths_and_only_its_own_diagnostics` (the sibling's warning and the `b/` prefix are still there); `a_dependency_that_does_not_compile_still_breaks_the_build` already passes — it is a guard that must keep passing.

- [ ] **Step 2: Implement.**

````diff
--- a/src/cargo_json.rs
+++ b/src/cargo_json.rs
@@ -148,10 +148,44 @@
         .env("RUSTFLAGS", compose_rustflags(&existing, extra_rustflags))
         .env_remove("CARGO_ENCODED_RUSTFLAGS")
         .output()?;
+    let diagnostics = parse_messages(&String::from_utf8_lossy(&output.stdout));
     Ok(BuildOutput {
-        diagnostics: parse_messages(&String::from_utf8_lossy(&output.stdout)),
+        diagnostics: relative_to_package(diagnostics, &crate::package::workspace_prefix(root)?),
         success: output.status.success(),
     })
+}
+
+/// cargo names files relative to the WORKSPACE root (`b/src/lib.rs` for a member `b`). Checks think
+/// in package-relative paths (`src/lib.rs`), so strip the member's prefix. A WARNING whose primary
+/// span is outside the package (a sibling member built as a dependency, a path dependency) is not
+/// the package's and is dropped; an error is kept wherever it is, because it breaks the build.
+fn relative_to_package(diagnostics: Vec<Diagnostic>, prefix: &str) -> Vec<Diagnostic> {
+    let inside = |file: &str| {
+        if prefix.is_empty() {
+            !file.starts_with("..") && !Path::new(file).is_absolute()
+        } else {
+            file.starts_with(prefix)
+        }
+    };
+    diagnostics
+        .into_iter()
+        .filter_map(|mut d| {
+            let outside = d
+                .spans
+                .iter()
+                .filter(|s| s.is_primary)
+                .any(|s| !inside(&s.file));
+            if outside && d.level != "error" {
+                return None;
+            }
+            for span in &mut d.spans {
+                if let Some(rest) = span.file.strip_prefix(prefix) {
+                    span.file = rest.to_string();
+                }
+            }
+            Some(d)
+        })
+        .collect()
 }
 
 /// Parses cargo's JSON stream (one object per line), keeping only `compiler-message` entries.
````

- [ ] **Step 3: Run the suite.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 124 lib tests (122 + 2), every integration count unchanged.

- [ ] **Step 4: Prove two guards can fail.** (a) Pass `""` instead of `&crate::package::workspace_prefix(root)?` to `relative_to_package`: the member-diagnostics unit test fails (and, from Task 4, the CLI/workspace tests). (b) Change `if outside && d.level != "error" {` to `if false {`: `a_member_build_reports_package_relative_paths_and_only_its_own_diagnostics` must FAIL. Revert each.

- [ ] **Step 5: Commit** — `git add -A src && git commit -m "feat: build diagnostics are package-relative and package-only"` (+ trailers).

---

### Task 4: End-to-end tests, CLI, docs, acceptance

**Files:** Create `tests/workspace.rs`; modify `tests/cli.rs`, `README.md`.

- [ ] **Step 1: Write the end-to-end tests.** They pass once Tasks 1–3 are in; their value is that they FAIL without them — to see it, copy `tests/workspace.rs` (and `tests/common/`) onto a checkout of the base commit and run `cargo test --test workspace`: all 7 `*_member*`/`*_workspace*` tests FAIL there (measured at plan time: 7 of 7). Create `tests/workspace.rs`:

````rust
// tests/workspace.rs
//! The three checks on a workspace MEMBER: the project is the member's directory, findings are about
//! that package only and carry package-relative paths, and the allowlist is the member's own.

mod common;

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::{ReachabilityCheck, UnusedParametersCheck, UnusedReturnValuesCheck};
use coderipper::finding::Finding;
use common::{git_repo_with, MANIFEST};
use std::path::Path;

const WORKSPACE: &str = "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n";

fn package(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
}

const A_LIB: &str = "\
pub fn used_by_b() -> i32 { 1 }
pub fn dead_in_a() -> i32 { 2 }
pub fn a_unused_param(p: i32) {}
pub fn a_discards() -> i32 { 3 }
pub fn a_caller() { a_discards(); }
";

const B_LIB: &str = "\
pub fn b_fn() -> i32 { a::used_by_b() }
pub fn b_dead(unused_b: i32) -> i32 { 3 }
pub fn b_helper() -> i32 { 4 }
pub fn b_discarder() { b_helper(); }
";

fn workspace_repo(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let b_manifest = package("b", "\n[dependencies]\na = { path = \"../a\" }\n");
    let mut files = vec![
        ("Cargo.toml", WORKSPACE.to_string()),
        ("a/Cargo.toml", package("a", "")),
        ("a/src/lib.rs", A_LIB.to_string()),
        ("b/Cargo.toml", b_manifest),
        ("b/src/lib.rs", B_LIB.to_string()),
    ];
    files.extend(extra.iter().map(|(n, c)| (*n, c.to_string())));
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
    git_repo_with(&borrowed)
}

fn run(check: &dyn Check, project: &Path) -> anyhow::Result<Vec<Finding>> {
    check.run(&CheckContext {
        project_root: project.to_path_buf(),
        portfolio_root: project.to_path_buf(),
    })
}

fn subjects(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

fn files(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings
        .iter()
        .filter_map(|f| f.location.as_ref().map(|l| l.file.clone()))
        .collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn unused_parameters_on_a_member_reports_that_package_only_with_package_relative_paths() {
    let repo = workspace_repo(&[]);
    let b = run(&UnusedParametersCheck, &repo.path().join("b")).unwrap();
    assert_eq!(subjects(&b), vec!["b_dead::unused_b"]);
    assert_eq!(files(&b), vec!["src/lib.rs"]);
    // the sibling is analyzed on its own, not as a side effect
    let a = run(&UnusedParametersCheck, &repo.path().join("a")).unwrap();
    assert_eq!(subjects(&a), vec!["a_unused_param::p"]);
}

#[test]
fn unused_return_values_on_a_member_reports_that_package_only() {
    let repo = workspace_repo(&[]);
    let b = run(&UnusedReturnValuesCheck, &repo.path().join("b")).unwrap();
    assert_eq!(subjects(&b), vec!["b_helper"]);
    assert_eq!(files(&b), vec!["src/lib.rs"]);
    let a = run(&UnusedReturnValuesCheck, &repo.path().join("a")).unwrap();
    assert_eq!(subjects(&a), vec!["a_discards"]);
}

#[test]
fn reachability_on_a_member_reports_that_package_only() {
    let repo = workspace_repo(&[]);
    let b = run(&ReachabilityCheck, &repo.path().join("b")).unwrap();
    // nothing in b calls these (b has no bin or test), and `dead_in_a` etc. are not b's to report
    assert_eq!(
        subjects(&b),
        vec!["b_dead", "b_discarder", "b_fn", "b_helper"]
    );
    assert_eq!(files(&b), vec!["src/lib.rs"]);
}

#[test]
fn a_project_scope_check_does_not_count_a_sibling_members_use() {
    // `used_by_b` is called by member b, but this check's scope is one package: for `a` it has zero
    // callers. (Whether another crate in the portfolio uses a pub item is the portfolio-scope pass.)
    let repo = workspace_repo(&[]);
    let a = run(&ReachabilityCheck, &repo.path().join("a")).unwrap();
    assert!(
        subjects(&a).contains(&"used_by_b".to_string()),
        "{:?}",
        subjects(&a)
    );
}

#[test]
fn a_virtual_workspace_root_is_refused_by_every_check_and_names_the_members() {
    let repo = workspace_repo(&[]);
    for check in [
        &ReachabilityCheck as &dyn Check,
        &UnusedParametersCheck,
        &UnusedReturnValuesCheck,
    ] {
        let err = run(check, repo.path()).unwrap_err().to_string();
        assert!(
            err.contains("not a package directory"),
            "{} said: {err}",
            check.id()
        );
        assert!(err.contains('a') && err.contains('b'), "{err}");
    }
}

#[test]
fn the_allowlist_is_the_members_own() {
    let repo = workspace_repo(&[(
        "b/.coderipper.toml",
        "[[allow]]\ncheck = \"unused-parameters\"\nfile = \"src/lib.rs\"\nsymbol = \"b_dead::unused_b\"\nreason = \"kept for the next release\"\n",
    )]);
    let result = coderipper::run_checks(
        &CheckContext {
            project_root: repo.path().join("b"),
            portfolio_root: repo.path().to_path_buf(),
        },
        Tier::Fast,
        Some("unused-parameters"),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn a_workspace_root_that_is_itself_a_package_is_analyzed_as_that_package() {
    let manifest = format!("{MANIFEST}\n[workspace]\nmembers = [\"sub\"]\n");
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        ("src/lib.rs", "pub fn root_fn(unused_root: i32) {}\n"),
        ("sub/Cargo.toml", &package("sub", "")),
        ("sub/src/lib.rs", "pub fn sub_fn(unused_sub: i32) {}\n"),
    ]);
    let root = run(&UnusedParametersCheck, repo.path()).unwrap();
    assert_eq!(subjects(&root), vec!["root_fn::unused_root"]);
    let sub = run(&UnusedParametersCheck, &repo.path().join("sub")).unwrap();
    assert_eq!(subjects(&sub), vec!["sub_fn::unused_sub"]);
}

#[test]
fn a_workspace_below_the_repository_root_uses_both_prefixes_correctly() {
    // The git repository is the monorepo (`proj/` is one cargo workspace inside it), so the member's
    // path in the CHECKOUT (`proj/m`) differs from its path in the WORKSPACE (`m`), which is the base
    // cargo uses for diagnostic file names.
    let repo = git_repo_with(&[
        (
            "proj/Cargo.toml",
            "[workspace]\nmembers = [\"m\"]\nresolver = \"2\"\n",
        ),
        ("proj/m/Cargo.toml", &package("m", "")),
        ("proj/m/src/lib.rs", "pub fn f(unused_m: i32) {}\n"),
        ("other/README.md", "unrelated sibling directory\n"),
    ]);
    let found = run(&UnusedParametersCheck, &repo.path().join("proj/m")).unwrap();
    assert_eq!(subjects(&found), vec!["f::unused_m"]);
    assert_eq!(files(&found), vec!["src/lib.rs"]);
}
````

- [ ] **Step 2: Add the CLI tests and the README section.**

````diff
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -26,6 +26,16 @@ never used) and `unused-return-values` (a function whose
 return value every caller discards). See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
 `docs/superpowers/plans/` for what's actually being built and in what order.
 
+## Workspaces
+
+A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
+root that is itself a package. A virtual workspace root is refused with the list of members to choose from.
+The whole repository is checked out in a throwaway worktree (so path dependencies resolve) but only the
+member is rewritten and built, findings carry paths relative to the member, and `.coderipper.toml` is read
+from the member's directory. The checks are project-scope: a `pub` item used only by a sibling member has no
+callers within this package. Each run builds the member's dependencies from scratch (about two minutes for a
+large crate); running every member in one command is not supported yet.
+
 ## Reachability on a package with a library
 
 `reachability` builds only the library when there is one, then does not report an item that a bin, test,
diff --git a/tests/cli.rs b/tests/cli.rs
--- a/tests/cli.rs
+++ b/tests/cli.rs
@@ -321,3 +321,77 @@ fn the_unused_parameters_check_runs_by_id_and_reports_a_parameter() {
         ))
         .stdout(predicate::str::contains("(unused-parameters)"));
 }
+
+fn workspace_fixture() -> tempfile::TempDir {
+    let tmp = tempfile::tempdir().unwrap();
+    let files = [
+        (
+            "Cargo.toml",
+            "[workspace]\nmembers = [\"one\", \"two\"]\nresolver = \"2\"\n",
+        ),
+        (
+            "one/Cargo.toml",
+            "[package]\nname = \"one\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
+        ),
+        ("one/src/lib.rs", "pub fn f(unused_one: i32) {}\n"),
+        (
+            "two/Cargo.toml",
+            "[package]\nname = \"two\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
+        ),
+        ("two/src/lib.rs", "pub fn g(unused_two: i32) {}\n"),
+    ];
+    for (name, contents) in files {
+        let path = tmp.path().join(name);
+        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
+        std::fs::write(path, contents).unwrap();
+    }
+    for args in [
+        vec!["init", "-q"],
+        vec!["add", "-A"],
+        vec![
+            "-c",
+            "user.email=t@t",
+            "-c",
+            "user.name=t",
+            "commit",
+            "-q",
+            "-m",
+            "init",
+        ],
+    ] {
+        StdCommand::new("git")
+            .args(args)
+            .current_dir(tmp.path())
+            .status()
+            .unwrap();
+    }
+    tmp
+}
+
+#[test]
+fn a_workspace_member_can_be_the_project() {
+    let ws = workspace_fixture();
+    Command::cargo_bin("coderipper")
+        .unwrap()
+        .args(["check", "unused-parameters", "--project"])
+        .arg(ws.path().join("two"))
+        .assert()
+        .success()
+        .stdout(predicate::str::contains(
+            "parameter `unused_two` of `g` is never used",
+        ))
+        .stdout(predicate::str::contains("unused_one").not());
+}
+
+#[test]
+fn a_virtual_workspace_root_asks_for_a_member() {
+    let ws = workspace_fixture();
+    Command::cargo_bin("coderipper")
+        .unwrap()
+        .args(["check", "unused-parameters", "--project"])
+        .arg(ws.path())
+        .assert()
+        .failure()
+        .stderr(predicate::str::contains("not a package directory"))
+        .stderr(predicate::str::contains("one, two").or(predicate::str::contains("two, one")));
+}
````

- [ ] **Step 3: Run everything.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 124 lib, 8 allowlist, 10 cli, 20 reachability, 11 unused-parameters, 13 unused-return-values, 8 workspace.

- [ ] **Step 4: Lint.** Run: `cargo clippy --all-targets -- -D warnings` (no output; the `dead_code` from Task 1 is gone).

- [ ] **Step 5: Acceptance on a real workspace (read-only).** `git clone --no-hardlinks <path to fuel> /tmp/fuelclone`, build this branch, then for each check
  `<built-binary> check <id> --project /tmp/fuelclone/fuel-core`.
  Expected (measured at plan time; `fuel` at `d1bfe127`): `reachability` 102 findings, `unused-parameters` "no issues found", `unused-return-values` 2 findings
  (`remove`: every one of its 15 call sites; `refresh_decode_session`: 1 of 2); each exits 0 and takes roughly 2 minutes (a cold dependency build).
  Then `<built-binary> check reachability --project /tmp/fuelclone` (the virtual workspace root).
  Expected: a non-zero exit and `... is not a package directory ... Workspace members: fuel-core, ...` on stderr.

- [ ] **Step 6: Commit, push, report.** `git add -A tests README.md && git commit -m "test+docs: workspace members as projects -- end-to-end tests, CLI, README"` (+ trailers), push, and report to the PM (who opens and merges the PR; note it is stacked on #11 until that merges). Read the PR's checks and **unresolved review threads** (GraphQL `reviewThreads{isResolved}`) before calling it READY.
