//! A persistent `CARGO_TARGET_DIR` for the builds the checks run, so a dependency is compiled once, not once per run.
//!
//! Every check builds the package it analyses in a throwaway git worktree. Left alone, each build starts from an
//! empty target directory and recompiles every dependency (about two minutes on a large crate). Measured: with one
//! shared target directory a build from a *different* worktree path finds every registry, git and out-of-repo
//! dependency unit `fresh`. Cargo's own fingerprinting does the reuse; this module only gives it a stable place to
//! live. (An in-repo path crate is rebuilt in every new worktree, because its checkout is newer than the artifact.
//! That limit is stated in the source repository's workspace-flag plan.)
//!
//! **Where it lives:** `CODERIPPER_CACHE_DIR`, else `%LOCALAPPDATA%\coderipper\build`, else
//! `$XDG_CACHE_HOME/coderipper/build`, else `~/.cache/coderipper/build`. Never a project's `target/` (that carries
//! cargo's build-directory lock, which another tool may be waiting on) and never inside the repository analysed.
//! One directory per *toolchain* and per *source repository*: `<root>/<toolchain>/<repository>/`.
//!
//! **Correctness never depends on the cache.** If it cannot be used (unwritable, not a git repository, busy) the
//! build runs in the worktree's own `target/` exactly as it did before this module existed, and says so.
//!
//! **No unbounded wait.** CodeRipper takes its own advisory lock on the repository's directory *before* cargo
//! takes its build-directory lock, and gives up after `CODERIPPER_CACHE_WAIT_SECS` (5), naming the last holder.
//! The holder's pid lives in a separate, never-locked sidecar file: on Windows a locked file cannot even be read
//! by a second handle, so the pid could not be stored in the file that is locked.
//!
//! **Size:** `CODERIPPER_CACHE_MAX_GB` (20). When the first build of a process starts, the least recently used
//! repository directories are deleted until the cache fits; a locked directory and the one being acquired are
//! never deleted, and nothing is deleted from a root that lacks the [`MARKER`] file (so pointing the cache at a
//! directory that is not one cannot delete anything there).
//!
//! The default for library users and tests is **no cache**: nothing happens until [`set_cache_config`] is called
//! (the CLI does it from the environment; `CODERIPPER_CACHE=off` disables it).

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A cache root contains this file. Pruning refuses a directory without it.
#[doc(hidden)]
pub const MARKER: &str = ".coderipper-cache";
const LOCK_FILE: &str = ".coderipper.lock";
const HOLDER_FILE: &str = ".coderipper.holder";
const POLL: Duration = Duration::from_millis(50);

/// How the build cache behaves: where it lives, how long a run waits for another run's lock on the same repository,
/// and the size cap.
///
/// Build one with [`CacheConfig::new`] and its setters: the struct is `#[non_exhaustive]`, so a struct literal is
/// rejected outside this crate and a field can be added later without breaking anyone.
///
/// ```compile_fail,E0639
/// use coderipper::build_cache::CacheConfig;
/// let _ = CacheConfig {
///     root: "/cache".into(),
///     wait: std::time::Duration::from_secs(5),
///     max_bytes: 1 << 30,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheConfig {
    /// The directory the cache lives in: `<root>/<toolchain>/<repository>/`.
    pub root: PathBuf,
    /// How long to wait for another CodeRipper run's lock on the same repository before building uncached.
    pub wait: Duration,
    /// The size cap: least-recently-used repository directories are pruned until the cache fits.
    pub max_bytes: u64,
}

/// The default wait for another run's lock, in seconds.
const DEFAULT_WAIT_SECS: f64 = 5.0;
/// The default size cap, in GB (1 GB = 2^30 bytes).
const DEFAULT_MAX_GB: f64 = 20.0;

impl CacheConfig {
    /// A cache rooted at `root` with the default wait (5 s for another run's lock on the same repository) and size
    /// cap (20 GB); adjust with [`CacheConfig::wait`] and [`CacheConfig::max_bytes`].
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            wait: Duration::from_secs_f64(DEFAULT_WAIT_SECS),
            max_bytes: (DEFAULT_MAX_GB * (1u64 << 30) as f64) as u64,
        }
    }

    /// How long a run waits for another run's lock on the same repository before building without the cache.
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    /// The size cap: least-recently-used repository directories are pruned until the cache fits.
    pub fn max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }
}

