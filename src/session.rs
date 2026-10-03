//! The session checkout: one git checkout for a whole `--workspace` run.
//!
//! Without a session every check run makes its own throwaway checkout, rewrites the analysed package, builds, and
//! deletes the checkout. A new checkout makes every file newer than every cached artifact, so the package's
//! in-repo siblings (path dependencies) are recompiled in every run. A session instead keeps ONE checkout for the
//! run: analysing a member rewrites that member's `src/` **in place**, and dropping the guard **restores** it with
//! `git checkout HEAD -- <member>`, which gives the files it rewrites a new mtime ("now", after the build ended).
//! Siblings nobody touches keep their mtimes and stay fresh for every later member.
//!
//! Rules this module keeps (each has a test; see `docs/superpowers/plans/2026-10-03-workspace-flag-and-build-cache.md`):
//! - **Never restore an old mtime.** An artifact compiled from the rewritten source is fresh for any file whose mtime
//!   is older than it; restoring the original bytes with the original mtime would make a dependent of that member link
//!   against the rewritten artifact. "Now" makes the next build of that member recompile it from the original bytes.
//! - **No `git clean`.** Nothing a check does creates files, and `git clean -fd` was measured to delete a non-ignored
//!   `target/` and other members' untracked files. After every restore `git status --porcelain --untracked-files=no`
//!   over the whole checkout must be empty (the workspace root's `Cargo.lock` excepted: cargo rewrites a committed
//!   lock that is out of step with the manifests, and the restore is member-scoped).
//! - **A failed restore is retried briefly, then poisons the session**: nothing more is analysed in a dirty checkout.
//! - **The session holds the build-cache lock for its whole run** (a concurrent run could otherwise write a rewritten
//!   artifact into the shared slot between two of the session's builds) and **owns its cargo target directory**:
//!   `<cache dir>/target` (the slot every `--project` run uses) when it got the lock, else a temporary directory
//!   outside the checkout.

use crate::build_cache::{self, CacheChoice, CacheDir};
use crate::github::git_command;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// How long to wait between attempts to restore a member (a transient lock from an antivirus scanner or an open
/// editor is real on Windows); the first attempt is immediate.
const RESTORE_WAITS_MS: [u64; 4] = [0, 100, 300, 700];

pub(crate) struct SessionInner {
    source_repo: PathBuf,
    identity: PathBuf,
    checkout: PathBuf,
    /// Parent directory of the checkout; removed after the checkout is.
    _scratch: tempfile::TempDir,
    /// The lock on the persistent cache's directory, held for the whole run.
    _cache: RefCell<Option<CacheDir>>,
    _temp_target: Option<tempfile::TempDir>,
    target: PathBuf,
    cache_dir: Option<PathBuf>,
    /// The workspace root's `Cargo.lock`, relative to the repository's top level, forward slashes.
    lock_rel: String,
    poison: RefCell<Option<String>>,
}

/// One checkout for a whole run (see the module documentation).
pub(crate) struct Session {
    inner: Rc<SessionInner>,
}

/// Where a build inside a session puts its artifacts.
pub(crate) struct SessionTarget {
    pub target: PathBuf,
    pub cache_dir: Option<PathBuf>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<SessionInner>>> = const { RefCell::new(None) };
}

