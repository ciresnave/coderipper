# Unused-return-values check (project scope, Rust) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CodeRipper's second real check: for each Rust function that returns a value, say whether
that value is discarded at *every* call site in the crate (a dead return value) or only at *some*.

**Architecture:** In a throwaway git worktree, tag every eligible function with
`#[must_use = "CR:<id>"]` and `#[deprecated(note = "CR:<id>")]`, build with
`cargo build --all-targets --message-format=json`, and join two rustc diagnostics on `<id>`:
`unused_must_use` (a call whose value was discarded) and `deprecated` (any use at all). Ignored ÷ total
decides the finding. No visibility rewrite is needed, so — unlike the reachability check — this works on a
package whose lib is consumed by its own bin or tests. It reuses the reachability check's worktree
lifecycle, its allowlist, and its per-run-sentinel pattern (the first and second are promoted to shared
modules in Task 1).

**Tech Stack:** Rust 2021. New dependencies: `syn` (parse source to find function items and their exact
positions) and `proc-macro2` with `span-locations` (line/column of those positions). Add both with
`cargo add` so they land on the latest release (CireSnave's standing rule: latest dependency versions) —
verified against `syn 3.0.6` / `proc-macro2 1.0.107`.

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §5 (`unused-return-values`) and §4
(allowlist). **This plan deliberately departs from §5's mechanism text in the ways listed under
"Corrections to the spec" below; Task 6 amends §5 to match.**

## Corrections to the spec (found by testing on rustc 1.98.1, not by reading)

Everything here was reproduced on a scratch crate, then again by the code in this plan.

1. **`unused_must_use` alone cannot tell "ignored at every call site" from "ignored at some".** It fires
   only at call sites whose value is discarded; used sites and never-called functions are silent. §5's
   "a mix of used/unused call sites is a weaker finding" is therefore not derivable from it. **Fix:** pair
   it with `#[deprecated]`, which fires at every use. `ignored == total` ⇒ all-ignored; `ignored < total` ⇒ mixed.
2. **No `pub` → `pub(crate)` rewrite is needed**, so this check does not inherit the reachability check's
   lib+bin `E0603` limitation (which hits CodeRipper's own repo). Verified: it analyzes a lib consumed by
   its own bin, and produced a correct finding when run against CodeRipper's own merged repo.
3. **Attribution needs a unique tag.** rustc's message names the function only as `f` or `S::m`. The
   `#[must_use]` reason string comes back as a child note of the diagnostic and `#[deprecated]`'s `note`
   is the tail of its message, so `CR:<id>` maps every hit to one definition.
4. **Unit-returning functions DO trigger `unused_must_use` once annotated** — they must be skipped, or
   every call to every `fn f()` is a finding. `Result`-returning functions are skipped too (ignoring one
   is already a default-on rustc warning), as is `-> &mut Self` (builder chaining, idiomatically ignored).
5. **`use` statements inflate "total".** rustc reports `deprecated` at every imported name (single-line,
   multi-line `use a::{ b, };`, and `pub use` alike; glob imports don't). Without correcting for it every
   function imported anywhere looks "mixed". The rewriter records each `use` item's line range and the
   classifier drops hits inside it.
6. **Self-recursion skews the counts.** rustc skips `deprecated` for a function's use of itself but still
   reports `unused_must_use`, so a function that discards its own recursive result would read "1 ignored of
   0 uses". Both signals are dropped inside the function's own line range.
7. **`syn` 3 differs from `syn` 2**: `ImplItemFn::defaultness` moved to `f.modifiers.defaultness`. The
   code below is verified against syn 3.

## Global Constraints

- Rust edition 2021; CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build --all-targets`, `cargo test --no-fail-fast` on
  **ubuntu, windows and macos**. Every task ends with all four passing locally.
- **A test must never point a check at the shared checkout** — checks run `git worktree add`, a write
  into the repo's `.git`. Every test builds a throwaway fixture in a tempdir and commits it to its own
  fresh `git init` repo (the reachability tests already follow this rule; see `tests/common/mod.rs`).
- Never mutate the caller's real working tree. All rewriting happens in the throwaway worktree, removed
  (and unregistered from the source repo) on drop — including when a rewrite fails.
- A finding that claims an absence ("never consumed") must carry a `positive_control`; the host refuses
  to emit it otherwise (`Finding::validate`). A run whose control fails must return `Err`, never `Ok(vec![])`
  — an empty result must always mean "genuinely clean".
- Do **not** bump any version in `Cargo.toml`: the portfolio PM allocates the version number at gate
  time (a per-PR bump doesn't compose across parallel PRs).
- No new third-party code is copied in; the only new code dependencies are `syn` and `proc-macro2`
  from crates.io (provenance rule).
- If you write Rust containing a backslash character literal (`'\\'`) via a shell heredoc on this Windows
  box, the backslash can be silently dropped — create such files with the editor/Write tool instead.
- Commit trailers (this lane): end each commit message with
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. Do not run `gh auth switch`.

## Review Focus

Input classes the spec is silent on that are most likely to bite a person using this tool. Each has a
named test in the task that owns the code.

1. **A function imported with `use`** (single-line, multi-line, `pub use`) must not count the import as a
   call site, or "discarded at every call site" can never be reported for anything used across modules.
   → `an_import_is_not_a_call_site` (Task 4), `it_separates_always_ignored_from_sometimes_ignored_on_a_lib_plus_bin_package` (Task 5), `use_statements_are_recorded_including_multiline_ones` (Task 3).
2. **A function whose only discarded call is its own recursion.** → `a_recursive_call_inside_the_functions_own_body_is_neither_ignored_nor_used` (Task 4), `a_function_that_only_discards_its_own_recursive_result_is_not_flagged` (Task 5).
3. **`let _ = f();` / `_ = f();`** is a deliberate discard: a use, not a finding. → `a_deliberate_let_underscore_discard_counts_as_a_use_not_an_ignore` (Task 5).
4. **A crate that silences one of the two lints** (`#![allow(deprecated)]`, `#![allow(unused_must_use)]`) must make the run fail loudly, not report clean; and `#![deny(warnings)]` (which turns every lint into an error) must neither hide findings nor read as a broken build. → Task 6 tests.
5. **Source the line-based approach would corrupt**: non-ASCII text before a function on the same line, CRLF line endings, two functions on one line. → `columns_are_characters_not_bytes_and_crlf_is_preserved`, `two_functions_on_one_line_are_both_tagged` (Task 3), `a_file_with_non_ascii_text_and_crlf_endings_is_analyzed_correctly` (Task 6).

**Known gaps accepted for v1 (documented in the module doc, not tested):** functions defined inside a
`macro_rules!` body are never tagged; only `src/` is rewritten (calls from `tests/`, `examples/`,
`benches/` still count as uses); a workspace root without its own `src/` errors out; analysis is
HEAD-only; §4's "stale allowlist entry → informational finding" mechanism is a separate piece of work
and is not part of this plan.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/worktree.rs` (moved from `src/checks/reachability/worktree.rs`) | Throwaway worktree, now generic over the per-file rewrite |
| `src/allowlist.rs` (moved from `src/checks/reachability/allowlist.rs`) | `.coderipper.toml` suppression, shared by every check |
| `src/cargo_json.rs` (new) | Run `cargo build --all-targets --message-format=json`, parse diagnostics with line **and column** and child notes |
| `src/checks/unused_return_values/rewriter.rs` (new) | `syn`-based tagging of eligible functions; records each function's and each `use` item's line range |
| `src/checks/unused_return_values/classify.rs` (new) | Join `unused_must_use` + `deprecated` by tag into per-function ignored/total counts |
| `src/checks/unused_return_values/sentinel.rs` (new) | Per-run positive control |
| `src/checks/unused_return_values/mod.rs` (new) | The `Check` impl, finding construction |
| `tests/common/mod.rs`, `tests/unused_return_values.rs` (new) | End-to-end fixtures |

`src/checks/reachability/diagnostics.rs` is **not** touched: it has its own dead-code-specific parser.
Migrating it onto `cargo_json` is a reasonable follow-up, deliberately left out so this PR can't regress M1.

Work on a branch from a **fresh** `origin/main` (`git fetch origin` first): `feat/unused-return-values`.
Baseline before starting, to compare against: `cargo test` shows 31 lib unit tests, 4 in `tests/cli.rs`,
4 in `tests/reachability.rs`, all passing.

---

### Task 1: Share the worktree and allowlist (refactor, no behaviour change)

**Files:**
- Move: `src/checks/reachability/worktree.rs` → `src/worktree.rs`
- Move: `src/checks/reachability/allowlist.rs` → `src/allowlist.rs`
- Modify: `src/lib.rs`, `src/checks/reachability/mod.rs`

**Interfaces:**
- Produces: `crate::worktree::RewrittenWorktree::create_with(project_root: &Path, rewrite: impl FnMut(&str, &str) -> anyhow::Result<String>) -> anyhow::Result<RewrittenWorktree>` (fields/`Drop` unchanged; `.root: PathBuf`). The closure receives `(relative_path, source)` — `relative_path` is `src/...` with forward slashes — and returns the new source. Files are visited in sorted order.
- Produces: `crate::allowlist::Allowlist` with `load(&Path)` and `is_allowed(check_id, file, symbol)` — identical API, new path.

- [ ] **Step 1: Move the files**

```bash
git mv src/checks/reachability/worktree.rs src/worktree.rs
git mv src/checks/reachability/allowlist.rs src/allowlist.rs
```

- [ ] **Step 2: Replace `src/worktree.rs` with this version**

It differs from the old file only in: `create` → `create_with` taking the rewrite as a closure; the guard being
built *before* rewriting (so a failing rewrite still unregisters the worktree); sorted traversal; the removed
`use super::rewriter...` import; and two new tests. The three old tests now call `create_with`.

```rust
// src/worktree.rs
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RewrittenWorktree {
    pub root: PathBuf,
    source_repo: PathBuf,
    _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
}

impl RewrittenWorktree {
    /// Creates a detached worktree of `project_root`'s HEAD and rewrites every `.rs` file under its
    /// `src/` with `rewrite(relative_path, source)`. `relative_path` is `src/...` with forward
    /// slashes. Files are visited in sorted order so a stateful `rewrite` (one that hands out ids)
    /// is deterministic. A rewrite error drops the worktree again before returning.
    pub fn create_with<F>(project_root: &Path, mut rewrite: F) -> anyhow::Result<Self>
    where
        F: FnMut(&str, &str) -> anyhow::Result<String>,
    {
        let scratch = tempfile::tempdir()?;
        let wt_path = scratch.path().join("wt");

        let status = Command::new("git")
            .args(["worktree", "add", "--detach", "-q"])
            .arg(&wt_path)
            .arg("HEAD")
            .current_dir(project_root)
            .status()?;
        anyhow::ensure!(status.success(), "git worktree add failed");

        // Build the guard BEFORE rewriting: if a rewrite fails, `Drop` still unregisters the
        // worktree from the source repo instead of leaving a dangling `git worktree list` entry.
        let guard = Self {
            root: wt_path.clone(),
            source_repo: project_root.to_path_buf(),
            _scratch: scratch,
        };

        let mut files = walk_rs_files(&wt_path.join("src"))?;
        files.sort();
        for entry in files {
            let relative = entry
                .strip_prefix(&wt_path)?
                .to_string_lossy()
                .replace('\\', "/");
            let source = std::fs::read_to_string(&entry)?;
            std::fs::write(&entry, rewrite(&relative, &source)?)?;
        }

        Ok(guard)
    }
}

impl Drop for RewrittenWorktree {
    fn drop(&mut self) {
        // `git worktree remove` (run from the SOURCE repo, which we now keep a handle to) both
        // deletes the directory AND unregisters it -- unlike a bare rm -rf, which leaves a dangling
        // `git worktree list` entry in the source repo that accumulates across runs. Reviewed
        // finding: in a shared checkout that dangling entry is an unwanted write into a shared .git.
        let removed = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.root)
            .current_dir(&self.source_repo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !removed {
            // Source repo may itself be gone (e.g. a test's own tempdir already dropped) -- fall
            // back to a plain directory removal so we don't leak disk space either way.
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn walk_rs_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs_files(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::RewrittenWorktree;
    use std::process::Command;

    fn pub_to_pub_crate(_file: &str, source: &str) -> anyhow::Result<String> {
        Ok(source.replace("pub fn", "pub(crate) fn"))
    }

    fn init_fixture_repo(dir: &std::path::Path) {
        Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(dir)
            .status()
            .unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn dead() {}\n").unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[test]
    fn the_worktree_is_a_rewritten_copy_not_a_mutation_of_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();

        let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
        assert!(rewritten.contains("pub(crate) fn dead"));

        let original = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert_eq!(
            original, "pub fn dead() {}\n",
            "the caller's real tree must be untouched"
        );
    }

    #[test]
    fn the_rewrite_closure_gets_forward_slash_relative_paths_in_sorted_order() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());
        std::fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "").unwrap();
        std::fs::write(tmp.path().join("src/sub/z.rs"), "").unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "more",
            ])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut seen = Vec::new();
        let _wt = RewrittenWorktree::create_with(tmp.path(), |file, source| {
            seen.push(file.to_string());
            Ok(source.to_string())
        })
        .unwrap();

        assert_eq!(seen, vec!["src/a.rs", "src/lib.rs", "src/sub/z.rs"]);
    }

    #[test]
    fn a_failing_rewrite_still_unregisters_the_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let result = RewrittenWorktree::create_with(tmp.path(), |_, _| anyhow::bail!("boom"));
        assert!(result.is_err());

        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "dangling entry:
{listing}"
        );
    }

    #[test]
    fn the_worktree_directory_is_removed_when_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let wt_root = {
            let wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();
            wt.root.clone()
        }; // dropped here

        assert!(!wt_root.exists(), "worktree dir must be cleaned up on drop");
    }

    #[test]
    fn dropping_also_unregisters_the_worktree_from_the_source_repo() {
        // Review finding: only deleting the directory leaves a dangling `git worktree list` entry
        // in the SOURCE repo, which accumulates across runs and, in a shared checkout, is a write
        // into a shared .git nobody asked for. `git worktree remove` (not just rm -rf) is required.
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        {
            let _wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();
        } // dropped here

        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        // Only the source repo's own primary worktree should remain listed.
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "expected only the source repo's own entry, got:\n{listing}"
        );
    }
}
```

- [ ] **Step 3: Wire the modules in `src/lib.rs`**

Add the two module declarations (keep the rest of the file as is):

```rust
pub(crate) mod allowlist;
pub mod check;
pub mod checks;
pub mod finding;
pub(crate) mod worktree;
```

- [ ] **Step 4: Point the reachability check at the shared modules**

In `src/checks/reachability/mod.rs`: delete the lines `mod allowlist;` and `mod worktree;`, and make the
`use` block read:

```rust
use crate::allowlist::Allowlist;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use diagnostics::collect_dead_code;
use rewriter::rewrite_pub_to_pub_crate;
use sentinel::{inject_sentinel, SENTINEL_SYMBOL};
```

and replace the first line of `run`:

```rust
        let wt = RewrittenWorktree::create_with(&ctx.project_root, |_, source| {
            Ok(rewrite_pub_to_pub_crate(source))
        })?;
