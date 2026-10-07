use super::*;
use std::fs;
use std::time::{Duration, SystemTime};

fn cfg(root: &Path, wait_ms: u64, max_bytes: u64) -> CacheConfig {
    CacheConfig {
        root: root.to_path_buf(),
        wait: Duration::from_millis(wait_ms),
        max_bytes,
    }
}

/// A throwaway git repository to be "the source repository".
fn source_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let status = crate::github::git_command(tmp.path(), &["init", "-q"])
        .status()
        .unwrap();
    assert!(status.success());
    tmp
}

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(name, _)| *name == k)
            .map(|(_, v)| v.to_string())
    }
}

#[test]
fn the_key_is_stable_and_short() {
    // Pinned against an independent FNV-1a 64 (Python), keeping the low 48 bits: the key names a directory
    // that must survive a CodeRipper upgrade, so it must not change with the Rust version.
    assert_eq!(
        key("rustc 1.98.1 (test)", Path::new("/r")),
        ("9c361f68b446".to_string(), "5407b49cb8e4".to_string())
    );
    let (a, b) = key("anything", Path::new("/x"));
    assert_eq!((a.len(), b.len()), (12, 12));
    assert!(a.chars().chain(b.chars()).all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn two_repositories_and_two_toolchains_get_different_directories() {
    let root = Path::new("/cache");
    let dirs = [
        dir_for(root, "rustc 1.98.1 (test)", Path::new("/r")),
        dir_for(root, "rustc 1.98.1 (test)", Path::new("/other")),
        dir_for(root, "rustc 1.97.0 (test)", Path::new("/r")),
        dir_for(root, "rustc 1.97.0 (test)", Path::new("/other")),
    ];
    for (i, a) in dirs.iter().enumerate() {
        assert!(a.starts_with(root));
        for b in &dirs[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn the_config_honours_the_override_and_the_defaults_are_never_in_a_project() {
    let get = lookup(&[("CODERIPPER_CACHE_DIR", "/explicit")]);
    assert_eq!(
        config_from_env(&get).unwrap().root,
        PathBuf::from("/explicit")
    );
    // an empty override is ignored, not "cache in the current directory"
    let get = lookup(&[("CODERIPPER_CACHE_DIR", ""), ("LOCALAPPDATA", "/local")]);
    assert_eq!(
        config_from_env(&get).unwrap().root,
        PathBuf::from("/local").join("coderipper").join("build")
    );
    let get = lookup(&[("XDG_CACHE_HOME", "/xdg"), ("HOME", "/home/u")]);
    assert_eq!(
        config_from_env(&get).unwrap().root,
        PathBuf::from("/xdg").join("coderipper").join("build")
    );
    let get = lookup(&[("HOME", "/home/u")]);
    let root = config_from_env(&get).unwrap().root;
    assert_eq!(
        root,
        PathBuf::from("/home/u")
            .join(".cache")
            .join("coderipper")
            .join("build")
    );
    assert!(root.components().all(|c| c.as_os_str() != "target"));
    // nothing to derive a root from: no cache, not a guess
    assert!(config_from_env(&lookup(&[])).is_none());
}

#[test]
fn the_cache_can_be_switched_off_and_its_limits_have_defaults() {
    let off = lookup(&[("CODERIPPER_CACHE", "off"), ("CODERIPPER_CACHE_DIR", "/c")]);
    assert!(config_from_env(&off).is_none());
    let c = config_from_env(&lookup(&[("CODERIPPER_CACHE_DIR", "/c")])).unwrap();
    assert_eq!(c.wait, Duration::from_secs(5));
    assert_eq!(c.max_bytes, 20 * (1u64 << 30));
    let c = config_from_env(&lookup(&[
        ("CODERIPPER_CACHE_DIR", "/c"),
        ("CODERIPPER_CACHE_WAIT_SECS", "1"),
        ("CODERIPPER_CACHE_MAX_GB", "2"),
    ]))
    .unwrap();
    assert_eq!(c.wait, Duration::from_secs(1));
    assert_eq!(c.max_bytes, 2 * (1u64 << 30));
}

#[test]
fn a_new_root_gets_the_marker_and_a_shared_directory_is_returned() {
    let root = tempfile::tempdir().unwrap();
    let repo = source_repo();
    let choice = acquire_in(&cfg(root.path(), 100, u64::MAX), "tc", repo.path(), false);
    let CacheChoice::Shared(dir) = choice else {
        panic!("expected Shared, got {choice:?}")
    };
    assert!(root.path().join(MARKER).is_file());
    assert!(dir.path.starts_with(root.path()));
    assert!(dir.path.is_dir());
}

#[test]
fn a_held_lock_makes_the_next_run_throwaway_after_the_wait_not_a_hang() {
    let root = tempfile::tempdir().unwrap();
    let repo = source_repo();
    let c = cfg(root.path(), 300, u64::MAX);
    let first = acquire_in(&c, "tc", repo.path(), false);
    assert!(matches!(first, CacheChoice::Shared(_)));

    let started = std::time::Instant::now();
    let second = acquire_in(&c, "tc", repo.path(), false);
    let elapsed = started.elapsed();
    let CacheChoice::Throwaway { why: Some(why) } = second else {
        panic!("expected a busy Throwaway, got {second:?}")
    };
    assert!(why.contains("cache busy"), "{why}");
    // the holder's pid comes from the unlocked sidecar: on Windows a locked file cannot be read at all
    assert!(
        why.contains(&format!("pid {}", std::process::id())),
        "{why}"
    );
    assert!(elapsed >= Duration::from_millis(250), "{elapsed:?}");
    // no upper bound: a loaded machine can stall a thread for many seconds; that the call returned at all, with a
    // 300 ms wait configured, is what shows it did not hang
}

#[test]
fn a_released_lock_is_taken_by_the_next_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = source_repo();
    let c = cfg(root.path(), 100, u64::MAX);
    drop(acquire_in(&c, "tc", repo.path(), false));
    assert!(matches!(
        acquire_in(&c, "tc", repo.path(), false),
        CacheChoice::Shared(_)
    ));
}

#[test]
fn an_unusable_root_or_source_is_throwaway_with_a_reason_never_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a-file");
    fs::write(&file, "x").unwrap();
    let repo = source_repo();
    let under_a_file = cfg(&file.join("sub"), 100, u64::MAX);
    let CacheChoice::Throwaway { why: Some(why) } =
        acquire_in(&under_a_file, "tc", repo.path(), false)
    else {
        panic!("expected Throwaway with a reason")
    };
    assert!(why.contains("cache"), "{why}");

    // not a git repository: there is no repository to key by
    let not_git = tempfile::tempdir().unwrap();
    let ok_root = tempfile::tempdir().unwrap();
    let choice = acquire_in(
        &cfg(ok_root.path(), 100, u64::MAX),
        "tc",
        not_git.path(),
        false,
    );
    assert!(
        matches!(choice, CacheChoice::Throwaway { why: Some(_) }),
        "{choice:?}"
    );
}

// ---- pruning ----

fn fake_repo_dir(root: &Path, tc: &str, name: &str, bytes: usize, age_secs: u64) -> PathBuf {
    let dir = root.join(tc).join(name);
    fs::create_dir_all(dir.join("target")).unwrap();
    fs::write(dir.join("target").join("blob"), vec![0u8; bytes]).unwrap();
    let lock = dir.join(".coderipper.lock");
    fs::write(&lock, "").unwrap();
    let when = SystemTime::now() - Duration::from_secs(age_secs);
    fs::OpenOptions::new()
        .write(true)
        .open(&lock)
        .unwrap()
        .set_modified(when)
        .unwrap();
    dir
}

fn fake_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(MARKER), "").unwrap();
    root
}

#[test]
fn prune_removes_the_oldest_unlocked_directories_until_under_the_cap() {
    let root = fake_root();
    let oldest = fake_repo_dir(root.path(), "tc", "a", 400, 3000);
    let middle = fake_repo_dir(root.path(), "tc", "b", 400, 2000);
    let newest = fake_repo_dir(root.path(), "tc", "c", 400, 1000);
    let removed = prune_to_cap(root.path(), 1000, None).unwrap();
    assert_eq!(removed, vec![oldest.clone()]);
    assert!(!oldest.exists() && middle.is_dir() && newest.is_dir());
}

#[test]
fn prune_never_removes_a_locked_directory_or_the_one_being_kept() {
    let root = fake_root();
    let locked = fake_repo_dir(root.path(), "tc", "locked", 600, 5000);
    let kept = fake_repo_dir(root.path(), "tc", "kept", 600, 4000);
    let newer = fake_repo_dir(root.path(), "tc", "newer", 600, 100);
    let guard = fs::OpenOptions::new()
        .write(true)
        .open(locked.join(".coderipper.lock"))
        .unwrap();
    guard.try_lock().unwrap();
    let removed = prune_to_cap(root.path(), 10, Some(&kept)).unwrap();
    assert_eq!(
        removed,
        vec![newer.clone()],
        "only the unlocked, unkept one"
    );
    assert!(locked.is_dir() && kept.is_dir() && !newer.exists());
}

#[test]
fn prune_refuses_a_directory_without_the_marker() {
    // CODERIPPER_CACHE_DIR=C:\Projects must never turn into "delete the oldest project".
    let not_a_cache = tempfile::tempdir().unwrap();
    let victim = fake_repo_dir(not_a_cache.path(), "tc", "a", 400, 3000);
    let err = prune_to_cap(not_a_cache.path(), 1, None).unwrap_err();
    assert!(err.to_string().contains("marker"), "{err}");
    assert!(victim.is_dir());
}

#[test]
fn status_lists_each_repository_directory_with_its_size() {
    let root = fake_root();
    fake_repo_dir(root.path(), "tc", "a", 400, 3000);
    fake_repo_dir(root.path(), "tc", "b", 900, 10);
    let mut listed = status(root.path()).unwrap();
    listed.sort_by_key(|d| d.bytes);
    assert_eq!(listed.len(), 2);
    assert!(listed[0].bytes >= 400 && listed[0].bytes < 900);
    assert!(listed[1].bytes >= 900);
    assert_ne!(listed[0].last_used, listed[1].last_used);
}

#[test]
fn acquire_prunes_the_oldest_unlocked_directory_when_over_the_cap() {
    let root = fake_root();
    let old = fake_repo_dir(root.path(), "tc", "old", 700, 9000);
    let recent = fake_repo_dir(root.path(), "tc", "recent", 700, 50);
    let repo = source_repo();
    let choice = acquire_in(&cfg(root.path(), 100, 1000), "tc", repo.path(), true);
    let CacheChoice::Shared(dir) = choice else {
        panic!("expected Shared")
    };
    assert!(!old.exists(), "the oldest unlocked directory is pruned");
    assert!(recent.is_dir(), "the recent one fits under the cap");
    assert!(
        dir.path.is_dir(),
        "the directory being acquired is never pruned"
    );
}

#[test]
fn stats_render_a_line_only_when_something_was_built_and_note_once() {
    let none = BuildStats::default();
    assert_eq!(render_stats(&none), None);
    let s = BuildStats {
        fresh: 3,
        compiled: 2,
        cache_dir: Some(PathBuf::from("/c/x")),
        notes: vec!["cache busy".to_string()],
    };
    let text = render_stats(&s).unwrap();
    assert!(text.contains("3 units fresh, 2 compiled"), "{text}");
    assert!(text.contains("cache busy"), "{text}");
    let off = BuildStats {
        compiled: 4,
        ..BuildStats::default()
    };
    assert!(render_stats(&off).unwrap().contains("cache off"));
}