// Tests only: make this many restore attempts fail before one is allowed to succeed.
#[cfg(test)]
thread_local! {
    pub(crate) static RESTORE_FAILURES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Session {
    /// Takes the cache lock (or falls back to a temporary target) and makes the checkout at HEAD.
    pub(crate) fn open(source_repo: &Path) -> anyhow::Result<Session> {
        let identity = build_cache::git_common_dir(source_repo)?;
        let (cache, target, cache_dir, temp_target) =
            match build_cache::acquire_for_session(source_repo) {
                CacheChoice::Shared(dir) => {
                    let target = dir.path.join("target");
                    let cache_dir = dir.path.clone();
                    (Some(dir), target, Some(cache_dir), None)
                }
                CacheChoice::Throwaway { why } => {
                    if let Some(why) = why {
                        build_cache::note(why);
                    }
                    let temp = tempfile::tempdir()?;
                    (None, temp.path().to_path_buf(), None, Some(temp))
                }
            };

        let scratch = tempfile::tempdir()?;
        let checkout = scratch.path().join("wt");
        let status = git_command(
            source_repo,
            &[
                "worktree",
                "add",
                "--detach",
                "-q",
                &checkout.to_string_lossy(),
                "HEAD",
            ],
        )
        .status()?;
        anyhow::ensure!(status.success(), "git worktree add failed");

        let lock_rel = workspace_lock_rel(source_repo)?;
        Ok(Session {
            inner: Rc::new(SessionInner {
                source_repo: source_repo.to_path_buf(),
                identity,
                checkout,
                _scratch: scratch,
                _cache: RefCell::new(cache),
                _temp_target: temp_target,
                target,
                cache_dir,
                lock_rel,
                poison: RefCell::new(None),
            }),
        })
    }

    /// True once a restore failed for good: the host loop must stop.
    #[cfg(test)]
    pub(crate) fn is_poisoned(&self) -> bool {
        self.inner.poisoned().is_some()
    }

    /// Why the session is poisoned, if it is.
    pub(crate) fn poison_message(&self) -> Option<String> {
        self.inner.poisoned()
    }

    #[cfg(test)]
    pub(crate) fn checkout(&self) -> &Path {
        &self.inner.checkout
    }

    #[cfg(test)]
    pub(crate) fn target(&self) -> &Path {
        &self.inner.target
    }

    #[cfg(test)]
    pub(crate) fn cache_dir(&self) -> Option<&Path> {
        self.inner.cache_dir.as_deref()
    }

    /// Tests only: let go of the cache lock (the sabotage of the overlapping-run test).
    #[cfg(test)]
    #[allow(dead_code)] // only a manual sabotage run of the overlapping-run test calls it
    pub(crate) fn release_lock_for_test(&self) {
        self.inner._cache.borrow_mut().take();
    }
}

/// Runs `f` with `session` active on this thread: `RewrittenWorktree::create_with` for the session's repository
/// then rewrites in place, and `build_with` builds in the session's target directory. Restores the previous state
/// on exit, also on panic. A thread spawned inside `f` is NOT in the session.
pub(crate) fn with_session<T>(session: &Session, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Rc<SessionInner>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|a| *a.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(ACTIVE.with(|a| a.borrow_mut().replace(session.inner.clone())));
    f()
}

/// The session active on this thread, if it belongs to the repository `project_root` is in (compared by the
/// canonical git common directory, never by path text).
pub(crate) fn active_for(project_root: &Path) -> Option<Rc<SessionInner>> {
    let active = ACTIVE.with(|a| a.borrow().clone())?;
    let identity = build_cache::git_common_dir(project_root).ok()?;
    (identity == active.identity).then_some(active)
}

/// The session target for a build of `cache_source`'s repository, when a session for it is active on this thread.
/// A build with `cache_source = None`, or of another repository, is an ordinary build.
pub(crate) fn session_for(cache_source: Option<&Path>) -> Option<SessionTarget> {
    let active = active_for(cache_source?)?;
    Some(SessionTarget {
        target: active.target.clone(),
        cache_dir: active.cache_dir.clone(),
    })
}

impl SessionInner {
    pub(crate) fn checkout_root(&self) -> &Path {
        &self.checkout
    }

    pub(crate) fn poisoned(&self) -> Option<String> {
        self.poison.borrow().clone()
    }

    fn poison(&self, why: String) {
        let mut slot = self.poison.borrow_mut();
        if slot.is_none() {
            *slot = Some(format!(
                "the session checkout is dirty ({why}); not analysing any more members"
            ));
        }
    }

    /// Puts the member at `prefix` (its directory relative to the repository's top level, with a trailing `/`, or
    /// empty for a package at the root) back to HEAD, then checks that nothing tracked is left modified.
    pub(crate) fn restore_member(&self, prefix: &str) {
        if self.poisoned().is_some() {
            return;
        }
        let member = prefix.trim_end_matches('/');
        let pathspec = if member.is_empty() {
            ":(literal).".to_string()
        } else {
            format!(":(literal){member}")
        };
        let mut failure = None;
        for wait in RESTORE_WAITS_MS {
            if wait > 0 {
                std::thread::sleep(Duration::from_millis(wait));
            }
            failure = self.checkout_once(&pathspec).err();
            if failure.is_none() {
                break;
            }
        }
        if let Some(why) = failure {
            self.poison(format!("cannot restore {pathspec}: {why}"));
            return;
        }
        match self.dirty_paths() {
            Ok(dirty) if dirty.is_empty() => {}
            Ok(dirty) => self.poison(format!(
                "tracked files are still modified after the restore: {}",
                dirty.join(", ")
            )),
            Err(why) => self.poison(format!("cannot read the checkout's status: {why}")),
        }
    }

    fn checkout_once(&self, pathspec: &str) -> Result<(), String> {
        #[cfg(test)]
        if RESTORE_FAILURES.with(|c| {
            let n = c.get();
            c.set(n.saturating_sub(1));
            n > 0
        }) {
            return Err("injected failure".to_string());
        }
        let output = git_command(&self.checkout, &["checkout", "-q", "HEAD", "--", pathspec])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// Tracked files that differ from HEAD, other than the workspace root's `Cargo.lock`.
    fn dirty_paths(&self) -> Result<Vec<String>, String> {
        let output = git_command(
            &self.checkout,
            &["status", "--porcelain", "--untracked-files=no"],
        )
        .output()
        .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.get(3..))
            .map(|path| path.trim_matches('"').to_string())
            .filter(|path| *path != self.lock_rel)
            .collect())
    }
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        // `git worktree remove` unregisters the checkout from the source repository as well as deleting it.
        let removed = git_command(
            &self.source_repo,
            &[
                "worktree",
                "remove",
                "--force",
                &self.checkout.to_string_lossy(),
            ],
        )
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
        if !removed {
            let _ = std::fs::remove_dir_all(&self.checkout);
        }
        // `cache` (the lock) is released when the fields drop, after the checkout is gone
    }
}

/// The workspace root's `Cargo.lock` as `git status --porcelain` prints it: relative to the repository's top
/// level (not to the directory git is run in), forward slashes.
fn workspace_lock_rel(source_repo: &Path) -> anyhow::Result<String> {
    let top = git_command(source_repo, &["rev-parse", "--show-toplevel"]).output()?;
    anyhow::ensure!(
        top.status.success(),
        "cannot find the top level of {}",
        source_repo.display()
    );
    let top = PathBuf::from(String::from_utf8_lossy(&top.stdout).trim()).canonicalize()?;
    let root = crate::package::metadata(source_repo)?
        .workspace_root
        .canonicalize()?;
    let relative = root.strip_prefix(&top).map_err(|_| {
        anyhow::anyhow!(
            "the workspace root {} is outside the repository {}",
            root.display(),
            top.display()
        )
    })?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    Ok(if relative.is_empty() {
        "Cargo.lock".to_string()
    } else {
        format!("{relative}/Cargo.lock")
    })
}

#[cfg(test)]
mod tests;