```

- [ ] **Step 5: Run everything — behaviour must be unchanged**

Run: `cargo fmt && cargo test`
Expected: PASS — 33 lib unit tests (31 + the 2 new worktree tests), 4 in `tests/cli.rs`, 4 in `tests/reachability.rs`. If any pre-existing test fails, the refactor changed behaviour: stop and fix before going on.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: no output.

```bash
git add -A src
git commit -m "refactor: share the rewritten-worktree and allowlist modules across checks"
```

---

### Task 2: Cargo diagnostics parser

**Files:**
- Create: `src/cargo_json.rs`
- Modify: `src/lib.rs` (add `pub(crate) mod cargo_json;`)

**Interfaces:**
- Produces: `crate::cargo_json::{Span, Diagnostic, BuildOutput, build_all_targets, parse_messages}`.
  `Diagnostic { code: Option<String>, level: String, message: String, notes: Vec<String>, spans: Vec<Span> }`,
  `Diagnostic::primary_span() -> Option<&Span>`, `Diagnostic::is_hard_error() -> bool` (an `error` that is
  not a lint escalated by `deny`: rustc error codes look like `E0603`, lint names are lowercase, a syntax
  error has no code). `Span { file: String /* forward slashes */, line: u32, column: u32, is_primary: bool }`.
  `BuildOutput { diagnostics: Vec<Diagnostic>, success: bool }` with `is_broken() -> bool`.
  `build_all_targets(root: &Path) -> anyhow::Result<BuildOutput>`.

- [ ] **Step 1: Create `src/cargo_json.rs`**

```rust
// src/cargo_json.rs
//! Runs `cargo build --all-targets --message-format=json` and parses rustc's diagnostics, keeping
//! the parts a check needs: the lint code, the primary span's line AND column, and the text of each
//! direct child note (where a `#[must_use = "..."]` reason string shows up).

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// Forward-slash path, relative to the directory cargo ran in.
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Diagnostic {
    pub code: Option<String>,
    pub level: String,
    pub message: String,
    /// `message` of each direct child (notes, helps) in order.
    pub notes: Vec<String>,
    pub spans: Vec<Span>,
}

impl Diagnostic {
    pub fn primary_span(&self) -> Option<&Span> {
        self.spans.iter().find(|s| s.is_primary)
    }

    /// An `error` that is not just a lint promoted by `#![deny(...)]`/`-D warnings`: rustc error
    /// codes look like `E0603`; lint names are lowercase snake_case; a syntax error has no code.
    pub fn is_hard_error(&self) -> bool {
        self.level == "error"
            && match &self.code {
                None => true,
                Some(code) => {
                    code.starts_with('E') && code[1..].chars().all(|c| c.is_ascii_digit())
                }
            }
    }
}