static CONFIG: OnceLock<CacheConfig> = OnceLock::new();

thread_local! {
    /// A per-thread override of the cache configuration, for in-crate tests (see [`with_config`]).
    static OVERRIDE: std::cell::RefCell<Option<CacheConfig>> = const { std::cell::RefCell::new(None) };
}

/// The configuration in force on this thread: the thread-local override if one is set, else the process-wide
/// one set by [`set_cache_config`], else `None` (no cache).
pub(crate) fn current_config() -> Option<CacheConfig> {
    OVERRIDE
        .with(|o| o.borrow().clone())
        .or_else(|| CONFIG.get().cloned())
}

/// Runs `f` with `config` as the cache configuration **on this thread only**; the previous value is restored on
/// exit, also when `f` panics. For in-crate tests: the process-wide configuration can be set once per process and
/// in-crate tests run in parallel threads, so a test cannot use it. A thread spawned inside `f` does NOT inherit
/// it and must call `with_config` itself. Production code never calls this.
#[cfg(test)]
pub(crate) fn with_config<T>(config: CacheConfig, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<CacheConfig>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| *o.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(OVERRIDE.with(|o| o.borrow_mut().replace(config)));
    f()
}

/// Turns the cache on for this process. Returns false if a configuration was already set.
pub fn set_cache_config(config: CacheConfig) -> bool {
    CONFIG.set(config).is_ok()
}

/// The cache configuration from the environment, or `None` when the cache is off or no root can be derived.
/// Takes the lookup as a parameter so every branch is testable without touching the process environment.
#[doc(hidden)]
pub fn config_from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<CacheConfig> {
    if get("CODERIPPER_CACHE").is_some_and(|v| v.trim().eq_ignore_ascii_case("off")) {
        return None;
    }
    let present = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let under = |base: String| PathBuf::from(base).join("coderipper").join("build");
    let root = if let Some(dir) = present("CODERIPPER_CACHE_DIR") {
        PathBuf::from(dir)
    } else if let Some(base) = present("LOCALAPPDATA") {
        under(base)
    } else if let Some(base) = present("XDG_CACHE_HOME") {
        under(base)
    } else {
        PathBuf::from(present("HOME")?)
            .join(".cache")
            .join("coderipper")
            .join("build")
    };
    let number = |k: &str| {
        present(k)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
    };
    Some(CacheConfig {
        root,
        wait: Duration::from_secs_f64(
            number("CODERIPPER_CACHE_WAIT_SECS").unwrap_or(DEFAULT_WAIT_SECS),
        ),
        max_bytes: (number("CODERIPPER_CACHE_MAX_GB").unwrap_or(DEFAULT_MAX_GB)
            * (1u64 << 30) as f64) as u64,
    })
}

/// FNV-1a, 64 bit. Not `DefaultHasher`: its output is unspecified across Rust versions, and the key names a
/// directory that must survive upgrades.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn hex12(hash: u64) -> String {
    format!("{:012x}", hash & 0xFFFF_FFFF_FFFF)
}

/// `(toolchain key, repository key)`: two 12-digit hashes, kept short because Windows paths are.
pub(crate) fn key(toolchain: &str, repository: &Path) -> (String, String) {
    (
        hex12(fnv1a(toolchain.as_bytes())),
        hex12(fnv1a(repository.to_string_lossy().as_bytes())),
    )
}

pub(crate) fn dir_for(root: &Path, toolchain: &str, repository: &Path) -> PathBuf {
    let (toolchain_key, repository_key) = key(toolchain, repository);
    root.join(toolchain_key).join(repository_key)
}

/// Holds the advisory lock on one repository's cache directory; released on drop.
#[derive(Debug)]
pub(crate) struct CacheLock(File);

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Debug)]
pub(crate) struct CacheDir {
    pub path: PathBuf,
    _lock: CacheLock,
}

#[derive(Debug)]
pub(crate) enum CacheChoice {
    /// Build with `CARGO_TARGET_DIR` inside this directory, for as long as the value lives.
    Shared(CacheDir),
    /// Set nothing: cargo builds in the worktree's own `target/`, as it always did. `why` is `Some` when a
    /// cache was configured but could not be used, and is shown to the user once.
    Throwaway { why: Option<String> },
}

