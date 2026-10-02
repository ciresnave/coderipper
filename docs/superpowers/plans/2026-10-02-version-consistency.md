# Version consistency — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CodeRipper's fourth check, `version-consistency`: report every package of a project whose version differs from the
project's version — CireSnave's standing rule, *"I always want all crates within a project to use the same version number so
developers consuming them know which go with which"* — with its one stated exception (a crate that tracks another project's
version) declared and verified rather than silently allowed.

**Architecture:** No worktree and no build. `cargo metadata --no-deps` already lists every workspace member with its resolved
version (`version.workspace = true` included). The check decides what version the project is at, and reports each package that
differs. The tracking exception is a new `[[tracks]]` table in `.coderipper.toml`, read by the check itself: the tracked package
is left out of the comparison and compared with the other project's *current* manifest instead.

**Tech Stack:** Rust 2021, no new dependencies (`toml` and `serde_json` are already used).

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §5 (`version-consistency`: `Project` scope, `LocalOnly`, `fast` tier;
findings `High`/`High`; exception list as a case of §4 where the right value is read from the other project's manifest).
**Scope, per the PM (2026-10-02): this is milestone 1, part 1 of 2** — `ci-protection-presence` is a separate plan and PR after
this one. Out of scope: `dependency-staleness` and the `sweep` runner (the PM's explicit stop), and non-Rust manifests.

## What running it taught (built and probed first, not assumed)

1. **No worktree and no build are needed**, so this check does not use the package-resolution machinery the other three use. A
   *project* here is the whole workspace: `--project` may be a virtual workspace root (which the three package checks refuse) or any
   member directory (cargo finds the workspace from there). Paths in findings are relative to the **workspace root**
   (`c/Cargo.toml`) so they do not depend on which member the user named.
2. **It reads the working tree, not HEAD** (the other checks analyze HEAD in a worktree). Deliberate: this is a rule about what is
   about to be released, and the PM allocates versions at gate time. Documented in the module doc and README.
3. **The tracking exception cannot be an ordinary allowlist entry.** An `[[allow]]` entry suppresses a finding; a tracked package
   must be *compared* with another manifest and reported if it drifted. Modelled as an allow entry it would suppress the drift
   finding it exists to raise, and an entry for a package that raises no finding is reported stale. So `[[tracks]]` is a separate
   table (`package`, `manifest`, `reason`, all required) — a declared relationship, not a suppression. Ordinary silencing of a
   package still uses `[[allow]]` through the host, including stale detection (a test covers it).
4. **The absence-word trap.** `Finding::validate` demands a `positive_control` whenever a summary contains `"0 "` — and
   `"... version 0.4.1 but ..."` does, by accident. Rather than contort the wording, every finding here carries a real control: how
   many manifests were read and how many agree with the project's version, i.e. evidence the comparison could see agreement as well as
   difference.
5. **Acceptance on the real portfolio (read-only clone of `fuel`, `d1bfe127`):** unmodified → "no issues found" (`cargo metadata` lists 43 packages, every one at
   `0.12.0` from `[workspace.package]`); with `fuel-ir` pinned to `0.11.0` in the scratch clone →
   ``[High/High] fuelclone — `fuel-ir` is at version 0.11.0 but this project is at 0.12.0``; CodeRipper itself (one package) → clean.

## Design decisions (rulings, with what each costs if wrong)

1. **The project's version** is `[workspace.package] version` when the workspace root defines one; otherwise the version most
   packages are at; a tie goes to the **highest** version (numeric, a release beats its pre-release). *Cost if wrong:* with no
   `[workspace.package]` and an even split, the lower half is reported — arguably right (a project that is half-bumped is behind), and
   deterministic; the detail text states the basis so a human can overrule it.
2. **A single-package project, or one where only one package is left after tracking exceptions, has nothing to be consistent with** and is clean.
3. **A `[[tracks]]` entry that cannot be honoured is an error, never a pass:** the referenced manifest unreadable, unparsable or
   declaring no version; or the entry naming a package that is not in the project. (A check that cannot see must not report clean.)
   The referenced version is read from `[package] version`, or `[workspace.package] version` for a virtual workspace manifest.
4. **Severity `High`, confidence `High`** (spec §5): a version is read, not inferred.
5. **`Finding.subject` = the package name; `location.file` = its manifest relative to the workspace root; `line` = none.** An `[[allow]]`
   entry therefore reads `check = "version-consistency"`, `file = "c/Cargo.toml"`, `symbol = "c"`.
6. **`reason` is required on `[[tracks]]`** and is shown in the drift finding (it is what makes the field "read" and what a reader needs).
7. **Rust (cargo) manifests only.** Other ecosystems' manifests are a later extension; stated in the module doc, README and spec.
8. **No version bump in this PR** — the PM allocates it at gate time.

## Global Constraints

- Rust edition 2021; CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --all-targets`,
  `cargo test --no-fail-fast` on **ubuntu, windows and macos**. The branch tip must pass all four.
- A test must never point a check at the shared checkout. This check does not write anywhere, but its tests still use throwaway
  tempdirs.
- A check that cannot read what it must compare returns `Err`, never `Ok(vec![])`: an empty result must always mean "consistent".
- `Check::run` returns RAW findings; the host applies the `[[allow]]` allowlist. The check reads `[[tracks]]` itself because it is
  not a suppression (see item 3 above).
- Do **not** bump any version in `Cargo.toml`; ask the PM for the number at gate time.
- Clone with `-c core.autocrlf=false` on this Windows box; never `checkout` in a shared tree.
- Backslash literals via a shell heredoc can be dropped or doubled here — use the editor/Write tool.
- Commit trailers (this lane): end each commit message with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. Do not run `gh auth switch` (the lane account cannot
  open or merge PRs here; the PM does).

## Review Focus

1. **The rule must see agreement as well as difference** — a negative control (all equal → clean) next to every positive one. → `packages_at_one_version_are_clean_the_negative_control`, `one_package_at_another_version_is_reported_the_positive_control` (Task 3), `packages_that_agree_produce_nothing` (Task 2).
2. **Which version is "the project's"**: workspace version beats a majority; majority beats a minority; ties are deterministic. → `the_workspace_package_version_is_the_project_version`, `without_a_workspace_version_the_majority_decides_and_a_tie_goes_to_the_highest` (Task 3), `a_tie_goes_to_the_highest_version`, `the_workspace_package_version_is_authoritative_even_against_a_majority`, `versions_order_numerically_and_a_release_beats_its_prerelease` (Task 2).
3. **The exception must be verified, not just allowed**: matching reference → clean and not an outlier; drifted → reported with the reason; unreadable reference or unknown package → error. → `a_tracking_package_that_matches_its_reference_is_clean_and_not_an_outlier`, `a_tracking_package_that_drifted_from_its_reference_is_reported`, `an_unreadable_reference_is_an_error_never_a_pass`, `a_tracks_entry_for_a_package_that_does_not_exist_is_an_error` (Task 3).
4. **Where the project is** — a member directory sees the whole workspace; a directory that is not a cargo project is an error; a single package is clean. → `a_member_directory_is_enough_to_see_the_whole_workspace`, `a_single_package_project_is_clean`, `a_directory_that_is_not_a_cargo_project_is_an_error_not_clean` (Task 3).
5. **Ordinary suppression still works and still goes stale** through the host. → `an_allowlist_entry_silences_one_package_and_goes_stale_when_the_versions_agree` (Task 3).

**Known gaps accepted (documented):** non-Rust manifests; a project whose "version" lives in a `package.json`/`pyproject.toml`;
pre-1.0 vs 1.0 semantics of "breaking" are not judged (this checks equality only); a package with `publish = false` is compared like any
other (use `[[allow]]` if it should differ).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/allowlist.rs` | + `[[tracks]]` entries (`package`, `manifest`, `reason`) and `Allowlist::tracks()` |
| `src/package.rs` | `Package` gains `version` and `manifest_path` |
| `src/checks/version_consistency/mod.rs` (new) | The check, the pure `evaluate`, version ordering |
| `tests/version_consistency.rs` (new), `tests/cli.rs` | End-to-end with real `cargo metadata`; CLI |

Baseline before starting (`origin/main` at `627f001`): `cargo test` shows 125 lib tests, 8 in `tests/allowlist.rs`, 10 in `tests/cli.rs`, 20 in
`tests/reachability.rs`, 11 in `tests/unused_parameters.rs`, 13 in `tests/unused_return_values.rs`, 13 in `tests/workspace.rs`.

---

### Task 1: `[[tracks]]` entries and a package's version

**Files:** Modify `src/allowlist.rs`, `src/package.rs`.

**Interfaces:**
- Produces: `allowlist::TrackEntry { package, manifest, reason }` (all `pub`, all required) and `Allowlist::tracks() -> &[TrackEntry]`; `[[tracks]]` is a
  new optional top-level table (the file's unknown-key check stays on). `package::Package` gains `version: String` and `manifest_path: PathBuf`.

- [ ] **Step 1: Write the failing tests first.**

````diff
--- a/src/allowlist.rs
+++ b/src/allowlist.rs
@@ -133,6 +133,38 @@
     }
 
     #[test]
+    fn tracks_entries_load_with_package_manifest_and_reason() {
+        let tmp = tempfile::tempdir().unwrap();
+        std::fs::write(
+            tmp.path().join(".coderipper.toml"),
+            "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"works with other\"\n",
+        )
+        .unwrap();
+        let al = Allowlist::load(tmp.path()).unwrap();
+        assert_eq!(al.tracks().len(), 1);
+        assert_eq!(al.tracks()[0].package, "emit");
+        assert_eq!(al.tracks()[0].manifest, "../other/Cargo.toml");
+        assert!(
+            al.entries().is_empty(),
+            "a [[tracks]] entry is not a suppression"
+        );
+    }
+
+    #[test]
+    fn a_tracks_entry_with_no_reason_fails_to_load() {
+        let tmp = tempfile::tempdir().unwrap();
+        std::fs::write(
+            tmp.path().join(".coderipper.toml"),
+            "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\n",
+        )
+        .unwrap();
+        assert!(
+            Allowlist::load(tmp.path()).is_err(),
+            "every exception says why"
+        );
+    }
+
+    #[test]
     fn an_entry_with_no_reason_fails_to_load() {
         let tmp = tempfile::tempdir().unwrap();
         std::fs::write(
````

````diff
--- a/src/package.rs
+++ b/src/package.rs
@@ -197,6 +197,20 @@
     }
 
     #[test]
+    fn a_packages_version_and_manifest_path_are_read() {
+        let ws = virtual_workspace();
+        let meta = metadata(ws.path()).unwrap();
+        let a = meta.packages.iter().find(|p| p.name == "a").unwrap();
+        assert_eq!(a.version, "0.1.0");
+        assert!(
+            a.manifest_path.ends_with("Cargo.toml"),
+            "{:?}",
+            a.manifest_path
+        );
+        assert!(a.manifest_path.starts_with(&a.dir));
+    }
+
+    #[test]
     fn the_git_prefix_is_empty_at_the_root_and_the_relative_path_below_it() {
         let ws = virtual_workspace();
         assert_eq!(git_prefix(ws.path()).unwrap(), "");
````

  Run: `cargo test --lib`
  Expected: FAIL to compile — `no method named tracks`, `no field version / manifest_path on Package`.

- [ ] **Step 2: Implement.**

````diff
--- a/src/allowlist.rs
+++ b/src/allowlist.rs
@@ -6,6 +6,19 @@
 struct AllowlistFile {
     #[serde(rename = "allow", default)]
     entries: Vec<AllowEntry>,
+    #[serde(default)]
+    tracks: Vec<TrackEntry>,
+}
+
+/// A declared relationship, not a suppression: `package` is exempt from the one-version rule because
+/// it works with another project, and must match the version in that project's manifest instead
+/// (`version-consistency`, design doc section 5). `manifest` is relative to the project root.
+#[derive(Debug, Deserialize)]
+pub(crate) struct TrackEntry {
+    pub package: String,
+    pub manifest: String,
+    /// Required, like an allowlist entry's: every exception says why.
+    pub reason: String,
 }
 
 #[derive(Debug, Deserialize)]
@@ -20,31 +33,36 @@
 
 pub struct Allowlist {
     entries: Vec<AllowEntry>,
+    tracks: Vec<TrackEntry>,
 }
 
 impl Allowlist {
     pub fn empty() -> Self {
         Self {
             entries: Vec::new(),
+            tracks: Vec::new(),
         }
     }
 
     pub fn load(project_root: &Path) -> anyhow::Result<Self> {
         let path = project_root.join(".coderipper.toml");
         if !path.exists() {
-            return Ok(Self {
-                entries: Vec::new(),
-            });
+            return Ok(Self::empty());
         }
         let text = std::fs::read_to_string(&path)?;
         let parsed: AllowlistFile = toml::from_str(&text)?;
         Ok(Self {
             entries: parsed.entries,
+            tracks: parsed.tracks,
         })
     }
 
     pub fn entries(&self) -> &[AllowEntry] {
         &self.entries
+    }
+
+    pub(crate) fn tracks(&self) -> &[TrackEntry] {
+        &self.tracks
     }
 }
 
````

````diff
--- a/src/package.rs
+++ b/src/package.rs
@@ -29,6 +29,9 @@
 #[derive(Debug, Clone)]
 pub(crate) struct Package {
     pub name: String,
+    /// The version in the package's manifest (`workspace = true` already resolved by cargo).
+    pub version: String,
+    pub manifest_path: PathBuf,
     /// cargo's package id, as it appears in `compiler-message` lines (`package_id`).
     pub id: String,
     pub dir: PathBuf,
@@ -56,12 +59,13 @@
         .into_iter()
         .flatten()
         .filter_map(|p| {
+            let manifest_path = PathBuf::from(p["manifest_path"].as_str()?);
             Some(Package {
                 name: p["name"].as_str()?.to_string(),
+                version: p["version"].as_str()?.to_string(),
                 id: p["id"].as_str()?.to_string(),
-                dir: Path::new(p["manifest_path"].as_str()?)
-                    .parent()?
-                    .to_path_buf(),
+                dir: manifest_path.parent()?.to_path_buf(),
+                manifest_path,
             })
         })
         .collect();
````

- [ ] **Step 3: Run the suite.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 128 lib tests (125 + 3), every integration count unchanged. (Expect one `dead_code` warning: `TrackEntry.reason` is read in Task 2.)

- [ ] **Step 4: Commit** — `git add -A src && git commit -m "feat: [[tracks]] entries in .coderipper.toml; a package's version and manifest path"` (+ trailers).

---

### Task 2: The check

**Files:** Create `src/checks/version_consistency/mod.rs`; modify `src/checks/mod.rs`, `src/lib.rs`.

**Interfaces:**
- Consumes: `Allowlist::{load, tracks}`, `package::{metadata, Package}`.
- Produces: `checks::VersionConsistencyCheck` (id `"version-consistency"`, `Scope::Project`, `Network::LocalOnly`); `CHECK_ID`. Pure helpers (crate-visible, unit-tested): `Pkg { name, version, manifest }`, `Tracked { version, manifest, reason }`,
  `evaluate(&[Pkg], workspace_version: Option<&str>, &BTreeMap<String, Tracked>) -> Vec<Problem>`, `compare_versions(&str, &str) -> Ordering`, `version_in_manifest(&Path) -> anyhow::Result<String>`.
  Findings: `Severity::High`, `Confidence::High`, `subject` = package name, `location.file` = manifest relative to the workspace root, a `positive_control` always.

- [ ] **Step 1: Declare the module and write the tests first.** In `src/checks/mod.rs` add `pub mod version_consistency;` after `pub mod unused_return_values;`. Create
  `src/checks/version_consistency/mod.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from the full file in Step 2.
  Run: `cargo test --lib version_consistency`
  Expected: FAIL to compile — `cannot find type Pkg / Tracked`, `cannot find function compare_versions`.

- [ ] **Step 2: Replace the file with the full implementation:**

````rust
// src/checks/version_consistency/mod.rs
//! Version-consistency check (project scope, local): do all of a project's packages share one version?
//!
//! CireSnave's standing rule: every crate in a project carries the same version number, so a consumer
//! can tell which ones go together. One narrow exception: a crate that exists to work with a version of
//! ANOTHER project (an emitter crate tracking the project it targets) keeps that project's version
//! instead. Mechanism, and nothing else: read every package's version with `cargo metadata`, decide
//! which version the project is at, and report each package that differs.
//!
//! - **The project's version** is `[workspace.package] version` when the workspace root defines one;
//!   otherwise the version most packages are at (a tie goes to the highest version).
//! - **A tracking exception** is declared in `.coderipper.toml` as a `[[tracks]]` entry: the package
//!   is left out of the uniformity comparison and is instead compared with the version in the other
//!   project's CURRENT manifest. An unreadable reference is an error, never a pass.
//! - A package may also be silenced with an ordinary `[[allow]]` entry (check `version-consistency`,
//!   file = its manifest relative to the workspace root, symbol = the package name), which the host
//!   applies and reports when it goes stale.
//!
//! Reads the working tree, not HEAD: this is a rule about what is about to be released.
//! Rust (cargo) manifests only.

use crate::allowlist::Allowlist;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::package::{metadata, Metadata};
use std::collections::BTreeMap;
use std::path::Path;

pub const CHECK_ID: &str = "version-consistency";

pub struct VersionConsistencyCheck;

impl Check for VersionConsistencyCheck {
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
        let meta = metadata(&ctx.project_root)?;
        anyhow::ensure!(
            !meta.packages.is_empty(),
            "cargo metadata found no packages in {}, so there is nothing to compare",
            ctx.project_root.display()
        );
        let workspace_root = meta.workspace_root.canonicalize()?;
        let packages: Vec<Pkg> = meta.packages_for_comparison(&workspace_root);

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let mut tracked: BTreeMap<String, Tracked> = BTreeMap::new();
        for entry in allowlist.tracks() {
            anyhow::ensure!(
                packages.iter().any(|p| p.name == entry.package),
                "[[tracks]] names package `{}`, which is not a package of this project (members: {})",
                entry.package,
                packages
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let manifest = ctx.project_root.join(&entry.manifest);
            tracked.insert(
                entry.package.clone(),
                Tracked {
                    version: version_in_manifest(&manifest)?,
                    manifest: entry.manifest.clone(),
                    reason: entry.reason.clone(),
                },
            );
        }

        let workspace_version = workspace_package_version(&workspace_root)?;
        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(evaluate(&packages, workspace_version.as_deref(), &tracked)
            .into_iter()
            .map(|p| p.into_finding(&project))
            .collect())
    }
}

/// One package as this check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pkg {
    pub name: String,
    pub version: String,
    /// Manifest path relative to the workspace root, forward slashes.
    pub manifest: String,
}

/// What a `[[tracks]]` entry resolved to: the version of the referenced manifest, read just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tracked {
    pub version: String,
    pub manifest: String,
    /// Why the exception exists, as the `[[tracks]]` entry says.
    pub reason: String,
}

impl Metadata {
    fn packages_for_comparison(&self, workspace_root: &Path) -> Vec<Pkg> {
        let mut out: Vec<Pkg> = self
            .packages
            .iter()
            .map(|p| Pkg {
                name: p.name.clone(),
                version: p.version.clone(),
                manifest: p
                    .manifest_path
                    .canonicalize()
                    .ok()
                    .and_then(|m| {
                        m.strip_prefix(workspace_root)
                            .ok()
                            .map(|r| r.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
                    })
                    .unwrap_or_else(|| p.manifest_path.to_string_lossy().replace('\\', "/")),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// `[workspace.package] version` of the manifest at `workspace_root`, when there is one.
fn workspace_package_version(workspace_root: &Path) -> anyhow::Result<Option<String>> {
    let text = std::fs::read_to_string(workspace_root.join("Cargo.toml"))?;
    let table: toml::Table = toml::from_str(&text)?;
    Ok(table
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// The version a manifest declares: `[package] version`, or, when that is `{ workspace = true }`
/// (or the manifest is a virtual workspace root), `[workspace.package] version`.
pub(crate) fn version_in_manifest(path: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {} to find its version: {e}", path.display()))?;
    let table: toml::Table = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", path.display()))?;
    let direct = table
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str());
    let inherited = table
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str());
    direct
        .or(inherited)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{} declares no version", path.display()))
}

/// How a version problem arose.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The package differs from the version the rest of the project is at.
    Outlier {
        pkg: Pkg,
        expected: String,
        basis: String,
        all: Vec<(String, String)>,
    },
    /// A package that tracks another project does not match that project's manifest.
    TrackMismatch { pkg: Pkg, tracked: Tracked },
}

pub(crate) fn evaluate(
    packages: &[Pkg],
    workspace_version: Option<&str>,
    tracked: &BTreeMap<String, Tracked>,
) -> Vec<Problem> {
    let mut problems = Vec::new();

    for pkg in packages {
        if let Some(t) = tracked.get(&pkg.name) {
            if pkg.version != t.version {
                problems.push(Problem::TrackMismatch {
                    pkg: pkg.clone(),
                    tracked: t.clone(),
                });
            }
        }
    }

    let uniform: Vec<&Pkg> = packages
        .iter()
        .filter(|p| !tracked.contains_key(&p.name))
        .collect();
    let all: Vec<(String, String)> = uniform
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();
    let (expected, basis) = match workspace_version {
        Some(v) => (v.to_string(), "`[workspace.package] version`".to_string()),
        None if uniform.len() < 2 => return problems,
        None => {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for p in &uniform {
                *counts.entry(p.version.as_str()).or_default() += 1;
            }
            let best = counts
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| compare_versions(a.0, b.0)))
                .map(|(v, n)| (v.to_string(), *n))
                .expect("at least two packages");
            let basis = format!(
                "the version most packages are at ({} of {}; a tie goes to the highest)",
                best.1,
                uniform.len()
            );
            (best.0, basis)
        }
    };
    for pkg in uniform {
        if pkg.version != expected {
            problems.push(Problem::Outlier {
                pkg: pkg.clone(),
                expected: expected.clone(),
                basis: basis.clone(),
                all: all.clone(),
            });
        }
    }
    problems
}

/// `major.minor.patch[-pre]` ordering; anything unparsable sorts below everything parsable, and ties
/// fall back to plain text order so the result is deterministic.
pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(v: &str) -> Option<(u64, u64, u64, bool)> {
        let (core, pre) = match v.split_once('-') {
            Some((c, _)) => (c, true),
            None => (v.split_once('+').map_or(v, |(c, _)| c), false),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??, !pre))
    }
    match (key(a), key(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

impl Problem {
    fn into_finding(self, project: &str) -> Finding {
        match self {
            Problem::Outlier {
                pkg,
                expected,
                basis,
                all,
            } => {
                let listing = all
                    .iter()
                    .map(|(n, v)| format!("{n} {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let at_expected = all.iter().filter(|(_, v)| *v == expected).count();
                Finding {
                    check_id: CHECK_ID.into(),
                    severity: Severity::High,
                    confidence: Confidence::High,
                    project: project.to_string(),
                    location: Some(Location {
                        file: pkg.manifest.clone(),
                        line: None,
                    }),
                    subject: Some(pkg.name.clone()),
                    summary: format!(
                        "`{}` is at version {} but this project is at {expected}",
                        pkg.name, pkg.version
                    ),
                    detail: format!(
                        "Every package of a project carries one version so a consumer knows which go \
                         together. The project's version is {basis}. Packages compared: {listing}. A \
                         package that exists to work with another project's version is declared in \
                         `.coderipper.toml` with a `[[tracks]]` entry."
                    ),
                    positive_control: Some(format!(
                        "read {} package manifest(s) with `cargo metadata`; {at_expected} of them are \
                         at {expected}, so the comparison could see agreement as well as difference",
                        all.len()
                    )),
                }
            }
            Problem::TrackMismatch { pkg, tracked } => Finding {
                check_id: CHECK_ID.into(),
                severity: Severity::High,
                confidence: Confidence::High,
                project: project.to_string(),
                location: Some(Location {
                    file: pkg.manifest.clone(),
                    line: None,
                }),
                subject: Some(pkg.name.clone()),
                summary: format!(
                    "`{}` is declared to track {} but is at version {} while that manifest is at {}",
                    pkg.name, tracked.manifest, pkg.version, tracked.version
                ),
                detail: format!(
                    "`{}` is exempt from the one-version rule because it works with another \
                     project ({}), so it must match THAT project's current version, read from {} \
                     just now.",
                    pkg.name, tracked.reason, tracked.manifest
                ),
                positive_control: Some(format!(
                    "{} was read successfully and declares version {}",
                    tracked.manifest, tracked.version
                )),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> Pkg {
        Pkg {
            name: name.into(),
            version: version.into(),
            manifest: format!("{name}/Cargo.toml"),
        }
    }

    fn outliers(problems: &[Problem]) -> Vec<&str> {
        problems
            .iter()
            .filter_map(|p| match p {
                Problem::Outlier { pkg, .. } => Some(pkg.name.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn packages_that_agree_produce_nothing() {
        let pkgs = [pkg("a", "1.2.3"), pkg("b", "1.2.3"), pkg("c", "1.2.3")];
        assert!(evaluate(&pkgs, None, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn the_minority_is_the_outlier_when_there_is_no_workspace_version() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("c", "0.2.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["c"]
        );
    }

    #[test]
    fn a_tie_goes_to_the_highest_version() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.2.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["a"]
        );
        let pkgs = [pkg("a", "0.9.0"), pkg("b", "0.10.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["a"]
        );
    }

    #[test]
    fn the_workspace_package_version_is_authoritative_even_against_a_majority() {
        let pkgs = [pkg("a", "2.0.0"), pkg("b", "2.0.0"), pkg("c", "1.0.0")];
        let found = evaluate(&pkgs, Some("1.0.0"), &BTreeMap::new());
        assert_eq!(outliers(&found), vec!["a", "b"]);
    }

    #[test]
    fn a_single_package_has_nothing_to_be_consistent_with() {
        assert!(evaluate(&[pkg("only", "0.1.0")], None, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn a_tracking_package_is_compared_with_its_reference_and_left_out_of_the_majority() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("emit", "7.0.0")];
        let mut tracked = BTreeMap::new();
        tracked.insert(
            "emit".to_string(),
            Tracked {
                version: "7.0.0".into(),
                manifest: "../other/Cargo.toml".into(),
                reason: "emits for other".into(),
            },
        );
        assert!(evaluate(&pkgs, None, &tracked).is_empty());

        tracked.get_mut("emit").unwrap().version = "7.1.0".into();
        let found = evaluate(&pkgs, None, &tracked);
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0], Problem::TrackMismatch { pkg, .. } if pkg.name == "emit"));
    }

    #[test]
    fn versions_order_numerically_and_a_release_beats_its_prerelease() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0-rc.1"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("1.0.0", "not-a-version"), Greater);
    }

    #[test]
    fn a_finding_validates_and_names_the_package_and_both_versions() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("c", "0.2.0")];
        let problem = evaluate(&pkgs, None, &BTreeMap::new()).remove(0);
        let finding = problem.into_finding("proj");
        assert_eq!(finding.subject.as_deref(), Some("c"));
        assert!(finding.summary.contains("0.2.0") && finding.summary.contains("0.1.0"));
        assert_eq!(finding.location.as_ref().unwrap().file, "c/Cargo.toml");
        assert!(finding.validate().is_ok());
    }
}
````

- [ ] **Step 3: Register it.** `src/checks/mod.rs`: add `pub use version_consistency::VersionConsistencyCheck;` after the `UnusedReturnValuesCheck` re-export. `src/lib.rs`: add
  `Box::new(checks::VersionConsistencyCheck),` after the `UnusedParametersCheck` entry of `registered_checks()`, and update the test that pins the list to
  `vec!["reachability", "unused-return-values", "unused-parameters", "version-consistency"]`.

- [ ] **Step 4: Run the tests.** Run: `cargo fmt && cargo test --lib`
  Expected: PASS — 136 lib tests (128 + 8). `cargo clippy --all-targets -- -D warnings`: no output.

- [ ] **Step 5: Prove four guards can fail.** (a) Replace `match workspace_version {` in `evaluate` with `match workspace_version.filter(|_| false) {`: `the_workspace_package_version_is_authoritative_even_against_a_majority` must FAIL. (b) Change `.filter(|p| !tracked.contains_key(&p.name))` to `.filter(|_| true)`: the two tracking tests must FAIL. (c) Swap the arguments of `compare_versions` in the tie-break (`compare_versions(b.0, a.0)`): `a_tie_goes_to_the_highest_version` must FAIL. (d) Change `if pkg.version != t.version {` to `if false && pkg.version != t.version {`: the tracking-drift tests must FAIL. Revert each.

- [ ] **Step 6: Commit** — `git add -A src && git commit -m "feat: version-consistency check"` (+ trailers).

---

### Task 3: End-to-end tests with real `cargo metadata`, CLI, docs, acceptance

**Files:** Create `tests/version_consistency.rs`; modify `tests/cli.rs`, `README.md`, `docs/superpowers/specs/2026-09-30-audit-host-design.md`.

- [ ] **Step 1: Write the end-to-end tests.** They pass once Task 2 is in (the check is the thing under test); the negative controls (`..._are_clean_the_negative_control`, the matching-reference case, the single package) are as important as the positive ones. Create `tests/version_consistency.rs`:

````rust
// tests/version_consistency.rs
//! `version-consistency` against real `cargo metadata`: each rule has a case that must report and a
//! control that must stay quiet. No git and no build are involved; the check reads manifests.

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::VersionConsistencyCheck;
use coderipper::finding::Finding;
use std::path::Path;

fn write(root: &Path, files: &[(&str, String)]) {
    for (name, contents) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

fn member(name: &str, version: &str) -> (String, String) {
    (
        format!("{name}/Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
}

/// A virtual workspace whose members are `(name, version)`, each with an empty lib.
fn workspace(members: &[(&str, &str)], extra_root: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let names: Vec<String> = members.iter().map(|(n, _)| format!("\"{n}\"")).collect();
    let mut files = vec![(
        "Cargo.toml".to_string(),
        format!(
            "[workspace]\nmembers = [{}]\nresolver = \"2\"\n{extra_root}",
            names.join(", ")
        ),
    )];
    for (name, version) in members {
        let (path, manifest) = member(name, version);
        files.push((path, manifest));
        files.push((format!("{name}/src/lib.rs"), String::new()));
    }
    let borrowed: Vec<(&str, String)> =
        files.iter().map(|(n, c)| (n.as_str(), c.clone())).collect();
    write(tmp.path(), &borrowed);
    tmp
}

fn run(project: &Path) -> anyhow::Result<Vec<Finding>> {
    VersionConsistencyCheck.run(&CheckContext {
        project_root: project.to_path_buf(),
        portfolio_root: project.to_path_buf(),
    })
}

fn subjects(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

#[test]
fn packages_at_one_version_are_clean_the_negative_control() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.2")], "");
    assert!(run(ws.path()).unwrap().is_empty());
}

#[test]
fn one_package_at_another_version_is_reported_the_positive_control() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    let found = run(ws.path()).unwrap();
    assert_eq!(subjects(&found), vec!["c"]);
    let f = &found[0];
    assert_eq!(f.severity, coderipper::finding::Severity::High);
    assert_eq!(f.confidence, coderipper::finding::Confidence::High);
    assert_eq!(f.location.as_ref().unwrap().file, "c/Cargo.toml");
    assert!(
        f.summary.contains("0.4.1") && f.summary.contains("0.4.2"),
        "{}",
        f.summary
    );
    assert!(f.clone().validate().is_ok());
}

#[test]
fn the_workspace_package_version_is_the_project_version() {
    // two members inherit 1.0.0, one pins 2.0.0: the pinned one is the outlier even though the
    // inheriting ones are only two of three
    let tmp = tempfile::tempdir().unwrap();
    let inheriting = |n: &str| {
        (
            format!("{n}/Cargo.toml"),
            format!("[package]\nname = \"{n}\"\nversion.workspace = true\nedition = \"2021\"\n"),
        )
    };
    let pinned = member("pinned", "2.0.0");
    let (ia, ib) = (inheriting("a"), inheriting("b"));
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\", \"pinned\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"1.0.0\"\n"
                    .to_string(),
            ),
            (ia.0.as_str(), ia.1.clone()),
            ("a/src/lib.rs", String::new()),
            (ib.0.as_str(), ib.1.clone()),
            ("b/src/lib.rs", String::new()),
            (pinned.0.as_str(), pinned.1.clone()),
            ("pinned/src/lib.rs", String::new()),
        ],
    );
    assert_eq!(subjects(&run(tmp.path()).unwrap()), vec!["pinned"]);
}

#[test]
fn without_a_workspace_version_the_majority_decides_and_a_tie_goes_to_the_highest() {
    let ws = workspace(&[("a", "0.1.0"), ("b", "0.1.0"), ("c", "0.2.0")], "");
    assert_eq!(subjects(&run(ws.path()).unwrap()), vec!["c"]);
    let tie = workspace(&[("a", "0.1.0"), ("b", "0.2.0")], "");
    assert_eq!(subjects(&run(tie.path()).unwrap()), vec!["a"]);
}

#[test]
fn a_single_package_project_is_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let (path, manifest) = member("solo", "0.9.9");
    write(
        tmp.path(),
        &[
            (path.as_str(), manifest),
            ("solo/src/lib.rs", String::new()),
        ],
    );
    assert!(run(&tmp.path().join("solo")).unwrap().is_empty());
}

#[test]
fn a_member_directory_is_enough_to_see_the_whole_workspace() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    // paths are relative to the WORKSPACE root even when the project is one member's directory
    let found = run(&ws.path().join("a")).unwrap();
    assert_eq!(subjects(&found), vec!["c"]);
    assert_eq!(found[0].location.as_ref().unwrap().file, "c/Cargo.toml");
}

// ---- tracking exceptions ----

fn tracked_fixture(emit_version: &str, reference_version: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "proj/Cargo.toml",
                "[workspace]\nmembers = [\"core\", \"core-two\", \"emit\"]\nresolver = \"2\"\n".to_string(),
            ),
            member_at("proj/core", "0.3.0"),
            ("proj/core/src/lib.rs", String::new()),
            member_at("proj/core-two", "0.3.0"),
            ("proj/core-two/src/lib.rs", String::new()),
            member_at("proj/emit", emit_version),
            ("proj/emit/src/lib.rs", String::new()),
            (
                "other/Cargo.toml",
                format!("[package]\nname = \"other\"\nversion = \"{reference_version}\"\nedition = \"2021\"\n"),
            ),
            ("other/src/lib.rs", String::new()),
            (
                "proj/.coderipper.toml",
                "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"emits for other\"\n"
                    .to_string(),
            ),
        ],
    );
    tmp
}

fn member_at(dir: &str, version: &str) -> (&'static str, String) {
    let name = dir.rsplit('/').next().unwrap();
    let path: &'static str = Box::leak(format!("{dir}/Cargo.toml").into_boxed_str());
    (
        path,
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
}

#[test]
fn a_tracking_package_that_matches_its_reference_is_clean_and_not_an_outlier() {
    // emit is at 7.0.0 and everything else at 0.3.0: without the exception it would be flagged
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    assert!(run(&tmp.path().join("proj")).unwrap().is_empty());
}

#[test]
fn a_tracking_package_that_drifted_from_its_reference_is_reported() {
    let tmp = tracked_fixture("7.0.0", "7.1.0");
    let found = run(&tmp.path().join("proj")).unwrap();
    assert_eq!(subjects(&found), vec!["emit"]);
    assert!(
        found[0].summary.contains("7.1.0") && found[0].summary.contains("7.0.0"),
        "{}",
        found[0].summary
    );
    assert!(
        found[0].detail.contains("emits for other"),
        "the reason must be shown: {}",
        found[0].detail
    );
}

#[test]
fn an_unreadable_reference_is_an_error_never_a_pass() {
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    std::fs::remove_file(tmp.path().join("other/Cargo.toml")).unwrap();
    let err = run(&tmp.path().join("proj")).unwrap_err().to_string();
    assert!(err.contains("other") && err.contains("Cargo.toml"), "{err}");
}

#[test]
fn a_tracks_entry_for_a_package_that_does_not_exist_is_an_error() {
    let ws = workspace(&[("a", "0.1.0"), ("b", "0.1.0")], "");
    std::fs::write(
        ws.path().join(".coderipper.toml"),
        "[[tracks]]\npackage = \"ghost\"\nmanifest = \"Cargo.toml\"\nreason = \"typo\"\n",
    )
    .unwrap();
    let err = run(ws.path()).unwrap_err().to_string();
    assert!(
        err.contains("ghost") && err.contains("not a package"),
        "{err}"
    );
}

// ---- through the host: the ordinary allowlist ----

#[test]
fn an_allowlist_entry_silences_one_package_and_goes_stale_when_the_versions_agree() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    std::fs::write(
        ws.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"version-consistency\"\nfile = \"c/Cargo.toml\"\nsymbol = \"c\"\nreason = \"c is frozen for a release\"\n",
    )
    .unwrap();
    let ctx = || CheckContext {
        project_root: ws.path().to_path_buf(),
        portfolio_root: ws.path().to_path_buf(),
    };
    let quiet = coderipper::run_checks(&ctx(), Tier::Fast, Some("version-consistency"));
    assert!(quiet.errors.is_empty(), "{:?}", quiet.errors);
    assert!(quiet.findings.is_empty(), "{:?}", quiet.findings);

    // fix the version: the entry now suppresses nothing and says so
    std::fs::write(
        ws.path().join("c/Cargo.toml"),
        "[package]\nname = \"c\"\nversion = \"0.4.2\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let stale = coderipper::run_checks(&ctx(), Tier::Fast, Some("version-consistency"));
    assert_eq!(stale.findings.len(), 1, "{:?}", stale.findings);
    assert_eq!(stale.findings[0].check_id, "allowlist");
}

#[test]
fn a_directory_that_is_not_a_cargo_project_is_an_error_not_clean() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(run(tmp.path()).is_err());
}
````

  Run: `cargo test --test version_consistency`
  Expected: PASS, 12 tests.

- [ ] **Step 2: CLI test and docs.**

````diff
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -13,6 +13,7 @@ coderipper sweep                     # everything, including network-backed chec
 coderipper check reachability        # one check by id
 coderipper check unused-return-values  # is a function's return value ever consumed?
 coderipper check unused-parameters   # which function parameters are never used?
+coderipper check version-consistency # does every package of the project share one version?
 ```
 
 A server mode (`coderipper serve`) is planned, for a free hosted instance on
@@ -26,6 +27,25 @@ never used) and `unused-return-values` (a function whose
 return value every caller discards). See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
 `docs/superpowers/plans/` for what's actually being built and in what order.
 
+## Version consistency
+
+`coderipper check version-consistency` reports every package of a project whose version differs from the
+project's (CireSnave's rule: one version per project). The project's version is `[workspace.package] version`
+when the workspace defines one, otherwise the version most packages are at (a tie goes to the highest). A package
+that exists to work with another project's version is declared in `.coderipper.toml`, and is then compared with
+that project's current manifest instead; the reason is required:
+
+```toml
+[[tracks]]
+package = "baracuda-unpopped"
+manifest = "../unpopped/Cargo.toml"
+reason = "emitter for Unpopped; keeps Unpopped's version"
+```
+
+A package can also be silenced with an ordinary `[[allow]]` entry (check `version-consistency`, `file` = its
+manifest relative to the workspace root, `symbol` = the package name). Rust (cargo) manifests only; it reads the
+working tree, not HEAD.
+
 ## Workspaces
 
 A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
diff --git a/docs/superpowers/specs/2026-09-30-audit-host-design.md b/docs/superpowers/specs/2026-09-30-audit-host-design.md
--- a/docs/superpowers/specs/2026-09-30-audit-host-design.md
+++ b/docs/superpowers/specs/2026-09-30-audit-host-design.md
@@ -191,6 +191,13 @@ almost entirely `Project`-scope but has one narrow cross-project read baked into
 confirms scope is a property of the check's *typical* need, not an absolute boundary the host has to
 enforce strictly.
 
+**`version-consistency` implemented 2026-10-02** (`docs/superpowers/plans/2026-10-02-version-consistency.md`), Rust
+manifests only. The project's version is `[workspace.package] version` when defined, else the majority (a tie goes
+to the highest). The narrow exception of the paragraph above is a `[[tracks]]` table in `.coderipper.toml`
+(`package`, `manifest`, `reason`, all required): not a suppression, a declared relationship, so it is read by the
+check itself and the tracked package is compared with the other project's CURRENT manifest; an unreadable reference
+is an error. Ordinary suppression of a package uses the §4 allowlist. Not built: package.json / pyproject manifests.
+
 **`dependency-staleness`** — *`Project` scope, `NetworkRequired`, `sweep` tier.* CireSnave's standing rule:
 dependencies stay on their most recent versions. Mechanism: read each project's lockfile, query the
 relevant registry (crates.io/npm/PyPI) for each dependency's latest version, diff. Findings: severity scales
diff --git a/tests/cli.rs b/tests/cli.rs
--- a/tests/cli.rs
+++ b/tests/cli.rs
@@ -395,3 +395,34 @@ fn a_virtual_workspace_root_asks_for_a_member() {
         .stderr(predicate::str::contains("not a package directory"))
         .stderr(predicate::str::contains("one, two").or(predicate::str::contains("two, one")));
 }
+
+#[test]
+fn the_version_consistency_check_runs_by_id_and_reports_the_outlier() {
+    let tmp = tempfile::tempdir().unwrap();
+    std::fs::write(
+        tmp.path().join("Cargo.toml"),
+        "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n",
+    )
+    .unwrap();
+    for (name, version) in [("a", "1.0.0"), ("b", "1.0.0"), ("c", "0.9.0")] {
+        let dir = tmp.path().join(name);
+        std::fs::create_dir_all(dir.join("src")).unwrap();
+        std::fs::write(
+            dir.join("Cargo.toml"),
+            format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
+        )
+        .unwrap();
+        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
+    }
+    Command::cargo_bin("coderipper")
+        .unwrap()
+        .args(["check", "version-consistency", "--project"])
+        .arg(tmp.path())
+        .assert()
+        .success()
+        .stdout(predicate::str::contains("[High/High]"))
+        .stdout(predicate::str::contains(
+            "`c` is at version 0.9.0 but this project is at 1.0.0",
+        ))
+        .stdout(predicate::str::contains("(version-consistency)"));
+}
````

- [ ] **Step 3: Run everything.** Run: `cargo fmt && cargo test --no-fail-fast && cargo clippy --all-targets -- -D warnings`
  Expected: PASS — 136 lib, 8 allowlist, 11 cli, 20 reachability, 11 unused-parameters, 13 unused-return-values, 12 version-consistency, 13 workspace; clippy silent.

- [ ] **Step 4: Acceptance on a real workspace (read-only).** `git clone --no-hardlinks <path to fuel> /tmp/fuelclone`, build this branch, then
  `<built-binary> check version-consistency --project /tmp/fuelclone`.
  Expected (measured at plan time; `fuel` at `d1bfe127`): `coderipper: no issues found` in ~3 s. Then, in the **scratch clone only**, change `fuel-ir/Cargo.toml`'s `version.workspace = true` to `version = "0.11.0"` and re-run.
  Expected: exactly one line, ``[High/High] fuelclone — `fuel-ir` is at version 0.11.0 but this project is at 0.12.0 (version-consistency)``. Revert the scratch edit.

- [ ] **Step 5: Commit, push, report.** `git add -A tests README.md docs && git commit -m "test+docs: version-consistency end-to-end tests, CLI, README, spec note"` (+ trailers), push, and report to the PM (who opens and merges the PR and allocates the version).
  Read the PR's checks and **unresolved review threads** (GraphQL `reviewThreads{isResolved}`) before calling it READY.