pub(crate) struct BuildOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub success: bool,
}

impl BuildOutput {
    /// True when this build can't be trusted to have reported what a check looked for: a real
    /// compiler error, or a non-zero exit with no diagnostic at all (e.g. a panicking build.rs,
    /// which writes plain text to stderr rather than rustc JSON).
    pub fn is_broken(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_hard_error)
            || (!self.success && self.diagnostics.is_empty())
    }
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<RawDiagnostic>,
}

#[derive(Deserialize)]
struct RawDiagnostic {
    code: Option<RawCode>,
    level: String,
    message: String,
    spans: Vec<RawSpan>,
    children: Vec<RawChild>,
}

#[derive(Deserialize)]
struct RawCode {
    code: String,
}

#[derive(Deserialize)]
struct RawChild {
    message: String,
}

#[derive(Deserialize)]
struct RawSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
}

pub(crate) fn build_all_targets(root: &Path) -> anyhow::Result<BuildOutput> {
    let output = Command::new("cargo")
        .args(["build", "--all-targets", "--message-format=json"])
        .current_dir(root)
        .output()?;
    Ok(BuildOutput {
        diagnostics: parse_messages(&String::from_utf8_lossy(&output.stdout)),
        success: output.status.success(),
    })
}

/// Parses cargo's JSON stream (one object per line), keeping only `compiler-message` entries.
pub(crate) fn parse_messages(stdout: &str) -> Vec<Diagnostic> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-message")
        .filter_map(|m| m.message)
        .map(|raw| Diagnostic {
            code: raw.code.map(|c| c.code),
            level: raw.level,
            message: raw.message,
            notes: raw.children.into_iter().map(|c| c.message).collect(),
            spans: raw
                .spans
                .into_iter()
                .map(|s| Span {
                    file: s.file_name.replace('\\', "/"),
                    line: s.line_start,
                    column: s.column_start,
                    is_primary: s.is_primary,
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"reason":"compiler-message","message":{"code":{"code":"unused_must_use","explanation":null},"level":"warning","message":"unused return value of `f` that must be used","spans":[{"file_name":"src\\lib.rs","line_start":7,"column_start":5,"is_primary":true}],"children":[{"message":"CR:3"},{"message":"use `let _ = ...` to ignore the resulting value"}]}}"#;

    #[test]
    fn a_compiler_message_keeps_code_span_column_and_child_notes() {
        let parsed = parse_messages(LINE);
        assert_eq!(parsed.len(), 1);
        let d = &parsed[0];
        assert_eq!(d.code.as_deref(), Some("unused_must_use"));
        assert_eq!(d.notes[0], "CR:3");
        let span = d.primary_span().unwrap();
        assert_eq!(
            (span.file.as_str(), span.line, span.column),
            ("src/lib.rs", 7, 5)
        );
    }

    #[test]
    fn non_message_lines_and_garbage_are_skipped() {
        let stdout = format!("{{\"reason\":\"compiler-artifact\"}}\nnot json\n{LINE}\n");
        assert_eq!(parse_messages(&stdout).len(), 1);
    }

    fn diag(level: &str, code: Option<&str>) -> Diagnostic {
        Diagnostic {
            code: code.map(str::to_string),
            level: level.into(),
            message: String::new(),
            notes: vec![],
            spans: vec![],
        }
    }

    #[test]
    fn a_denied_lint_is_not_a_hard_error_but_a_real_error_code_is() {
        assert!(!diag("error", Some("deprecated")).is_hard_error());
        assert!(!diag("warning", None).is_hard_error());
        assert!(diag("error", Some("E0603")).is_hard_error());
        assert!(diag("error", None).is_hard_error());
    }

    #[test]
    fn a_failed_build_with_no_diagnostics_is_broken() {
        let out = BuildOutput {
            diagnostics: vec![],
            success: false,
        };
        assert!(out.is_broken());
        let ok = BuildOutput {
            diagnostics: vec![],
            success: true,
        };
        assert!(!ok.is_broken());
    }
}
```

- [ ] **Step 2: Declare the module** — in `src/lib.rs` add `pub(crate) mod cargo_json;` after `pub(crate) mod allowlist;`.

- [ ] **Step 3: Run the new tests**

Run: `cargo test cargo_json`
Expected: PASS, 4 tests (`a_compiler_message_keeps_code_span_column_and_child_notes`, `non_message_lines_and_garbage_are_skipped`, `a_denied_lint_is_not_a_hard_error_but_a_real_error_code_is`, `a_failed_build_with_no_diagnostics_is_broken`). Expect `dead_code` warnings for the not-yet-used items; they go away in Task 5.

- [ ] **Step 4: Prove a test can fail** — temporarily change `starts_with('E')` to `starts_with('X')`; `a_denied_lint_is_not_a_hard_error_but_a_real_error_code_is` must FAIL. Revert.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt
git add -A src
git commit -m "feat: cargo --message-format=json parser keeping lint code, span column and child notes"
```

---

### Task 3: The tagging rewriter

**Files:**
- Create: `src/checks/unused_return_values/rewriter.rs`, `src/checks/unused_return_values/mod.rs` (stub for now)
- Modify: `src/checks/mod.rs`, `Cargo.toml`

**Interfaces:**
- Produces (all in `checks::unused_return_values::rewriter`): `FnTag { id: u32, file: String, name: String, line: u32, last_line: u32 }`; `UseRange { file: String, first_line: u32, last_line: u32 }`; `Annotated { source: String, tags: Vec<FnTag>, use_ranges: Vec<UseRange> }`; `annotate(file: &str, source: &str, next_id: &mut u32) -> anyhow::Result<Annotated>`.
  The tag text inserted before each eligible function is exactly
  `#[must_use = "CR:<id>"] #[deprecated(note = "CR:<id>")] ` — inline, so **no line number in the file changes**.
  An unparseable file is an `Err`, never a silent skip.

- [ ] **Step 1: Add the dependencies**

```bash
cargo add syn --features full,visit
cargo add proc-macro2 --features span-locations
```

Expected `Cargo.toml` lines (version numbers may be newer; the API below was verified on these):
`syn = { version = "3.0.6", features = ["full", "visit"] }`, `proc-macro2 = { version = "1.0.107", features = ["span-locations"] }`.
(`cargo add` needs a target to exist; one does — `src/main.rs`.)

- [ ] **Step 2: Create the module skeleton**

`src/checks/unused_return_values/mod.rs`:

```rust
mod rewriter;
```

`src/checks/mod.rs` becomes:

```rust
pub mod reachability;
pub mod unused_return_values;
pub use reachability::ReachabilityCheck;
```

- [ ] **Step 3: Create `src/checks/unused_return_values/rewriter.rs`**

```rust
// src/checks/unused_return_values/rewriter.rs
//! Tags every eligible function with `#[must_use = "CR:<id>"] #[deprecated(note = "CR:<id>")]`,
//! inserted INLINE (never as a new line) so every original line number survives the rewrite.
//!
//! Why both attributes: `unused_must_use` fires only where a call's value is discarded; `deprecated`
//! fires at every use of the function. Ignored-sites / all-uses is what separates "never consumed"
//! from "consumed somewhere" -- `unused_must_use` alone can't (used sites are silent).

use proc_macro2::LineColumn;
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// One function that was tagged. `id` is the number in its `CR:<id>` reason string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnTag {
    pub id: u32,
    /// Forward-slash path relative to the project root, e.g. `src/lib.rs`.
    pub file: String,
    pub name: String,
    /// 1-based line of the function's name.
    pub line: u32,
    /// 1-based last line of the function's body (== `line` for a bodiless trait method).
    pub last_line: u32,
}

/// A `use` item's line span (inclusive). rustc reports `deprecated` at every imported name, which
/// is not a call site -- the classifier drops hits inside these ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseRange {
    pub file: String,
    pub first_line: u32,
    pub last_line: u32,
}

#[derive(Debug)]
pub struct Annotated {
    pub source: String,
    pub tags: Vec<FnTag>,
    pub use_ranges: Vec<UseRange>,
}

/// Annotates one file. `next_id` is shared across files so ids are unique per run.
pub fn annotate(file: &str, source: &str, next_id: &mut u32) -> anyhow::Result<Annotated> {
    let parsed = syn::parse_file(source)
        .map_err(|e| anyhow::anyhow!("could not parse {file} with syn ({e}); refusing to guess"))?;

    let mut visitor = Annotator {
        file,
        next_id,
        insertions: Vec::new(),
        tags: Vec::new(),
        use_ranges: Vec::new(),
    };
    visitor.visit_file(&parsed);

    let Annotator {
        mut insertions,
        tags,
        use_ranges,
        ..
    } = visitor;
    Ok(Annotated {
        source: apply_insertions(source, &mut insertions),
        tags,
        use_ranges,
    })
}

