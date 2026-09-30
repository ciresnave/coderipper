# Reachability check, project scope (Rust) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **IMPLEMENTATION STATUS (2026-09-30, post-implementation, post-review): this plan was executed, and
> real defects found DURING execution — several reversing this document's own original design — mean
> the text below no longer matches what was actually built in several places. Kept as the historical
> record of what was planned (not rewritten in place, so the design's evolution stays visible); read
> this note first, and prefer the actual code + `docs/superpowers/specs/2026-09-30-audit-host-design.md`
> over the plan text wherever they disagree.**
>
> **Corrections, found by implementing, then a second round found by an independent review:**
> 1. **`pub use` is downgraded too** (§ "Global Constraints" below says the opposite). Leaving a
>    re-export at full `pub` while its target drops to `pub(crate)` is a hard compile error (E0364) —
>    confirmed empirically. `rewrite_pub_to_pub_crate` treats `use` as an ordinary declaring keyword.
> 2. **Task 6's fixture is a binary crate** (`src/main.rs`, `fn main()` calling `caller()`), not the
>    lib-crate-with-`pub use ... as public_alias` design shown in Task 6 below. A pure lib crate with
>    everything downgraded has NO live reachability root at all under this mechanism.
> 3. **A function called only from a `#[test]` is NOT rescued from `dead_code`** by rustc (confirmed
>    across many independent, fully-isolated fresh builds). This directly narrows what "a test counts
>    as reachability" (design doc §2) means in practice: it doesn't, for this mechanism, in v1.
> 4. **The biggest gap, found by a fresh reviewer, not by the implementer**: this mechanism cannot
>    successfully analyze ANY package with more than one compilation target (a `lib` + a `bin`, or a
>    `lib` + integration tests) where the other target imports the lib by crate name — downgrading the
>    lib's `pub` items breaks that import with `E0603`. **This is true of CodeRipper's own repo.**
>    `ReachabilityCheck::run` now detects this (see #5) and returns an `Err`, not a silent empty result.
> 5. **A real per-run positive control was added, not in this plan**: a guaranteed-dead "sentinel"
>    function is injected into the rewritten tree before every build. If it isn't detected as dead
>    code, the run is untrustworthy (build failed, or dead_code is suppressed some other way, e.g. a
>    crate-wide `#[allow(dead_code)]`) and `run` returns `Err` — it never reports empty findings from a
>    run it couldn't actually verify. This replaces the fixed narrative string this plan originally
>    specified for `Finding.positive_control`.
> 6. **The review also found, and this document's Review Focus missed**: grouped diagnostics
>    ("methods `a`, `b`, and `c` are never used" — one message, several primary spans) were only
>    partially parsed (first span only); `pub async fn`/`pub unsafe fn`/`pub extern "C" fn` were left
>    at full `pub` (only the word immediately after `pub` was checked against the keyword list); a
>    crate-level `#![deny(warnings)]` promoting `dead_code` to `error` was discarded as "build failed"
>    instead of reported; a panicking `build.rs` producing no rustc JSON at all fell through
>    undetected. All fixed; see the commit history on this branch for each, and the code's own comments
>    for the mechanism.
> 7. **Not fixed, left as documented, known gaps**: analysis is HEAD-only (uncommitted edits in the
>    real working tree aren't seen — this is a property of using a git worktree, not a bug); `--project`
>    pointing at a crate nested inside a larger repo analyzes the wrong root (the enclosing repo's, not
>    the nested crate's); an allowlist entry can't distinguish two same-named items in the same file
>    (e.g. two `fn new` in different `impl` blocks).

**Goal:** Implement the first real check — does every function in a Rust crate get called from
somewhere, including `pub` items that `rustc`'s own `dead_code` lint deliberately exempts.

**Architecture:** `rustc`'s `dead_code` lint already does exactly this analysis correctly for
private/`pub(crate)` items — it's the *exemption* for `pub` that's the gap, not the analysis. So the
mechanism is: in an isolated git worktree (never the caller's real checkout — this portfolio's own
established discipline for exactly this reason), rewrite every top-level `pub` item declaration to
`pub(crate)`, run `cargo build --all-targets --message-format=json` to let the existing lint fire for
real, parse its `dead_code` diagnostics back into `Finding`s, and filter out anything an
`.coderipper.toml` allowlist marks as intentional public API. Four small, independently-testable
pieces (rewriter, worktree runner, diagnostic parser, allowlist filter) compose into one `Check` impl.

**Tech Stack:** Rust. No new external dependencies beyond what's already in `Cargo.toml` except `toml`
(parsing `.coderipper.toml`) and `tempfile` (scratch space for the worktree) — both added in Task 1.

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §2 (this plan implements Sub-pass A
of §3 only — within-project reachability; cross-project, §3 Sub-pass B, is a separate later plan).

## Global Constraints

- Never mutate the caller's real working tree. All source rewriting happens in a throwaway `git
  worktree`, cleaned up even on error (spec's own repos already follow this discipline for the same
  reason — a shared tree must stay read-only to anything destructive).
- The `pub`→`pub(crate)` rewrite is intentionally line-based, not a full `syn`-AST rewrite, for v1.
  This is a scoping decision, not an oversight: it will miss some shapes (see Review Focus). Ship the
  common case correctly; a fuller AST-based rewrite is a later increment if the line-based one proves
  too lossy in practice.
- Every `Finding` this check emits is an absence claim ("N references found: 0") and therefore MUST
  carry a `positive_control` per `Finding::validate` (already enforced by the host, `src/lib.rs`) — the
  control here is: the same rewrite-and-build pipeline finds a *used* `pub` item's warning absent,
  proving the pipeline can see a real symbol when one exists.
- `cargo build --all-targets --message-format=json` runs inside the worktree only; never touch the
  caller's own `target/` directory or its cargo build lock.

## Review Focus

- **A crate that fails to build even before the rewrite** (a real compile error unrelated to this
  check) must not be reported as "N dead items" — the check has to distinguish "rewrite made it
  uncompilable" from "crate was already broken," or it manufactures false findings on every red crate.
- **`pub use` re-exports** must never be rewritten — downgrading a re-export's visibility doesn't test
  "is this item used," it just breaks re-export-based public APIs that the crate's own internal code
  may rely on importing through.
- **A workspace with multiple crates** (most of this portfolio's real Rust projects) needs the rewrite
  applied consistently across every member crate in one build, or a symbol `pub` in crate A and used
  only from crate B in the same workspace reads as dead when it isn't.
- **An item already `pub(crate)`, `pub(super)`, or `pub(in path)`** must be left alone — only bare `pub`
  is a rewrite candidate; the others are already internally-scoped and rewriting them is a no-op at
  best, a broken build at worst if `pub(in path)` syntax isn't preserved correctly by a naive regex.
- **The allowlist must suppress by the SAME identity the finding is reported under** (file + symbol
  name), not by a looser match like "path contains this string" — a loose match could silently
  suppress an unrelated dead item that happens to share a path prefix.

---

## File structure

- Create: `src/checks/mod.rs` — module declaration, re-exports `ReachabilityCheck`.
- Create: `src/checks/reachability/mod.rs` — the `Check` impl, composes the four pieces below.
- Create: `src/checks/reachability/rewriter.rs` — line-based `pub` → `pub(crate)` rewrite.
- Create: `src/checks/reachability/worktree.rs` — throwaway git worktree lifecycle.
- Create: `src/checks/reachability/diagnostics.rs` — `cargo build --message-format=json` → `dead_code`
  diagnostics.
- Create: `src/checks/reachability/allowlist.rs` — `.coderipper.toml` parsing + filter.
- Modify: `src/lib.rs:11` (`registered_checks`) — register `ReachabilityCheck`.
- Test fixture: `tests/fixtures/dead-pub-fn/` — a tiny standalone two-file Rust crate with one
  genuinely dead `pub fn`, one used `pub fn`, and one `pub use` re-export, to exercise Review Focus's
  claims directly.
- Test: `tests/reachability.rs` — integration test running the real check against the fixture.

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `toml` and `tempfile` available as dependencies for the rest of this plan.

- [ ] **Step 1: Add dependencies**

```toml
# append to [dependencies] in Cargo.toml
toml = "0.9"
tempfile = "3"
```

- [ ] **Step 2: Verify the workspace still builds with the new deps present but unused**

Run: `cargo build`
Expected: succeeds (unused-dependency warnings are fine at this step; nothing references them yet)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add toml and tempfile deps for the reachability check"
```

---

### Task 2: The `pub` → `pub(crate)` rewriter (pure function, no I/O)

**Files:**
- Create: `src/checks/reachability/rewriter.rs`
- Test: same file, `#[cfg(test)]`

**Interfaces:**
- Produces: `pub fn rewrite_pub_to_pub_crate(source: &str) -> String` — used by Task 3's worktree runner.

- [ ] **Step 1 (RED): write the failing tests**

```rust
use super::rewrite_pub_to_pub_crate;

#[test]
fn a_bare_pub_fn_is_downgraded() {
    let src = "pub fn dead() {}\n";
    assert_eq!(rewrite_pub_to_pub_crate(src), "pub(crate) fn dead() {}\n");
}

#[test]
fn pub_use_is_left_alone() {
    let src = "pub use crate::inner::Thing;\n";
    assert_eq!(rewrite_pub_to_pub_crate(src), src);
}

#[test]
fn already_scoped_visibility_is_left_alone() {
    let src = "pub(crate) fn a() {}\npub(super) fn b() {}\npub(in crate::x) fn c() {}\n";
    assert_eq!(rewrite_pub_to_pub_crate(src), src);
}

#[test]
fn indented_pub_items_inside_a_module_are_downgraded() {
    let src = "mod inner {\n    pub struct Thing;\n}\n";
    assert_eq!(
        rewrite_pub_to_pub_crate(src),
        "mod inner {\n    pub(crate) struct Thing;\n}\n"
    );
}

#[test]
fn every_declaring_keyword_is_covered() {
    for kw in ["fn", "struct", "enum", "trait", "const", "static", "type", "mod"] {
        let src = format!("pub {kw} x;\n");
        let want = format!("pub(crate) {kw} x;\n");
        assert_eq!(rewrite_pub_to_pub_crate(&src), want, "keyword {kw}");
    }
}
```

- [ ] **Step 2: run, confirm RED**

Run: `cargo test --lib rewriter`
Expected: FAIL — `rewrite_pub_to_pub_crate` not defined

- [ ] **Step 3 (GREEN): implement**

```rust
//! Line-based `pub` -> `pub(crate)` rewrite (design doc §2, plan's Global Constraints: intentionally
//! not a full AST rewrite for v1). Downgrades a bare `pub` immediately before one of the declaring
//! keywords; leaves `pub use`, `pub(crate)`, `pub(super)`, `pub(in ...)` untouched.

const KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "const", "static", "type", "mod",
];

pub fn rewrite_pub_to_pub_crate(source: &str) -> String {
    source
        .lines()
        .map(rewrite_line)
        .collect::<Vec<_>>()
        .join("\n")
        + if source.ends_with('\n') { "\n" } else { "" }
}

fn rewrite_line(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);

    let Some(after_pub) = rest.strip_prefix("pub ") else {
        return line.to_string();
    };
    // Already scoped (`pub(crate)`, `pub(super)`, `pub(in ...)`) or a re-export (`pub use`) -- leave
    // alone. `after_pub` starts right after "pub ", so a scoped visibility shows up as "(...)" here,
    // and `pub(...)` with no space (e.g. "pub(crate)") never matched the "pub " prefix at all.
    if after_pub.starts_with("use ") {
        return line.to_string();
    }

    let next_word = after_pub.split_whitespace().next().unwrap_or("");
    if KEYWORDS.contains(&next_word) {
        format!("{indent}pub(crate) {after_pub}")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // tests from Step 1 go here
}
```

- [ ] **Step 4: run, confirm GREEN**

Run: `cargo test --lib rewriter`
Expected: PASS, all 5 tests (the keyword-coverage test runs 8 sub-cases in one `#[test]`)

- [ ] **Step 5: commit**

```bash
git add src/checks/reachability/rewriter.rs Cargo.toml
git commit -m "feat(reachability): line-based pub -> pub(crate) rewriter"
```

---

### Task 3: Throwaway worktree runner

**Files:**
- Create: `src/checks/reachability/worktree.rs`
- Test: same file, `#[cfg(test)]` — uses a fixture git repo built inline with `tempfile` + `git init`

**Interfaces:**
- Consumes: `rewrite_pub_to_pub_crate` from Task 2.
- Produces: `pub struct RewrittenWorktree { pub root: PathBuf }` with
  `pub fn create(project_root: &Path) -> anyhow::Result<RewrittenWorktree>` (clones `project_root`'s
  `HEAD` into a `tempfile::TempDir`-backed `git worktree`, rewrites every `.rs` file under it) and a
  `Drop` impl that removes the worktree. Used by Task 4's diagnostic collector.

- [ ] **Step 1 (RED): write the failing test**

```rust
use super::RewrittenWorktree;
use std::process::Command;

fn init_fixture_repo(dir: &std::path::Path) {
    Command::new("git").arg("init").arg("-q").current_dir(dir).status().unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn dead() {}\n").unwrap();
    Command::new("git").args(["add", "-A"]).current_dir(dir).status().unwrap();
    Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"])
        .current_dir(dir)
        .status()
        .unwrap();
}

#[test]
fn the_worktree_is_a_rewritten_copy_not_a_mutation_of_the_source() {
    let tmp = tempfile::tempdir().unwrap();
    init_fixture_repo(tmp.path());

    let wt = RewrittenWorktree::create(tmp.path()).unwrap();

    let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
    assert!(rewritten.contains("pub(crate) fn dead"));

    let original = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
    assert_eq!(original, "pub fn dead() {}\n", "the caller's real tree must be untouched");
}

#[test]
fn the_worktree_directory_is_removed_when_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    init_fixture_repo(tmp.path());

    let wt_root = {
        let wt = RewrittenWorktree::create(tmp.path()).unwrap();
        wt.root.clone()
    }; // dropped here

    assert!(!wt_root.exists(), "worktree dir must be cleaned up on drop");
}
```

- [ ] **Step 2: run, confirm RED**

Run: `cargo test --lib worktree`
Expected: FAIL — `RewrittenWorktree` not defined

- [ ] **Step 3 (GREEN): implement**

```rust
use super::rewriter::rewrite_pub_to_pub_crate;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RewrittenWorktree {
    pub root: PathBuf,
    _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
}

impl RewrittenWorktree {
    pub fn create(project_root: &Path) -> anyhow::Result<Self> {
        let scratch = tempfile::tempdir()?;
        let wt_path = scratch.path().join("wt");

        let status = Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&wt_path)
            .arg("HEAD")
            .current_dir(project_root)
            .status()?;
        anyhow::ensure!(status.success(), "git worktree add failed");

        for entry in walk_rs_files(&wt_path.join("src"))? {
            let source = std::fs::read_to_string(&entry)?;
            std::fs::write(&entry, rewrite_pub_to_pub_crate(&source))?;
        }

        Ok(Self {
            root: wt_path,
            _scratch: scratch,
        })
    }
}

impl Drop for RewrittenWorktree {
    fn drop(&mut self) {
        // Best-effort: `git worktree remove` needs the ORIGINAL repo as cwd, which we don't keep a
        // handle to here, so just remove the directory tree. The scratch TempDir's own Drop would do
        // this anyway; doing it explicitly first lets `git worktree prune` (run periodically by
        // callers, not here) reclaim the now-dangling worktree registration in the source repo.
        let _ = std::fs::remove_dir_all(&self.root);
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
    use super::*;
    // tests from Step 1 go here
}
```

- [ ] **Step 4: run, confirm GREEN**

Run: `cargo test --lib worktree`
Expected: PASS, both tests

- [ ] **Step 5: commit**

```bash
git add src/checks/reachability/worktree.rs Cargo.toml Cargo.lock
git commit -m "feat(reachability): throwaway rewritten-worktree lifecycle, never mutates the real tree"
```

---

### Task 4: Diagnostic collector (`cargo build --message-format=json` → `dead_code` hits)

**Files:**
- Create: `src/checks/reachability/diagnostics.rs`
- Test: same file, `#[cfg(test)]`

**Interfaces:**
- Consumes: `RewrittenWorktree` from Task 3.
- Produces: `pub struct DeadCodeHit { pub file: String, pub line: u32, pub symbol: String }` and
  `pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult>` where
  `pub struct CollectResult { pub hits: Vec<DeadCodeHit>, pub build_failed_for_other_reasons: bool }`
  — Review Focus's first item: a build that fails for reasons OTHER than dead_code warnings must set
  `build_failed_for_other_reasons = true` so Task 5's `Check` impl can refuse to report findings from a
  crate that was already broken, rather than manufacture false "dead code" noise from a compile error.

- [ ] **Step 1 (RED): write the failing tests**

```rust
use super::{collect_dead_code, CollectResult};
use std::path::Path;

fn write_fixture(dir: &Path, lib_rs: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), lib_rs).unwrap();
}

#[test]
fn a_dead_pub_crate_item_is_reported_with_its_real_location() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path(), "pub(crate) fn dead() {}\n");

    let CollectResult { hits, build_failed_for_other_reasons } =
        collect_dead_code(tmp.path()).unwrap();

    assert!(!build_failed_for_other_reasons);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].symbol, "dead");
    assert!(hits[0].file.ends_with("lib.rs"));
}

#[test]
fn a_used_pub_crate_item_produces_no_hit_the_positive_control() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(
        tmp.path(),
        "pub(crate) fn used() -> i32 { 1 }\npub(crate) fn caller() -> i32 { used() }\n",
    );

    let CollectResult { hits, .. } = collect_dead_code(tmp.path()).unwrap();
    assert!(hits.is_empty(), "the pipeline must be able to see a real caller when one exists");
}

#[test]
fn a_crate_that_fails_to_compile_for_other_reasons_is_flagged_not_reported_as_dead_code() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path(), "this is not valid rust syntax {{{\n");

    let CollectResult { hits, build_failed_for_other_reasons } =
        collect_dead_code(tmp.path()).unwrap();

    assert!(build_failed_for_other_reasons);
    assert!(hits.is_empty(), "no dead-code claim should be made about a crate that didn't compile");
}
```

- [ ] **Step 2: run, confirm RED**

Run: `cargo test --lib diagnostics`
Expected: FAIL — `collect_dead_code` not defined

- [ ] **Step 3 (GREEN): implement**

```rust
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

pub struct DeadCodeHit {
    pub file: String,
    pub line: u32,
    pub symbol: String,
}

pub struct CollectResult {
    pub hits: Vec<DeadCodeHit>,
    pub build_failed_for_other_reasons: bool,
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
struct CompilerMessage {
    code: Option<CompilerCode>,
    level: String,
    message: String,
    spans: Vec<CompilerSpan>,
}

#[derive(Deserialize)]
struct CompilerCode {
    code: String,
}

#[derive(Deserialize)]
struct CompilerSpan {
    file_name: String,
    line_start: u32,
    is_primary: bool,
}

pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult> {
    let output = Command::new("cargo")
        .args(["build", "--all-targets", "--message-format=json"])
        .current_dir(worktree_root)
        .output()?;

    let mut hits = Vec::new();
    let mut saw_error = false;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<CargoMessage>(line) else {
            continue;
        };
        if msg.reason != "compiler-message" {
            continue;
        }
        let Some(cm) = msg.message else { continue };

        if cm.level == "error" {
            saw_error = true;
            continue;
        }

        let is_dead_code = cm
            .code
            .as_ref()
            .map(|c| c.code == "dead_code")
            .unwrap_or(false);
        if !is_dead_code {
            continue;
        }

        if let Some(span) = cm.spans.iter().find(|s| s.is_primary) {
            hits.push(DeadCodeHit {
                file: span.file_name.clone(),
                line: span.line_start,
                symbol: extract_symbol_name(&cm.message),
            });
        }
    }

    Ok(CollectResult {
        hits,
        build_failed_for_other_reasons: saw_error,
    })
}

/// rustc's dead_code message is like: "function `dead` is never used" -- pull the backtick-quoted name.
fn extract_symbol_name(message: &str) -> String {
    message
        .split('`')
        .nth(1)
        .unwrap_or(message)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    // tests from Step 1 go here
}
```

- [ ] **Step 4: run, confirm GREEN**

Run: `cargo test --lib diagnostics`
Expected: PASS, all 3 tests

- [ ] **Step 5: commit**

```bash
git add src/checks/reachability/diagnostics.rs
git commit -m "feat(reachability): parse cargo's dead_code diagnostics, distinguish them from real compile errors"
```

---

### Task 5: Allowlist

**Files:**
- Create: `src/checks/reachability/allowlist.rs`
- Test: same file, `#[cfg(test)]`

**Interfaces:**
- Consumes: `DeadCodeHit` from Task 4.
- Produces: `pub struct Allowlist { .. }` with `pub fn load(project_root: &Path) -> anyhow::Result<Self>`
  (returns an empty allowlist if `.coderipper.toml` doesn't exist — absence is not an error) and
  `pub fn is_allowed(&self, check_id: &str, file: &str, symbol: &str) -> bool`.

- [ ] **Step 1 (RED): write the failing tests**

```rust
use super::Allowlist;

#[test]
fn missing_allowlist_file_allows_nothing_not_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let al = Allowlist::load(tmp.path()).unwrap();
    assert!(!al.is_allowed("reachability", "src/lib.rs", "anything"));
}

#[test]
fn an_exact_file_and_symbol_match_is_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".coderipper.toml"),
        r#"
[[allow]]
check = "reachability"
file = "src/api.rs"
symbol = "public_entry_point"
reason = "published crate API, consumed outside this portfolio"
"#,
    )
    .unwrap();

    let al = Allowlist::load(tmp.path()).unwrap();
    assert!(al.is_allowed("reachability", "src/api.rs", "public_entry_point"));
    // Review Focus: must match by identity, not loose path containment.
    assert!(!al.is_allowed("reachability", "src/api.rs", "some_other_fn"));
    assert!(!al.is_allowed("reachability", "src/api_v2.rs", "public_entry_point"));
}

#[test]
fn an_entry_with_no_reason_fails_to_load() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".coderipper.toml"),
        r#"
[[allow]]
check = "reachability"
file = "src/api.rs"
symbol = "x"
"#,
    )
    .unwrap();

    assert!(Allowlist::load(tmp.path()).is_err(), "reason is required, per design doc §4");
}
```

- [ ] **Step 2: run, confirm RED**

Run: `cargo test --lib allowlist`
Expected: FAIL — `Allowlist` not defined

- [ ] **Step 3 (GREEN): implement**

```rust
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct AllowlistFile {
    #[serde(rename = "allow", default)]
    entries: Vec<AllowEntry>,
}

#[derive(Deserialize)]
struct AllowEntry {
    check: String,
    file: String,
    symbol: String,
    reason: String, // required: a missing field fails TOML deserialization, which is the point
}

pub struct Allowlist {
    entries: Vec<AllowEntry>,
}

impl Allowlist {
    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = project_root.join(".coderipper.toml");
        if !path.exists() {
            return Ok(Self { entries: Vec::new() });
        }
        let text = std::fs::read_to_string(&path)?;
        let parsed: AllowlistFile = toml::from_str(&text)?;
        Ok(Self {
            entries: parsed.entries,
        })
    }

    pub fn is_allowed(&self, check_id: &str, file: &str, symbol: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.check == check_id && e.file == file && e.symbol == symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // tests from Step 1 go here
}
```

- [ ] **Step 4: run, confirm GREEN**

Run: `cargo test --lib allowlist`
Expected: PASS, all 3 tests

- [ ] **Step 5: commit**

```bash
git add src/checks/reachability/allowlist.rs
git commit -m "feat(reachability): .coderipper.toml allowlist, exact file+symbol match, reason required"
```

---

### Task 6: Wire it together as a `Check`, register it, and prove it end-to-end

**Files:**
- Create: `src/checks/reachability/mod.rs`
- Create: `src/checks/mod.rs`
- Modify: `src/lib.rs`
- Create: `tests/fixtures/dead-pub-fn/Cargo.toml`, `tests/fixtures/dead-pub-fn/src/lib.rs` (the plan's
  own fixture — a real tiny crate, not a temp-dir-generated one, so the integration test is a normal
  file a human can also open and read)
- Test: `tests/reachability.rs`

**Interfaces:**
- Consumes: everything from Tasks 2-5, plus `Check`/`CheckContext`/`Scope`/`Network` from
  `src/check.rs` and `Finding`/`Severity`/`Confidence`/`Location` from `src/finding.rs`.
- Produces: `pub struct ReachabilityCheck;` implementing `Check`, registered in
  `coderipper::registered_checks()`.

- [ ] **Step 1: create the fixture crate**

```toml
# tests/fixtures/dead-pub-fn/Cargo.toml
[package]
name = "dead-pub-fn-fixture"
version = "0.1.0"
edition = "2021"
```

```rust
// tests/fixtures/dead-pub-fn/src/lib.rs
pub fn dead_function() -> i32 {
    42
}

pub fn used_function() -> i32 {
    1
}

pub fn caller() -> i32 {
    used_function()
}

pub use caller as public_alias; // must never be rewritten (Review Focus)
```

This fixture needs its own git history to be worktree-able — it's committed as part of the coderipper
repo itself (a nested, separately-initialized repo is NOT what we want; see Step 2's note).

- [ ] **Step 1b: note on git nesting, resolve before continuing**

`git worktree add` (Task 3) requires the *target* to be a git repo. The fixture crate lives inside
coderipper's own repo, so `Task 3`'s `RewrittenWorktree::create` needs to run with `project_root` = the
fixture's own directory, which is NOT independently a git repo when it's just a subdirectory of
coderipper's checkout. Resolve this by having the integration test (Step 4 below) copy the fixture into
a fresh `tempfile::tempdir()` and `git init` it there first (same pattern already proven in Task 3's own
unit tests), rather than trying to run the check against the fixture in place. Write this helper once,
in `tests/reachability.rs`, not duplicated per test.

- [ ] **Step 2 (RED): write the failing test**

```rust
// tests/reachability.rs
use coderipper::check::{Check, CheckContext};
use coderipper::checks::reachability::ReachabilityCheck;
use std::process::Command;

fn fixture_as_a_git_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let fixture_src = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/dead-pub-fn");
    for name in ["Cargo.toml", "src"] {
        let from = std::path::Path::new(fixture_src).join(name);
        let to = tmp.path().join(name);
        copy_recursive(&from, &to);
    }
    Command::new("git").arg("init").arg("-q").current_dir(tmp.path()).status().unwrap();
    Command::new("git").args(["add", "-A"]).current_dir(tmp.path()).status().unwrap();
    Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    tmp
}

fn copy_recursive(from: &std::path::Path, to: &std::path::Path) {
    if from.is_dir() {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            copy_recursive(&entry.path(), &to.join(entry.file_name()));
        }
    } else {
        std::fs::copy(from, to).unwrap();
    }
}

#[test]
fn the_reachability_check_finds_exactly_the_dead_function() {
    let repo = fixture_as_a_git_repo();
    let ctx = CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    };

    let findings = ReachabilityCheck.run(&ctx).unwrap();

    assert_eq!(findings.len(), 1, "exactly one dead pub item: dead_function");
    assert!(findings[0].summary.contains("dead_function"));
    assert!(findings[0].positive_control.is_some());
    // caller/used_function/public_alias must NOT appear -- they're referenced or a re-export.
    assert!(!findings.iter().any(|f| f.summary.contains("used_function")));
    assert!(!findings.iter().any(|f| f.summary.contains("caller")));
    assert!(!findings.iter().any(|f| f.summary.contains("public_alias")));
}

#[test]
fn an_allowlisted_dead_symbol_is_suppressed() {
    let repo = fixture_as_a_git_repo();
    std::fs::write(
        repo.path().join(".coderipper.toml"),
        r#"
[[allow]]
check = "reachability"
file = "src/lib.rs"
symbol = "dead_function"
reason = "test: confirm the allowlist suppresses a real finding"
"#,
    )
    .unwrap();

    let ctx = CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    };
    let findings = ReachabilityCheck.run(&ctx).unwrap();
    assert!(findings.is_empty());
}
```

- [ ] **Step 3: run, confirm RED**

Run: `cargo test --test reachability`
Expected: FAIL — `coderipper::checks` module doesn't exist yet

- [ ] **Step 4 (GREEN): implement the `Check`, wire modules**

```rust
// src/checks/mod.rs
pub mod reachability;
pub use reachability::ReachabilityCheck;
```

```rust
// src/checks/reachability/mod.rs
mod allowlist;
mod diagnostics;
mod rewriter;
mod worktree;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use allowlist::Allowlist;
use diagnostics::collect_dead_code;
use worktree::RewrittenWorktree;

pub struct ReachabilityCheck;

impl Check for ReachabilityCheck {
    fn id(&self) -> &'static str {
        "reachability"
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let wt = RewrittenWorktree::create(&ctx.project_root)?;
        let result = collect_dead_code(&wt.root)?;

        if result.build_failed_for_other_reasons {
            // Per Review Focus: don't manufacture dead-code findings about a crate that didn't even
            // compile. A future iteration could surface this as its own (non-absence) finding.
            return Ok(Vec::new());
        }

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let project_name = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let findings = result
            .hits
            .into_iter()
            .filter(|hit| !allowlist.is_allowed("reachability", &hit.file, &hit.symbol))
            .map(|hit| Finding {
                check_id: "reachability".into(),
                severity: Severity::Medium,
                confidence: Confidence::Medium,
                project: project_name.clone(),
                location: Some(Location {
                    file: hit.file.clone(),
                    line: Some(hit.line),
                }),
                summary: format!("`{}` has zero callers found within this crate", hit.symbol),
                detail: format!(
                    "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
                     `pub(crate)` in a throwaway worktree, so the lint's normal `pub`-exemption doesn't \
                     hide it. File: {}, line {}.",
                    hit.file, hit.line
                ),
                positive_control: Some(
                    "the same rewrite-and-build pipeline finds no dead_code warning for a pub item \
                     that IS called elsewhere in this crate (see used_function/caller in this check's \
                     own test fixture) -- confirming the pipeline can see a real caller when one exists"
                        .to_string(),
                ),
            })
            .collect();

        Ok(findings)
    }
}
```

```rust
// src/lib.rs -- replace the body of registered_checks()
pub mod checks;

pub fn registered_checks() -> Vec<Box<dyn Check>> {
    vec![Box::new(checks::ReachabilityCheck)]
}
```

- [ ] **Step 5: run, confirm GREEN**

Run: `cargo test`
Expected: PASS, every test in the crate, including the two new integration tests — note this also
re-runs the now-stale `running_with_no_registered_checks_returns_empty_cleanly` test in `src/lib.rs`
(Task's own earlier scaffold), which will FAIL now that a check IS registered; update that test to
assert on `ReachabilityCheck` being present instead of the list being empty, as part of this step.

- [ ] **Step 6: run the CLI against the check's own fixture manually, confirm it reads right**

Run: `cargo run -- check reachability --project tests/fixtures/dead-pub-fn`
Expected: prints one finding line mentioning `dead_function`, nothing about `used_function`/`caller`

- [ ] **Step 7: commit**

```bash
git add src/checks tests/fixtures tests/reachability.rs src/lib.rs
git commit -m "feat(reachability): wire rewriter+worktree+diagnostics+allowlist into a real Check, register it"
```

---

## Self-review notes

- **Spec coverage**: design doc §2's within-project mechanism (pub→pub(crate) trick) — Tasks 2-6.
  §4's allowlist, generalized in the spec but implemented here scoped to this one check first — Task
  5. §2's schema (`Finding`, `positive_control` requirement) — already built in the scaffold, consumed
  correctly in Task 6. Cross-project reachability (§3, Sub-pass B) is explicitly NOT in this plan — a
  separate later plan, per the Spec line above.
- **Placeholder scan**: no TBD/TODO in any step; every step's code is complete and runnable, not
  sketched.
- **Type consistency**: `Check::run` returns `anyhow::Result<Vec<Finding>>` in both `src/check.rs`
  (already built) and Task 6's `ReachabilityCheck` impl — matches. `DeadCodeHit`, `CollectResult`,
  `RewrittenWorktree`, `Allowlist` signatures used in Task 6 match exactly what Tasks 2-5 produce.
- **Review Focus**: all five items have an owning test — a broken-build crate (Task 4, test 3), `pub
  use` untouched (Task 2 test + Task 6 fixture's `public_alias`), workspace-wide consistency (NOT
  covered — flagged here as a known gap: this plan only handles a single-crate project; a
  multi-crate-workspace variant is future work, noted, not silently assumed done), already-scoped
  visibility left alone (Task 2), allowlist exact-identity matching (Task 5, test 2's two negative
  assertions).
