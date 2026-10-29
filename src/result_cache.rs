// SPDX-License-Identifier: MIT OR Apache-2.0
//! A cache of the results of the checks that build the project, so an unchanged repository is not analysed twice.
//!
//! The three checks that rewrite the project, build it and read the compiler's diagnostics (`reachability`,
//! `unused-return-values`, `unused-parameters`) cost minutes per workspace member (measured on `fuel@8cdb33dd`: 1742 s cold and
//! 915 s warm for one rule over 45 members). They analyse the **committed** tree, so what they report is a function of that tree,
//! of the toolchain that built it and of the options they were given. This module stores that function's value, and a later run
//! over the same inputs reads it back instead of building.
//!
//! **The key** (SHA-256 over): this CodeRipper's version, the check, the project's path inside its repository, the git tree hash of
//! the repository's `HEAD` (the whole tree, on purpose: a build can read any file in it, `include_str!("../../README.md")` for one,
//! so a narrower key could serve a stale result), the toolchain (`rustc -vV`), the cargo features, and the environment variables that
//! change what a build does (`RUSTFLAGS` and the like). A change to any one of them is a miss.
//!
//! **Design rules (patent reading of US10459822B1, `docs/PATENT-SWEEP.md` hit 15):** the key is always the whole committed tree plus the
//! toolchain and options; **never a per-file, per-snippet or per-function hash**; **never a prior result reused for a partly changed tree**;
//! the store is local, never shared or remote. A change to any of these needs a new sweep.
//!
//! **What it never does:**
//! - Correctness never depends on it. No cache directory, a repository that is not git, an unreadable or damaged entry, a key that
//!   cannot be computed, a write that fails: the check runs, exactly as it did before this module existed.
//! - A check that ended in an error (a build that failed, a sentinel that did not come back: a gap) is never stored. Only a result
//!   a check returned as a verdict is.
//! - It is not used by a check that reads the working tree or the network (`version-consistency`, `dependency-freshness`,
//!   `ci-protection-presence`): [`CACHEABLE`] is the list.
//!
//! **Where:** `<build cache root>/results/<key>.json` (see [`crate::build_cache`]); the build cache is off by default for a library
//! user and so is this. `CODERIPPER_RESULT_CACHE=off` turns just this off. At most [`MAX_ENTRIES`] entries are kept, the oldest
//! dropped first.
//!
//! **What a hit says:** a run that reused or computed a result through the store ends with one stderr line (`coderipper: result
//! cache: N reused, M computed`), like the build cache's line, so a reader is told which results were not computed in this run. It is
//! not a note on the result: a note changes what a clean run prints, and where a result came from says nothing about the project.

use crate::finding::Finding;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The checks whose result is a function of the committed tree (see the module documentation).
pub(crate) const CACHEABLE: &[&str] = &[
    "reachability",
    "unused-return-values",
    "unused-parameters",
    "OPS-011",
];

/// How many entries are kept before the oldest are dropped.
const MAX_ENTRIES: usize = 4000;

/// How many results this process reused and computed through the store, for the one stderr line a run ends with. (Not a note on the
/// result: a note changes what a clean run prints, and where a result came from is not a fact about the project.)
static REUSED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static COMPUTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The line a run prints when it reused or stored a result: `coderipper: result cache - N reused, M computed`; `None` when the
/// store was not used (off, a library user, no cacheable check ran). Takes the counts, so a second call says nothing.
#[cfg(feature = "cli")]
pub(crate) fn take_stats_line() -> Option<String> {
    use std::sync::atomic::Ordering::Relaxed;
    let reused = REUSED.swap(0, Relaxed);
    let computed = COMPUTED.swap(0, Relaxed);
    (reused + computed > 0).then(|| {
        format!(
            "coderipper: result cache - {reused} reused (same committed tree), {computed} computed"
        )
    })
}

/// The environment variables that change what a build does, and so what a check reports.
const BUILD_ENV: &[&str] = &[
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTUP_TOOLCHAIN",
    "CARGO_BUILD_TARGET",
];

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    /// The key this entry was stored under (checked on read: a renamed file is not served).
    key: String,
    findings: Vec<Finding>,
    notes: Vec<String>,
}

/// What a check returns when it ran to a verdict.
pub(crate) type Verdict = (Vec<Finding>, Vec<String>);