struct Insertion {
    at: LineColumn,
    text: String,
}

struct Annotator<'a> {
    file: &'a str,
    next_id: &'a mut u32,
    insertions: Vec<Insertion>,
    tags: Vec<FnTag>,
    use_ranges: Vec<UseRange>,
}

/// Attribute names (last path segment) that mean "leave this function alone".
const SKIP_ATTRS: &[&str] = &[
    "must_use",
    "deprecated",
    "test",
    "bench",
    "main",
    "no_mangle",
    "export_name",
    "proc_macro",
    "proc_macro_derive",
    "proc_macro_attribute",
];

impl Annotator<'_> {
    fn consider(
        &mut self,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        sig: &syn::Signature,
        last_line: u32,
    ) {
        if !is_eligible(attrs, sig) {
            return;
        }
        // Attributes must come before the visibility, so insert before `pub` when there is one.
        let at = if matches!(vis, syn::Visibility::Inherited) {
            sig.span().start()
        } else {
            vis.span().start()
        };
        let id = *self.next_id;
        *self.next_id += 1;
        self.insertions.push(Insertion {
            at,
            text: format!("#[must_use = \"CR:{id}\"] #[deprecated(note = \"CR:{id}\")] "),
        });
        self.tags.push(FnTag {
            id,
            file: self.file.to_string(),
            name: sig.ident.to_string(),
            line: sig.ident.span().start().line as u32,
            last_line,
        });
    }
}

impl<'ast> Visit<'ast> for Annotator<'_> {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let last = f.block.brace_token.span.close().end().line as u32;
        self.consider(&f.attrs, &f.vis, &f.sig, last);
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if f.modifiers.defaultness.is_none() {
            let last = f.block.brace_token.span.close().end().line as u32;
            self.consider(&f.attrs, &f.vis, &f.sig, last);
        }
        syn::visit::visit_impl_item_fn(self, f);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        let last = match &f.default {
            Some(block) => block.brace_token.span.close().end().line as u32,
            None => f.sig.ident.span().start().line as u32,
        };
        self.consider(&f.attrs, &syn::Visibility::Inherited, &f.sig, last);
        syn::visit::visit_trait_item_fn(self, f);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        // A trait impl's methods are reached through the trait declaration, which is tagged.
        // Tagging them too would double-count every call.
        if i.trait_.is_none() {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        self.use_ranges.push(UseRange {
            file: self.file.to_string(),
            first_line: u.use_token.span.start().line as u32,
            last_line: u.semi_token.span.end().line as u32,
        });
    }
}

fn is_eligible(attrs: &[syn::Attribute], sig: &syn::Signature) -> bool {
    if sig.asyncness.is_some() || sig.abi.is_some() || sig.ident == "main" {
        return false;
    }
    let skipped_attrs: HashSet<&str> = SKIP_ATTRS.iter().copied().collect();
    let has_skip_attr = attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|seg| skipped_attrs.contains(seg.ident.to_string().as_str()))
    });
    if has_skip_attr {
        return false;
    }
    returns_a_value_worth_tracking(&sig.output)
}

/// No `->`, `-> ()`, `-> !`, a `Result` (rustc already warns on ignoring it by default) and
/// `-> &mut Self` (builder chaining, idiomatically ignored) are not worth tracking.
fn returns_a_value_worth_tracking(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    match ty.as_ref() {
        syn::Type::Never(_) => false,
        syn::Type::Tuple(t) if t.elems.is_empty() => false,
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .is_none_or(|seg| seg.ident != "Result"),
        syn::Type::Reference(r) if r.mutability.is_some() => !matches!(
            r.elem.as_ref(),
            syn::Type::Path(p) if p.path.is_ident("Self")
        ),
        _ => true,
    }
}

/// Applies insertions from the last position to the first so earlier columns stay valid.
/// `LineColumn.column` counts characters, not bytes, so convert per line.
fn apply_insertions(source: &str, insertions: &mut [Insertion]) -> String {
    insertions.sort_by_key(|i| std::cmp::Reverse((i.at.line, i.at.column)));
    let mut lines: Vec<String> = source.split_inclusive('\n').map(str::to_string).collect();
    for ins in insertions.iter() {
        let line = &mut lines[ins.at.line - 1];
        let byte = line
            .char_indices()
            .nth(ins.at.column)
            .map_or(line.len(), |(b, _)| b);
        line.insert_str(byte, &ins.text);
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Annotated {
        annotate("src/lib.rs", src, &mut 0).unwrap()
    }

    const TAG0: &str = "#[must_use = \"CR:0\"] #[deprecated(note = \"CR:0\")] ";

    #[test]
    fn a_plain_function_is_tagged_inline_and_keeps_its_line_number() {
        let out = run("pub fn a() -> i32 { 1 }\n");
        assert_eq!(out.source, format!("{TAG0}pub fn a() -> i32 {{ 1 }}\n"));
        assert_eq!(out.tags.len(), 1);
        assert_eq!((out.tags[0].name.as_str(), out.tags[0].line), ("a", 1));
    }

    #[test]
    fn the_tag_goes_after_existing_attributes_and_before_pub_crate() {
        let out = run("/// doc\n#[inline]\n    pub(crate) fn a() -> i32 { 1 }\n");
        assert_eq!(
            out.source,
            format!("/// doc\n#[inline]\n    {TAG0}pub(crate) fn a() -> i32 {{ 1 }}\n")
        );
        assert_eq!(out.tags[0].line, 3);
    }

    #[test]
    fn inherent_methods_and_trait_declarations_are_tagged_but_trait_impl_methods_are_not() {
        let src = "struct S;\nimpl S {\n    fn m(&self) -> i32 { 1 }\n}\ntrait T {\n    fn t(&self) -> i32;\n}\nimpl T for S {\n    fn t(&self) -> i32 { 2 }\n}\n";
        let out = run(src);
        let names: Vec<_> = out.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["m", "t"]);
        assert_eq!(out.tags[1].line, 6, "the trait DECLARATION, not the impl");
        assert_eq!(out.source.matches("#[deprecated").count(), 2);
    }

    #[test]
    fn nested_functions_and_functions_in_modules_are_tagged() {
        let out = run("mod m {\n    pub fn a() -> i32 {\n        fn inner() -> i32 { 1 }\n        inner()\n    }\n}\n");
        let names: Vec<_> = out.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["a", "inner"]);
        assert_eq!(out.tags[0].last_line, 5, "body spans lines 2..=5");
    }

    #[test]
    fn ids_continue_across_files() {
        let mut next = 0;
        let a = annotate("src/a.rs", "fn a() -> i32 { 1 }\n", &mut next).unwrap();
        let b = annotate("src/b.rs", "fn b() -> i32 { 1 }\n", &mut next).unwrap();
        assert_eq!((a.tags[0].id, b.tags[0].id, next), (0, 1, 2));
    }

    #[test]
    fn two_functions_on_one_line_are_both_tagged() {
        let out = run("fn a() -> i32 { 1 } fn b() -> i32 { 2 }\n");
        assert_eq!(out.tags.len(), 2);
        assert_eq!(out.source.matches("#[must_use").count(), 2);
    }

    #[test]
    fn columns_are_characters_not_bytes_and_crlf_is_preserved() {
        let out = run("/* é */ fn a() -> i32 { 1 }\r\nfn b() -> i32 { 2 }\r\n");
        assert!(out.source.starts_with(&format!("/* é */ {TAG0}fn a()")));
        assert!(out.source.contains("{ 1 }\r\n"));
        assert!(out.source.ends_with("{ 2 }\r\n"));
    }

    #[test]
    fn functions_not_worth_tracking_are_left_untouched() {
        let cases = [
            "fn f() {}\n",
            "fn f() -> () {}\n",
            "fn f() -> ! { loop {} }\n",
            "async fn f() -> i32 { 1 }\n",
            "fn f() -> Result<i32, ()> { Ok(1) }\n",
            "fn f() -> std::io::Result<i32> { Ok(1) }\n",
            "struct B;\nimpl B { fn f(&mut self) -> &mut Self { self } }\n",
            "fn main() -> i32 { 1 }\n",
            "#[test]\nfn f() -> i32 { 1 }\n",
            "#[tokio::main]\nfn f() -> i32 { 1 }\n",
            "#[must_use]\nfn f() -> i32 { 1 }\n",
            "#[deprecated]\nfn f() -> i32 { 1 }\n",
            "extern \"C\" fn f() -> i32 { 1 }\n",
            "#[no_mangle]\npub fn f() -> i32 { 1 }\n",
        ];
        for src in cases {
            let out = run(src);
            assert_eq!(out.source, src, "should be untouched: {src:?}");
            assert!(out.tags.is_empty(), "no tag expected: {src:?}");
        }
    }

    #[test]
    fn a_function_returning_a_mutable_reference_to_something_else_is_still_tracked() {
        let out = run("fn f(v: &mut Vec<i32>) -> &mut i32 { &mut v[0] }\n");
        assert_eq!(out.tags.len(), 1);
    }

    #[test]
    fn use_statements_are_recorded_including_multiline_ones() {
        let out = run("use a::b;\npub use c::{\n    d,\n};\nfn f() -> i32 { 1 }\n");
        assert_eq!(
            out.use_ranges,
            vec![
                UseRange {
                    file: "src/lib.rs".into(),
                    first_line: 1,
                    last_line: 1
                },
                UseRange {
                    file: "src/lib.rs".into(),
                    first_line: 2,
                    last_line: 4
                },
            ]
        );
    }

    #[test]
    fn a_file_syn_cannot_parse_is_an_error_not_a_silent_skip() {
        assert!(annotate("src/lib.rs", "this is not rust {{{", &mut 0).is_err());
    }
}
```

- [ ] **Step 4: Run the rewriter tests**

Run: `cargo test unused_return_values::rewriter`
Expected: PASS, 11 tests.

If `syn`'s API has moved again and the build fails, fix against the compiler's own suggestion — the one
change already known between syn 2 and 3 is `f.modifiers.defaultness` (see "Corrections", item 7).

- [ ] **Step 5: Prove the skip rules and the position logic can fail**

(a) In `returns_a_value_worth_tracking`, change `syn::Type::Never(_) => false` to `=> true`: `functions_not_worth_tracking_are_left_untouched` must FAIL. Revert.
(b) In `apply_insertions`, replace `.nth(ins.at.column)` with `.nth(ins.at.column + 1)`: `columns_are_characters_not_bytes_and_crlf_is_preserved` and `a_plain_function_is_tagged_inline_and_keeps_its_line_number` must FAIL. Revert.

- [ ] **Step 6: Format, lint, commit**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: clean except `dead_code` warnings for items only used by later tasks. **If clippy `-D warnings` fails on `dead_code`, add `#![allow(dead_code)]` at the top of `src/checks/unused_return_values/mod.rs` for this commit and remove it in Task 5** (CI gates every commit on this PR's branch only at the tip, but keep each commit green anyway).