pub(crate) fn git_common_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    let output = crate::github::git_command(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{} is not inside a git repository",
        dir.display()
    );
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    Ok(path.canonicalize().unwrap_or(path))
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What the holder wrote to its sidecar, as words. The lock file itself cannot be read while it is locked.
fn holder_text(dir: &Path) -> String {
    let text = fs::read_to_string(dir.join(HOLDER_FILE)).unwrap_or_default();
    let mut parts = text.split_whitespace();
    match (
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(|s| s.parse::<u64>().ok()),
    ) {
        (Some(pid), Some(since)) => format!(
            "last holder pid {pid}, started {}s ago",
            unix_secs().saturating_sub(since)
        ),
        _ => "holder unknown".to_string(),
    }
}

/// Picks the cache directory for `source_repo` under `config.root`, taking the repository's lock.
/// `prune` enforces the size cap first (the caller does it once per process, because it walks the cache).
pub(crate) fn acquire_in(
    config: &CacheConfig,
    toolchain: &str,
    source_repo: &Path,
    prune: bool,
) -> CacheChoice {
    let unusable = |why: String| CacheChoice::Throwaway {
        why: Some(format!(
            "build cache unusable ({why}): building without the cache"
        )),
    };
    let identity = match git_common_dir(source_repo) {
        Ok(identity) => identity,
        Err(e) => return unusable(e.to_string()),
    };
    let dir = dir_for(&config.root, toolchain, &identity);
    if let Err(e) = fs::create_dir_all(&dir) {
        return unusable(format!("cannot create {}: {e}", dir.display()));
    }
    let marker = config.root.join(MARKER);
    if !marker.exists() {
        let text = "CodeRipper build cache. `coderipper cache prune` deletes only inside a directory holding this file.\n";
        if let Err(e) = fs::write(&marker, text) {
            return unusable(format!("cannot write {}: {e}", marker.display()));
        }
    }
    if prune {
        let _ = prune_to_cap(&config.root, config.max_bytes, Some(&dir));
    }
    let lock_path = dir.join(LOCK_FILE);
    let file = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(e) => return unusable(format!("cannot open {}: {e}", lock_path.display())),
    };
    let deadline = Instant::now() + config.wait;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return CacheChoice::Throwaway {
                        why: Some(format!(
                            "cache busy ({}): building without the cache",
                            holder_text(&dir)
                        )),
                    };
                }
                std::thread::sleep(POLL);
            }
            Err(fs::TryLockError::Error(e)) => {
                return unusable(format!("cannot lock {}: {e}", lock_path.display()))
            }
        }
    }
    let _ = fs::write(
        dir.join(HOLDER_FILE),
        format!("{} {}\n", std::process::id(), unix_secs()),
    );
    // "last used", for the least-recently-used pruning
    let _ = file.set_modified(SystemTime::now());
    CacheChoice::Shared(CacheDir {
        path: dir,
        _lock: CacheLock(file),
    })
}

fn toolchain_id(dir: &Path) -> String {
    // Run in the project's directory so a `rust-toolchain.toml` there is honoured.
    Command::new("rustc")
        .arg("-vV")
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown-toolchain".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).into_owned(),
        )
}

/// [`acquire`] for a session: the same rules (the thread-local or process-wide configuration, the process-wide prune
/// `Once`, never pruning the directory being acquired); the caller keeps the returned [`CacheDir`], and so the
/// lock, for the whole run instead of one build.
pub(crate) fn acquire_for_session(source_repo: &Path) -> CacheChoice {
    acquire(source_repo)
}

/// The cache for builds of the repository `source_repo` (the *source*, not the throwaway checkout).
pub(crate) fn acquire(source_repo: &Path) -> CacheChoice {
    let Some(config) = current_config() else {
        return CacheChoice::Throwaway { why: None };
    };
    static PRUNED: Once = Once::new();
    let mut first = false;
    PRUNED.call_once(|| first = true);
    acquire_in(&config, &toolchain_id(source_repo), source_repo, first)
}