/// Runs `run` for the check `check_id` over `ctx`, or returns the stored result of an earlier run with the same inputs.
pub(crate) fn through(
    check_id: &str,
    ctx: &crate::check::CheckContext,
    run: impl FnOnce() -> anyhow::Result<Verdict>,
) -> anyhow::Result<Verdict> {
    let Some(store) = Store::for_check(check_id, ctx) else {
        return run();
    };
    if let Some((mut findings, notes)) = store.read() {
        let name = project_name(&ctx.project_root);
        for f in &mut findings {
            f.project = name.clone();
        }
        REUSED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok((findings, notes));
    }
    let verdict = run()?;
    COMPUTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    store.write(&verdict);
    Ok(verdict)
}

/// The directory name a check calls its project.
fn project_name(root: &Path) -> String {
    std::path::absolute(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

struct Store {
    key: String,
    dir: PathBuf,
}

impl Store {
    /// The store for this check and project, or `None` when nothing should be cached or looked up.
    fn for_check(check_id: &str, ctx: &crate::check::CheckContext) -> Option<Self> {
        if !CACHEABLE.contains(&check_id)
            || std::env::var("CODERIPPER_RESULT_CACHE").is_ok_and(|v| v.eq_ignore_ascii_case("off"))
        {
            return None;
        }
        let config = crate::build_cache::current_config()?;
        let dir = config.root.join("results");
        let key = key_for(check_id, ctx)?;
        Some(Self { key, dir })
    }

    fn path(&self) -> PathBuf {
        self.dir.join(format!("{}.json", self.key))
    }

    fn read(&self) -> Option<Verdict> {
        let text = std::fs::read_to_string(self.path()).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;
        (entry.key == self.key).then_some((entry.findings, entry.notes))
    }

    /// Stores `verdict`. Any failure is silent: the cache is an optimisation, never a dependency.
    fn write(&self, verdict: &Verdict) {
        let entry = Entry {
            key: self.key.clone(),
            findings: verdict.0.clone(),
            notes: verdict.1.clone(),
        };
        let Ok(text) = serde_json::to_string(&entry) else {
            return;
        };
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        // write beside, then rename: a reader never sees half an entry, and two runs storing the same key store the same bytes
        let temp = self
            .dir
            .join(format!("{}.{}.tmp", self.key, std::process::id()));
        if std::fs::write(&temp, text).is_err() || std::fs::rename(&temp, self.path()).is_err() {
            let _ = std::fs::remove_file(&temp);
            return;
        }
        self.trim();
    }

    /// Drops the oldest entries once there are more than [`MAX_ENTRIES`].
    fn trim(&self) {
        let Ok(read) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut entries: Vec<(std::time::SystemTime, PathBuf)> = read
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if entries.len() <= MAX_ENTRIES {
            return;
        }
        entries.sort();
        let excess = entries.len() - MAX_ENTRIES + MAX_ENTRIES / 10;
        for (_, path) in entries.into_iter().take(excess) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `git` output for `args` run in `dir`, trimmed, or `None` if it failed.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The key for `check_id` over `ctx`, or `None` when an input cannot be read (then nothing is cached or looked up).
fn key_for(check_id: &str, ctx: &crate::check::CheckContext) -> Option<String> {
    let root = &ctx.project_root;
    let tree = git(root, &["rev-parse", "--verify", "-q", "HEAD^{tree}"])?;
    if tree.len() < 40 {
        return None;
    }
    let prefix = git(root, &["rev-parse", "--show-prefix"])?;
    let rustc = Command::new("rustc").arg("-vV").output().ok()?;
    if !rustc.status.success() {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut part = |bytes: &[u8]| {
        hasher.update(bytes);
        hasher.update([0]);
    };
    part(b"result-cache-v1");
    part(env!("CARGO_PKG_VERSION").as_bytes());
    part(check_id.as_bytes());
    part(tree.as_bytes());
    part(prefix.replace('\\', "/").as_bytes());
    part(&rustc.stdout);
    part(format!("{:?}", ctx.cargo_features).as_bytes());
    for name in BUILD_ENV {
        part(name.as_bytes());
        part(std::env::var(name).unwrap_or_default().as_bytes());
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::CheckContext;
    use crate::finding::{Confidence, Severity};
    use std::cell::Cell;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_CONFIG_GLOBAL", "")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}: {out:?}");
        };
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "x"]);
        dir
    }

    fn finding(project: &str) -> Finding {
        Finding::new(
            "reachability",
            Severity::Medium,
            Confidence::High,
            project,
            "s",
            "d",
        )
    }

    fn cached<T>(cache: &Path, f: impl FnOnce() -> T) -> T {
        crate::build_cache::with_config(crate::build_cache::CacheConfig::new(cache), f)
    }

    #[test]
    fn a_second_run_over_the_same_tree_is_served_from_the_store_and_says_so() {
        let repo = repo();
        let cache = tempfile::tempdir().unwrap();
        let ctx = CheckContext::new(repo.path());
        let runs = Cell::new(0);
        let go = || {
            through("reachability", &ctx, || {
                runs.set(runs.get() + 1);
                Ok((vec![finding("x")], vec!["a note".to_string()]))
            })
            .unwrap()
        };
        let first = cached(cache.path(), go);
        assert_eq!(runs.get(), 1);
        assert_eq!(first.1, vec!["a note".to_string()], "{first:?}");
        let second = cached(cache.path(), go);
        assert_eq!(runs.get(), 1, "the second run did not run the check");
        assert_eq!(second.0.len(), 1);
        assert_eq!(
            second.1,
            vec!["a note".to_string()],
            "a reused result carries exactly the notes it was stored with"
        );
        // the project is named as this run names it (a checkout of the same tree may live in a directory with another name)
        assert_eq!(second.0[0].project, project_name(repo.path()));
    }

    #[test]
    fn a_changed_tree_or_a_changed_check_is_a_miss() {
        let repo = repo();
        let cache = tempfile::tempdir().unwrap();
        let ctx = CheckContext::new(repo.path());
        let runs = Cell::new(0);
        let go = |id: &str, ctx: &CheckContext| {
            cached(cache.path(), || {
                through(id, ctx, || {
                    runs.set(runs.get() + 1);
                    Ok((Vec::new(), Vec::new()))
                })
                .unwrap()
            })
        };
        go("reachability", &ctx);
        go("reachability", &ctx);
        assert_eq!(runs.get(), 1);
        go("unused-parameters", &ctx);
        assert_eq!(runs.get(), 2, "another check is another key");
        // a commit that changes the tree
        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
        for args in [&["add", "-A"][..], &["commit", "-q", "-m", "y"]] {
            let ok = Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .env("GIT_CONFIG_GLOBAL", "")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .status()
                .unwrap()
                .success();
            assert!(ok);
        }
        go("reachability", &ctx);
        assert_eq!(runs.get(), 3, "a new tree is a new key");
    }

    #[test]
    fn an_error_is_never_stored_and_a_damaged_entry_is_a_miss() {
        let repo = repo();
        let cache = tempfile::tempdir().unwrap();
        let ctx = CheckContext::new(repo.path());
        let failed = cached(cache.path(), || {
            through("reachability", &ctx, || {
                Err(anyhow::anyhow!("the build failed"))
            })
        });
        assert!(failed.is_err());
        let runs = Cell::new(0);
        let ok = || {
            cached(cache.path(), || {
                through("reachability", &ctx, || {
                    runs.set(runs.get() + 1);
                    Ok((Vec::new(), Vec::new()))
                })
                .unwrap()
            })
        };
        ok();
        assert_eq!(runs.get(), 1, "the failed run left nothing behind");
        // damage the stored entry: the next run computes again and replaces it
        let results = cache.path().join("results");
        let entry = std::fs::read_dir(&results)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::write(&entry, "not json").unwrap();
        ok();
        assert_eq!(runs.get(), 2);
        ok();
        assert_eq!(runs.get(), 2, "and the replacement is served");
    }

    #[test]
    fn nothing_is_cached_without_a_cache_for_a_cheap_check_or_outside_git() {
        let repo = repo();
        let ctx = CheckContext::new(repo.path());
        let runs = Cell::new(0);
        let count = || {
            runs.set(runs.get() + 1);
            Ok((Vec::new(), Vec::new()))
        };
        // no cache configured on this thread: the check runs every time
        through("reachability", &ctx, count).unwrap();
        through("reachability", &ctx, count).unwrap();
        assert_eq!(runs.get(), 2);
        // a check that reads the working tree or the network is not in the list
        let cache = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            cached(cache.path(), || {
                through("version-consistency", &ctx, count).unwrap()
            });
        }
        assert_eq!(runs.get(), 4);
        // not a git repository: no key, so the check runs every time
        let plain = tempfile::tempdir().unwrap();
        let plain_ctx = CheckContext::new(plain.path());
        for _ in 0..2 {
            cached(cache.path(), || {
                through("reachability", &plain_ctx, count).unwrap()
            });
        }
        assert_eq!(runs.get(), 6);
    }
}