```bash
git add -A Cargo.toml Cargo.lock src
git commit -m "feat(unused-return-values): syn-based tagging rewriter, inline so line numbers survive"
```

---

### Task 4: Classifier and sentinel

**Files:**
- Create: `src/checks/unused_return_values/classify.rs`, `src/checks/unused_return_values/sentinel.rs`
- Modify: `src/checks/unused_return_values/mod.rs`

**Interfaces:**
- Consumes: `rewriter::{FnTag, UseRange}`, `crate::cargo_json::Diagnostic`.
- Produces: `classify::classify(diagnostics: &[Diagnostic], tags: &[FnTag], use_ranges: &[UseRange]) -> Classification<'_>` where
  `Classification { sentinel_ok: bool, usages: Vec<Usage<'_>> }`, `Usage { tag: &FnTag, ignored: u32, total: u32 }` with `Usage::all_ignored() -> bool`; only functions with `ignored >= 1` appear, ordered by (file, line). `classify::SENTINEL_TAG: &str = "sentinel"`.
  `sentinel::inject_sentinel(worktree_root: &Path) -> anyhow::Result<String>` (returns `"src/lib.rs"` or `"src/main.rs"`) and `sentinel::SENTINEL_FN: &str`.

- [ ] **Step 1: Create `src/checks/unused_return_values/classify.rs`**

```rust
// src/checks/unused_return_values/classify.rs
//! Turns rustc's diagnostics plus the rewriter's tag table into per-function usage counts.
//!
//! Two signals, joined by the `CR:<id>` reason string:
//! - `unused_must_use` -> a call whose value was discarded ("ignored"). The tag is a child note.
//! - `deprecated`      -> any use at all ("total"). The tag is the text after the last `: CR:`.

use super::rewriter::{FnTag, UseRange};
use crate::cargo_json::Diagnostic;
use std::collections::{BTreeMap, HashSet};

/// The reason string of the per-run sentinel, which is not a real function.
pub const SENTINEL_TAG: &str = "sentinel";

#[derive(Debug, PartialEq, Eq)]
enum Tag {
    Fn(u32),
    Sentinel,
}

fn parse_tag(text: &str) -> Option<Tag> {
    let rest = text.strip_prefix("CR:")?;
    if rest == SENTINEL_TAG {
        Some(Tag::Sentinel)
    } else {
        rest.parse().ok().map(Tag::Fn)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Usage<'a> {
    pub tag: &'a FnTag,
    /// Call sites whose returned value is discarded.
    pub ignored: u32,
    /// Every use (calls, method calls, function-pointer uses), `let _ =` included.
    pub total: u32,
}

impl Usage<'_> {
    pub fn all_ignored(&self) -> bool {
        self.ignored == self.total
    }
}

#[derive(Debug)]
pub struct Classification<'a> {
    /// True only if BOTH signals were seen for the injected sentinel in this same build.
    pub sentinel_ok: bool,
    /// Functions with at least one ignored call, ordered by (file, line).
    pub usages: Vec<Usage<'a>>,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Kind {
    Ignored,
    Used,
}

pub fn classify<'a>(
    diagnostics: &[Diagnostic],
    tags: &'a [FnTag],
    use_ranges: &[UseRange],
) -> Classification<'a> {
    let by_id: BTreeMap<u32, &FnTag> = tags.iter().map(|t| (t.id, t)).collect();
    let mut seen: HashSet<(Kind, u32, String, u32, u32)> = HashSet::new();
    let mut ignored: BTreeMap<u32, u32> = BTreeMap::new();
    let mut total: BTreeMap<u32, u32> = BTreeMap::new();
    let (mut sentinel_ignored, mut sentinel_used) = (false, false);

    for d in diagnostics {
        let (kind, tag) = match d.code.as_deref() {
            Some("unused_must_use") => (Kind::Ignored, d.notes.iter().find_map(|n| parse_tag(n))),
            Some("deprecated") => (
                Kind::Used,
                d.message
                    .rsplit_once(": ")
                    .and_then(|(_, tail)| parse_tag(tail)),
            ),
            _ => continue,
        };
        let (Some(tag), Some(span)) = (tag, d.primary_span()) else {
            continue;
        };

        let id = match tag {
            Tag::Sentinel => {
                match kind {
                    Kind::Ignored => sentinel_ignored = true,
                    Kind::Used => sentinel_used = true,
                }
                continue;
            }
            Tag::Fn(id) => id,
        };
        let Some(def) = by_id.get(&id) else { continue };

        // `--all-targets` compiles shared code more than once; one site is one site.
        if !seen.insert((kind, id, span.file.clone(), span.line, span.column)) {
            continue;
        }
        // rustc never reports `deprecated` for a function's use of ITSELF, but does report
        // `unused_must_use` for it. Drop both so a recursive call is not an ignored caller.
        if span.file == def.file && (def.line..=def.last_line).contains(&span.line) {
            continue;
        }
        // An import is not a call site, but rustc reports `deprecated` at every imported name.
        if kind == Kind::Used
            && use_ranges
                .iter()
                .any(|u| u.file == span.file && (u.first_line..=u.last_line).contains(&span.line))
        {
            continue;
        }

        match kind {
            Kind::Ignored => *ignored.entry(id).or_default() += 1,
            Kind::Used => *total.entry(id).or_default() += 1,
        }
    }

    let mut usages: Vec<Usage> = ignored
        .into_iter()
        .map(|(id, ignored)| Usage {
            tag: by_id[&id],
            ignored,
            // A macro can expand one `deprecated` hit into several ignored sites; never report
            // more discarded calls than uses.
            total: total.get(&id).copied().unwrap_or(0).max(ignored),
        })
        .collect();
    usages.sort_by(|a, b| (&a.tag.file, a.tag.line).cmp(&(&b.tag.file, b.tag.line)));

    Classification {
        sentinel_ok: sentinel_ignored && sentinel_used,
        usages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo_json::Span;

    fn tag(id: u32, line: u32, last_line: u32) -> FnTag {
        FnTag {
            id,
            file: "src/lib.rs".into(),
            name: format!("f{id}"),
            line,
            last_line,
        }
    }

    fn ignored(id: &str, line: u32, col: u32) -> Diagnostic {
        Diagnostic {
            code: Some("unused_must_use".into()),
            level: "warning".into(),
            message: "unused return value of `f` that must be used".into(),
            notes: vec![format!("CR:{id}"), "use `let _ = ...`".into()],
            spans: vec![Span {
                file: "src/lib.rs".into(),
                line,
                column: col,
                is_primary: true,
            }],
        }
    }

    fn used(id: &str, line: u32, col: u32) -> Diagnostic {
        Diagnostic {
            code: Some("deprecated".into()),
            level: "warning".into(),
            message: format!("use of deprecated function `f`: CR:{id}"),
            notes: vec![],
            spans: vec![Span {
                file: "src/lib.rs".into(),
                line,
                column: col,
                is_primary: true,
            }],
        }
    }

    #[test]
    fn all_ignored_versus_mixed_comes_from_comparing_ignored_to_total() {
        let tags = [tag(0, 1, 1), tag(1, 2, 2)];
        let diags = [
            // f0: used twice, both discarded
            used("0", 10, 5),
            ignored("0", 10, 5),
            used("0", 11, 5),
            ignored("0", 11, 5),
            // f1: used twice, one discarded
            used("1", 12, 5),
            ignored("1", 12, 5),
            used("1", 13, 13),
        ];
        let c = classify(&diags, &tags, &[]);
        let got: Vec<_> = c
            .usages
            .iter()
            .map(|u| (u.tag.id, u.ignored, u.total, u.all_ignored()))
            .collect();
        assert_eq!(got, vec![(0, 2, 2, true), (1, 1, 2, false)]);
    }

    #[test]
    fn a_function_never_discarded_is_not_reported_at_all() {
        let tags = [tag(0, 1, 1)];
        let c = classify(&[used("0", 10, 5)], &tags, &[]);
        assert!(c.usages.is_empty());
    }

    #[test]
    fn duplicate_reports_from_multiple_targets_count_once() {
        let tags = [tag(0, 1, 1)];
        let diags = [
            used("0", 10, 5),
            ignored("0", 10, 5),
            used("0", 10, 5),
            ignored("0", 10, 5),
        ];
        let c = classify(&diags, &tags, &[]);
        assert_eq!((c.usages[0].ignored, c.usages[0].total), (1, 1));
    }

    #[test]
    fn a_recursive_call_inside_the_functions_own_body_is_neither_ignored_nor_used() {
        // Review Focus: rustc skips `deprecated` for self-use but not `unused_must_use`, so a
        // function that only discards its own recursive result would otherwise look "ignored 1 of 0".
        let tags = [tag(0, 3, 9)];
        let diags = [ignored("0", 6, 9), used("0", 20, 5)];
        let c = classify(&diags, &tags, &[]);
        assert!(
            c.usages.is_empty(),
            "the only real use (line 20) is not discarded"
        );
    }

    #[test]
    fn an_import_is_not_a_call_site() {
        // Review Focus: `use a::f;` makes rustc report `deprecated` at the imported name, which
        // would turn "ignored 1 of 1" into "ignored 1 of 2" -- every cross-module function "mixed".
        let tags = [tag(0, 1, 1)];
        let imports = [UseRange {
            file: "src/lib.rs".into(),
            first_line: 7,
            last_line: 9,
        }];
        let diags = [used("0", 8, 5), used("0", 12, 5), ignored("0", 12, 5)];
        let c = classify(&diags, &tags, &imports);
        assert_eq!((c.usages[0].ignored, c.usages[0].total), (1, 1));
        assert!(c.usages[0].all_ignored());
    }

    #[test]
    fn the_sentinel_needs_both_signals() {
        let tags: [FnTag; 0] = [];
        assert!(!classify(&[ignored("sentinel", 1, 1)], &tags, &[]).sentinel_ok);
        assert!(!classify(&[used("sentinel", 1, 1)], &tags, &[]).sentinel_ok);
        let both = [ignored("sentinel", 1, 1), used("sentinel", 1, 1)];
        assert!(classify(&both, &tags, &[]).sentinel_ok);
    }

    #[test]
    fn a_tag_that_is_not_ours_or_not_in_the_table_is_ignored() {
        let tags = [tag(0, 1, 1)];
        let mut other = ignored("0", 10, 5);
        other.notes = vec!["some other must_use reason".into()];
        let diags = [other, ignored("99", 11, 5), used("99", 11, 5)];
        assert!(classify(&diags, &tags, &[]).usages.is_empty());
    }
}
```