// ---- size, listing, pruning ----

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct RepoDirInfo {
    pub path: PathBuf,
    pub bytes: u64,
    /// Modification time of the lock file, touched whenever a run takes the directory.
    pub last_used: SystemTime,
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(m) if m.is_dir() => dir_size(&entry.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Every `<root>/<toolchain>/<repository>` directory.
#[doc(hidden)]
pub fn status(root: &Path) -> anyhow::Result<Vec<RepoDirInfo>> {
    anyhow::ensure!(
        root.join(MARKER).is_file(),
        "{} has no {MARKER} marker file: it is not a CodeRipper build cache",
        root.display()
    );
    let mut out = Vec::new();
    for toolchain_dir in subdirs(root) {
        for path in subdirs(&toolchain_dir) {
            let last_used = fs::metadata(path.join(LOCK_FILE))
                .or_else(|_| fs::metadata(&path))
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            out.push(RepoDirInfo {
                bytes: dir_size(&path),
                path,
                last_used,
            });
        }
    }
    Ok(out)
}

/// Deletes the least recently used repository directories until the cache is at most `max_bytes`, skipping
/// a directory another process has locked and the `keep` directory. Returns what it deleted. Refuses a root
/// without the [`MARKER`] file.
#[doc(hidden)]
pub fn prune_to_cap(
    root: &Path,
    max_bytes: u64,
    keep: Option<&Path>,
) -> anyhow::Result<Vec<PathBuf>> {
    anyhow::ensure!(
        root.join(MARKER).is_file(),
        "{} has no {MARKER} marker file, so it is not a CodeRipper build cache; refusing to delete anything in it",
        root.display()
    );
    let mut dirs = status(root)?;
    let mut total: u64 = dirs.iter().map(|d| d.bytes).sum();
    dirs.sort_by_key(|d| d.last_used);
    let mut removed = Vec::new();
    for dir in dirs {
        if total <= max_bytes {
            break;
        }
        if keep.is_some_and(|k| k == dir.path) {
            continue;
        }
        if remove_if_unlocked(&dir.path) {
            total = total.saturating_sub(dir.bytes);
            removed.push(dir.path);
        }
    }
    Ok(removed)
}

fn remove_if_unlocked(dir: &Path) -> bool {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_FILE));
    let Ok(lock) = lock else {
        return false;
    };
    match lock.try_lock() {
        Ok(()) => {
            // release (and close) before deleting: Windows cannot delete a directory holding an open file
            let _ = lock.unlock();
            drop(lock);
            fs::remove_dir_all(dir).is_ok()
        }
        Err(_) => false,
    }
}

// ---- what the builds did, for the one line `main` prints at the end of a run ----

#[doc(hidden)]
#[derive(Debug, Default, Clone)]
pub struct BuildStats {
    pub fresh: u64,
    pub compiled: u64,
    /// Things the user should be told once (a busy or unusable cache).
    pub notes: Vec<String>,
    /// The cache directory the last build used; `None` when every build ran uncached.
    pub cache_dir: Option<PathBuf>,
}

static STATS: Mutex<BuildStats> = Mutex::new(BuildStats {
    fresh: 0,
    compiled: 0,
    notes: Vec::new(),
    cache_dir: None,
});

pub(crate) fn record(fresh: u64, compiled: u64, cache_dir: Option<&Path>) {
    if let Ok(mut stats) = STATS.lock() {
        stats.fresh += fresh;
        stats.compiled += compiled;
        if let Some(dir) = cache_dir {
            stats.cache_dir = Some(dir.to_path_buf());
        }
    }
}

pub(crate) fn note(text: String) {
    if let Ok(mut stats) = STATS.lock() {
        if !stats.notes.contains(&text) {
            stats.notes.push(text);
        }
    }
}

/// Everything recorded so far, resetting the counters.
#[doc(hidden)]
pub fn take_stats() -> BuildStats {
    STATS
        .lock()
        .map(|mut stats| std::mem::take(&mut *stats))
        .unwrap_or_default()
}

/// The text `main` prints to stderr, or `None` when nothing was built and nothing needs saying.
#[doc(hidden)]
pub fn render_stats(stats: &BuildStats) -> Option<String> {
    if stats.fresh == 0 && stats.compiled == 0 && stats.notes.is_empty() {
        return None;
    }
    let mut lines = vec![match &stats.cache_dir {
        Some(dir) => format!(
            "coderipper: cache {} - {} units fresh, {} compiled",
            dir.display(),
            stats.fresh,
            stats.compiled
        ),
        None => format!("coderipper: cache off - {} units compiled", stats.compiled),
    }];
    lines.extend(stats.notes.iter().map(|n| format!("coderipper: note: {n}")));
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests;