- [ ] **Step 2: Create `src/checks/unused_return_values/sentinel.rs`**

```rust
// src/checks/unused_return_values/sentinel.rs
use super::classify::SENTINEL_TAG;
use std::path::Path;

/// Not underscore-prefixed on purpose: rustc treats a leading `_` as "intentionally unused" and
/// would silence the very lints the sentinel exists to prove are working.
pub const SENTINEL_FN: &str = "coderipper_unused_return_sentinel_7d41";

/// Appends a function tagged exactly like every real one, plus a caller that discards its value,
/// to `src/lib.rs` (preferred) or `src/main.rs`. If this run's build does not report BOTH the
/// ignored call and the use of this function, the crate is suppressing one of the two lints
/// (`#![allow(deprecated)]`, `#![allow(unused_must_use)]`) and no real result can be trusted.
/// Returns the file it wrote to.
pub fn inject_sentinel(worktree_root: &Path) -> anyhow::Result<String> {
    for candidate in ["lib.rs", "main.rs"] {
        let path = worktree_root.join("src").join(candidate);
        if path.exists() {
            let mut content = std::fs::read_to_string(&path)?;
            content.push_str(&format!(
                "\n#[allow(dead_code)]\n\
                 #[must_use = \"CR:{SENTINEL_TAG}\"] #[deprecated(note = \"CR:{SENTINEL_TAG}\")]\n\
                 fn {SENTINEL_FN}() -> i32 {{ 1 }}\n\
                 #[allow(dead_code)]\n\
                 fn {SENTINEL_FN}_caller() {{ {SENTINEL_FN}(); }}\n"
            ));
            std::fs::write(&path, content)?;
            return Ok(format!("src/{candidate}"));
        }
    }
    anyhow::bail!("neither src/lib.rs nor src/main.rs exists under {worktree_root:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_with(file: &str, contents: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src").join(file), contents).unwrap();
        tmp
    }

    #[test]
    fn it_appends_to_lib_rs_and_keeps_the_original_content() {
        let tmp = crate_with("lib.rs", "pub fn real() {}\n");
        assert_eq!(inject_sentinel(tmp.path()).unwrap(), "src/lib.rs");
        let text = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(text.starts_with("pub fn real() {}\n"));
        assert!(text.contains(SENTINEL_FN) && text.contains("CR:sentinel"));
    }

    #[test]
    fn it_falls_back_to_main_rs() {
        let tmp = crate_with("main.rs", "fn main() {}\n");
        assert_eq!(inject_sentinel(tmp.path()).unwrap(), "src/main.rs");
    }

    #[test]
    fn it_fails_when_there_is_no_root_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        assert!(inject_sentinel(tmp.path()).is_err());
    }
}
```

- [ ] **Step 3: Declare both modules** — `src/checks/unused_return_values/mod.rs`:

```rust
mod classify;
mod rewriter;
mod sentinel;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test unused_return_values`
Expected: PASS — 11 rewriter + 7 classify (`all_ignored_versus_mixed_comes_from_comparing_ignored_to_total`, `a_function_never_discarded_is_not_reported_at_all`, `duplicate_reports_from_multiple_targets_count_once`, `a_recursive_call_inside_the_functions_own_body_is_neither_ignored_nor_used`, `an_import_is_not_a_call_site`, `the_sentinel_needs_both_signals`, `a_tag_that_is_not_ours_or_not_in_the_table_is_ignored`) + 3 sentinel.

- [ ] **Step 5: Prove the two Review Focus guards can fail**

(a) In `classify`, delete the `if span.file == def.file && ...` block: `a_recursive_call_inside_the_functions_own_body_is_neither_ignored_nor_used` must FAIL. Revert.
(b) Delete the `if kind == Kind::Used && use_ranges...` block: `an_import_is_not_a_call_site` must FAIL. Revert.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt
git add -A src
git commit -m "feat(unused-return-values): classify ignored vs total uses by tag; per-run sentinel"
```

---

### Task 5: The check, its registration, and the core end-to-end tests

**Files:**
- Create: `src/checks/unused_return_values/mod.rs` (replaces the module-declaration stub), `tests/common/mod.rs`, `tests/unused_return_values.rs`
- Modify: `src/checks/mod.rs`, `src/lib.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces: `checks::UnusedReturnValuesCheck` (a `Check`, id `"unused-return-values"`, `Scope::Project`, `Network::LocalOnly`); `checks::unused_return_values::CHECK_ID`. Findings: all-ignored → `Severity::Medium` "…is discarded at every one of its N call site(s)"; some-ignored → `Severity::Low` "…discarded at K of its N call sites"; `Confidence::Medium` for both; `positive_control` always set; `location` = the function's name; allowlist key = (`"unused-return-values"`, file, bare fn name).
- Test helper: `tests/common/mod.rs` — `git_repo_with(&[(path, contents)]) -> TempDir` and `MANIFEST`.

- [ ] **Step 1: Write the failing end-to-end tests first**

Create `tests/common/mod.rs`:

```rust
// tests/common/mod.rs
//! Shared fixture helpers for integration tests. A fixture is built in a throwaway tempdir and
//! committed to its own fresh git repo, because every check reads HEAD through a git worktree --
//! a test must never point a check at the real shared checkout.

use std::path::Path;
use std::process::Command;

pub fn git_repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["add", "-A"]);
    git(
        tmp.path(),
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    tmp
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

pub const MANIFEST: &str =
    "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
```

Create `tests/unused_return_values.rs` with this content (Task 6 appends more tests to it):

```rust
// tests/unused_return_values.rs (part A)
mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::UnusedReturnValuesCheck;
use coderipper::finding::Finding;
use common::{git_repo_with, MANIFEST};

fn run(repo: &tempfile::TempDir) -> anyhow::Result<Vec<Finding>> {
    UnusedReturnValuesCheck.run(&CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    })
}

fn names(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings
        .iter()
        .map(|f| f.summary.split('`').nth(1).unwrap().to_string())
        .collect();
    v.sort();
    v
}

const LIB: &str = "\
pub fn always_ignored() -> i32 { 1 }
pub fn sometimes_ignored() -> i32 { 2 }
pub fn always_used() -> i32 { 3 }
pub fn never_called() -> i32 { 4 }
pub fn unit() {}
";

const MAIN: &str = "\
use fixture::always_ignored;
fn main() {
    always_ignored();
    fixture::sometimes_ignored();
    let x = fixture::sometimes_ignored();
    println!(\"{}\", x + fixture::always_used());
    fixture::unit();
}
";

#[test]
fn it_separates_always_ignored_from_sometimes_ignored_on_a_lib_plus_bin_package() {
    // A lib consumed by its own bin: the reachability check cannot analyze this shape at all
    // (E0603 after its pub->pub(crate) rewrite). This check needs no visibility rewrite.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("src/main.rs", MAIN),
    ]);
    let findings = run(&repo).unwrap();

    assert_eq!(
        names(&findings),
        vec!["always_ignored", "sometimes_ignored"]
    );
    let always = findings
        .iter()
        .find(|f| f.summary.contains("`always_ignored`"))
        .unwrap();
    // Review Focus: `use fixture::always_ignored;` must not count as a second use.
    assert!(
        always.summary.contains("every one of its 1 call site"),
        "{}",
        always.summary
    );
    let sometimes = findings
        .iter()
        .find(|f| f.summary.contains("`sometimes_ignored`"))
        .unwrap();
    assert!(
        sometimes.summary.contains("1 of its 2 call sites"),
        "{}",
        sometimes.summary
    );
    assert!(findings.iter().all(|f| f.positive_control.is_some()));
    assert!(findings.iter().all(|f| f.clone().validate().is_ok()));
}

#[test]
fn a_deliberate_let_underscore_discard_counts_as_a_use_not_an_ignore() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn f() -> i32 { 1 }\nfn main() { let _ = f(); _ = f(); }\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}

#[test]
fn a_function_that_only_discards_its_own_recursive_result_is_not_flagged() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn rec(n: i32) -> i32 { if n == 0 { 0 } else { rec(n - 1); 1 } }\nfn main() { println!(\"{}\", rec(3)); }\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}

#[test]
fn an_allowlisted_function_is_suppressed() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nsymbol = \"f\"\nreason = \"test\"\n",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}
```

- [ ] **Step 2: Run them — they must fail**

Run: `cargo test --test unused_return_values`
Expected: FAIL to compile — `no UnusedReturnValuesCheck in checks`.

- [ ] **Step 3: Replace `src/checks/unused_return_values/mod.rs` with the check**

If you added `#![allow(dead_code)]` in Task 3, drop it now.

```rust
// src/checks/unused_return_values/mod.rs
//! Unused-return-values check (project scope, Rust): is a function's return value ever consumed,
//! or does every caller discard it?
//!
//! Mechanism (design doc §5, as corrected by the plan): in a throwaway worktree, tag every eligible
//! function with `#[must_use = "CR:<id>"]` + `#[deprecated(note = "CR:<id>")]`, build, and join rustc's
//! `unused_must_use` (ignored call sites) with its `deprecated` (all uses) by the `<id>`.
//!
//! No visibility rewrite is involved, so unlike the reachability check this works on a package with
//! a lib target consumed by its own bin or tests.
//!
//! **Known limitations (by design for v1):**
//! - A function defined inside a macro body (`macro_rules!`) is never tagged: syn does not see
//!   inside a macro's token tree.
//! - `let _ = f();` and `_ = f();` count as a USE: the author discarded it deliberately.
//! - Analysis is HEAD-only (uncommitted edits aren't seen), as with the reachability check.
//! - Only `src/` is rewritten; calls from `tests/`, `examples/`, `benches/` still count as uses.
//! - Single-package projects only (a workspace root without its own `src/` errors out).

mod classify;
mod rewriter;
mod sentinel;

use crate::allowlist::Allowlist;
use crate::cargo_json::build_all_targets;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use classify::classify;
use rewriter::annotate;
use sentinel::{inject_sentinel, SENTINEL_FN};

pub const CHECK_ID: &str = "unused-return-values";

pub struct UnusedReturnValuesCheck;

impl Check for UnusedReturnValuesCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let mut next_id = 0;
        let mut tags = Vec::new();
        let mut use_ranges = Vec::new();
        let wt = RewrittenWorktree::create_with(&ctx.project_root, |file, source| {
            let annotated = annotate(file, source, &mut next_id)?;
            tags.extend(annotated.tags);
            use_ranges.extend(annotated.use_ranges);
            Ok(annotated.source)
        })?;
        let sentinel_file = inject_sentinel(&wt.root)?;
        let build = build_all_targets(&wt.root)?;

        // A real compiler error anywhere must never look like a clean result.
        anyhow::ensure!(
            !build.is_broken(),
            "this crate's build reported real compiler error(s) (or failed without any diagnostic) \
             after the unused-return-values rewrite, so no result can be trusted"
        );

        let classification = classify(&build.diagnostics, &tags, &use_ranges);
        anyhow::ensure!(
            classification.sentinel_ok,
            "unused-return-values' own per-run positive control (a tagged sentinel function with a \
             discarded call, injected into {sentinel_file}) was not reported by BOTH `unused_must_use` \
             and `deprecated` even though the build reported no errors -- one of those lints is \
             suppressed in this crate (e.g. a crate-wide #![allow(deprecated)]), so this run's result \
             can't be trusted."
        );

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let positive_control = format!(
            "this run's own sentinel (`{SENTINEL_FN}`, tagged exactly like every real function and \
             called once with its value discarded, injected into `{sentinel_file}`) was reported by \
             BOTH `unused_must_use` (discarded call) and `deprecated` (use) in this same build, so \
             both signals were working for this crate"
        );

        Ok(classification
            .usages
            .iter()
            .filter(|u| !allowlist.is_allowed(CHECK_ID, &u.tag.file, &u.tag.name))
            .map(|u| {
                let (severity, summary) = if u.all_ignored() {
                    (
                        Severity::Medium,
                        format!(
                            "`{}`'s return value is discarded at every one of its {} call site(s)",
                            u.tag.name, u.total
                        ),
                    )
                } else {
                    (
                        Severity::Low,
                        format!(
                            "`{}`'s return value is discarded at {} of its {} call sites",
                            u.tag.name, u.ignored, u.total
                        ),
                    )
                };
                Finding {
                    check_id: CHECK_ID.into(),
                    severity,
                    confidence: Confidence::Medium,
                    project: project.clone(),
                    location: Some(Location {
                        file: u.tag.file.clone(),
                        line: Some(u.tag.line),
                    }),
                    summary,
                    detail: format!(
                        "Counted by tagging the function with `#[must_use]` and `#[deprecated]` in a \
                         throwaway worktree and comparing rustc's discarded-value reports to its \
                         use reports. `let _ = f();` counts as a use (a deliberate discard). Defined \
                         at {}:{}.",
                        u.tag.file, u.tag.line
                    ),
                    positive_control: Some(positive_control.clone()),
                }
            })
            .collect())
    }
}
```

- [ ] **Step 4: Register it**

`src/checks/mod.rs`:

```rust
pub mod reachability;
pub mod unused_return_values;
pub use reachability::ReachabilityCheck;
pub use unused_return_values::UnusedReturnValuesCheck;
```

`src/lib.rs` — the registered list, and the test that pins it:

```rust
pub fn registered_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(checks::ReachabilityCheck),
        Box::new(checks::UnusedReturnValuesCheck),
    ]
}
```

and in `mod tests`, `a_registered_check_is_actually_registered` becomes:

```rust
        let checks = registered_checks();
        let ids: Vec<_> = checks.iter().map(|c| c.id()).collect();
        assert_eq!(ids, vec!["reachability", "unused-return-values"]);
```

(keep that test's existing explanatory comment above it.)

- [ ] **Step 5: Run the end-to-end tests**

Run: `cargo test --test unused_return_values`
Expected: PASS, 4 tests. Each builds a real crate with `cargo build`, so allow ~2–4 s per test.

- [ ] **Step 6: Prove the Review Focus guards fail end to end**

(a) In `mod.rs`, change `classify(&build.diagnostics, &tags, &use_ranges)` to pass `&[]` for the last argument:
`it_separates_always_ignored_from_sometimes_ignored_on_a_lib_plus_bin_package` must FAIL (the import of
`always_ignored` is counted as a second use, so it reads "1 of its 2 call sites"). Revert.
(b) Delete the self-range `continue` block in `classify.rs`:
`a_function_that_only_discards_its_own_recursive_result_is_not_flagged` must FAIL. Revert.

- [ ] **Step 7: Full suite, lint, commit**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all green, no clippy output.

```bash
git add -A src tests
git commit -m "feat(unused-return-values): the check, registered, with end-to-end fixtures"
```

---

### Task 6: Hardening tests, CLI test, docs, spec correction, PR

**Files:**
- Modify: `tests/unused_return_values.rs`, `tests/cli.rs`, `README.md`, `docs/superpowers/specs/2026-09-30-audit-host-design.md`

- [ ] **Step 1: Append the hardening tests to `tests/unused_return_values.rs`**

These need no new production code — they pin behaviour Task 5 already has: a silenced lint must fail
the run loudly, `deny(warnings)` must neither hide findings nor look like a broken build, a real compile
error must fail the run, and non-ASCII/CRLF source and skipped shapes must behave.

```rust
// tests/unused_return_values.rs (part B, appended)
#[test]
fn a_crate_that_allows_deprecated_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![allow(deprecated)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    let err = run(&repo).unwrap_err().to_string();
    assert!(err.contains("positive control"), "{err}");
}

#[test]
fn a_crate_that_allows_unused_must_use_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![allow(unused_must_use)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    assert!(run(&repo).is_err());
}

#[test]
fn deny_warnings_does_not_hide_findings_or_look_like_a_broken_build() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "#![deny(warnings)]\nfn f() -> i32 { 1 }\nfn main() { f(); }\n",
        ),
    ]);
    assert_eq!(names(&run(&repo).unwrap()), vec!["f"]);
}

#[test]
fn a_crate_that_does_not_compile_errors_instead_of_reporting_clean() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn main() { let x: i32 = \"no\"; }\n"),
    ]);
    assert!(run(&repo).is_err());
}

#[test]
fn a_file_with_non_ascii_text_and_crlf_endings_is_analyzed_correctly() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "// héllo wörld\r\nfn f() -> i32 { 1 }\r\nfn main() { /* ünï */ f(); }\r\n",
        ),
    ]);
    assert_eq!(names(&run(&repo).unwrap()), vec!["f"]);
}

#[test]
fn skipped_function_shapes_never_produce_findings() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "\
struct B;
impl B { fn chain(&mut self) -> &mut Self { self } }
fn res() -> Result<i32, ()> { Ok(1) }
async fn asy() -> i32 { 1 }
#[must_use]
fn already() -> i32 { 1 }
fn main() {
    let mut b = B;
    b.chain();
    let _ = res();
    let _ = asy();
    let _ = already();
}
",
        ),
    ]);
    assert!(run(&repo).unwrap().is_empty());
}
```

- [ ] **Step 2: Run them**

Run: `cargo test --test unused_return_values`
Expected: PASS, 10 tests total.

If `deny_warnings_does_not_hide_findings_or_look_like_a_broken_build` fails with "real compiler error(s)",
rustc is emitting an error-level diagnostic with no code under `deny(warnings)` (a summary line). Print the
offending diagnostics and exempt exactly that one message in `Diagnostic::is_hard_error`
(`src/cargo_json.rs`) with a comment naming it — do not loosen the rule beyond it. (It passes on rustc 1.98.1.)

- [ ] **Step 3: Prove the blind-crate guard can fail** — in `mod.rs`, comment out the `anyhow::ensure!(classification.sentinel_ok, ...)`: `a_crate_that_allows_deprecated_errors_instead_of_reporting_clean` must FAIL (the run returns `Ok` and reports clean). Revert.

- [ ] **Step 4: Add the CLI test** — append to `tests/cli.rs`:

```rust
// tests/cli.rs (appended)
#[test]
fn the_unused_return_values_check_runs_by_id_and_reports_a_finding() {
    let repo = {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"discarder\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src/main.rs"),
            "fn f() -> i32 { 1 }\nfn main() { f(); }\n",
        )
        .unwrap();
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
                "init",
            ],
        ] {
            StdCommand::new("git")
                .args(args)
                .current_dir(tmp.path())
                .status()
                .unwrap();
        }
        tmp
    };
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("(unused-return-values)"))
        .stdout(predicate::str::contains("`f`"));
}
```

Run: `cargo test --test cli`
Expected: PASS, 5 tests.

- [ ] **Step 5: README** — in `README.md`, change the command block to include
`coderipper check unused-return-values  # is a function's return value ever consumed?` after the
`coderipper check reachability` line, and in `## Status` replace the text "Early scaffold. The host, the shared
`Finding` schema, and the check-plugin interface exist; no checks are implemented yet." (it is already stale —
reachability merged without updating it) with: "Early. The host, the shared `Finding` schema, and the
check-plugin interface exist, and two checks are implemented: `reachability` (dead code, including `pub`
items) and `unused-return-values` (a function whose return value every caller discards)."

- [ ] **Step 6: Amend the spec** — **precondition: PR #3 (which adds §4/§5) is merged.** At the time this plan was
written (`origin/main` = `7271ad6`) it was still open and `git show origin/main:docs/superpowers/specs/2026-09-30-audit-host-design.md | grep -c unused-return-values` printed `0`.
`git fetch origin` and re-run that grep: if it still prints `0`, skip this step, say so in the PR description,
and leave the amendment to whoever lands #3. Otherwise, in
`docs/superpowers/specs/2026-09-30-audit-host-design.md` §5, replace everything from `**Mechanism, Rust**:`
up to (not including) `**Mechanism, other languages**:` — that is one sentence ending
"...is a weaker (but still worth reporting) finding." — with:

```
**Mechanism, Rust** (corrected 2026-10-01 after testing; see `docs/superpowers/plans/2026-10-01-unused-return-values.md`): in the throwaway worktree, tag every eligible function with `#[must_use = "CR:<id>"]` *and* `#[deprecated(note = "CR:<id>")]` (inline, so line numbers survive), rebuild with `--all-targets`, and join rustc's `unused_must_use` (call sites whose value is discarded) with its `deprecated` (every use) by `<id>`. `unused_must_use` alone cannot distinguish all-ignored from some-ignored, because it is silent at used call sites; the `deprecated` pairing supplies the denominator. Imports (`use`) and a function's own recursive calls are excluded from the counts. No `pub` -> `pub(crate)` rewrite is needed, so this check — unlike `reachability` — works on a package whose lib is consumed by its own bin or tests. Unit-returning, `Result`-returning, `async`, `extern`, and `-> &mut Self` functions are skipped.
```

(Keep the trailing space before `**Mechanism, other languages**:`.)

- [ ] **Step 7: Final full gate, exactly as CI runs it**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --no-fail-fast
```

Expected: every command exits 0; `cargo test` shows 58 lib unit tests, 5 CLI, 4 reachability, 10 unused-return-values.

- [ ] **Step 8: Realistic-crate probe (no repo write needed)** — clone CodeRipper into a scratch dir, run the built
binary against it, then append `pub fn zzz_probe() -> i32 { 1 }` and `pub fn zzz_caller() { zzz_probe(); }` to its
`src/lib.rs`, commit **in the scratch clone only**, and run again:

```bash
git clone <this repo's local path or URL> /tmp/scan && cd /tmp/scan
<built-binary> check unused-return-values --project .      # expect: coderipper: no issues found
printf '\npub fn zzz_probe() -> i32 { 1 }\npub fn zzz_caller() { zzz_probe(); }\n' >> src/lib.rs
git -c user.email=t@t -c user.name=t commit -qam probe
<built-binary> check unused-return-values --project .      # expect one Medium finding naming zzz_probe
```

Expected: first run clean, second run exactly one finding for `zzz_probe`. (Measured at plan time against
CodeRipper at `7271ad6`, a lib+bin package the reachability check cannot analyze: clean, then the one finding; ~15 s.)

- [ ] **Step 9: Commit, push, open the PR**

```bash
git add -A README.md docs tests
git commit -m "test+docs(unused-return-values): hardening fixtures, CLI test, README, spec §5 corrected"
git push -u origin feat/unused-return-values
gh pr create --title "feat: unused-return-values check (project scope, Rust)" --body-file <(printf '%s\n' "Second real check. Tags functions with must_use + deprecated in a throwaway worktree and joins rustc's two lints by id to separate all-ignored from some-ignored return values. Plan: docs/superpowers/plans/2026-10-01-unused-return-values.md" "" "🤖 Generated with [Claude Code](https://claude.com/claude-code)" "" "https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG")
```

Then read the PR's checks and **unresolved review threads** (GraphQL `reviewThreads{isResolved}`) before reporting READY — green says nothing about threads.
