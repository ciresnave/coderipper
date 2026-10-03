//! Cross-run staleness: a build must never be served a unit another CodeRipper run wrote into the shared cache.
//!
//! Cargo decides a unit is fresh by comparing its source files' mtimes with the artifact. A checkout made BEFORE
//! another run's build finished has files older than that run's artifact, and unit hashes do not depend on the
//! checkout's path, so without a refresh the older checkout is served the other run's compile (the artifact then
//! contains the other run's rewritten symbol).

use super::*;
use crate::build_cache::{with_config, CacheConfig};
use crate::worktree::RewrittenWorktree;
use std::time::{Duration, SystemTime};

const KRATE: &str = "stale_probe";

fn git(dir: &Path, args: &[&str]) {
    let mut all = vec!["-c", "user.email=t@t", "-c", "user.name=t"];
    all.extend_from_slice(args);
    let status = crate::github::git_command(dir, &all).status().unwrap();
    assert!(status.success(), "git {args:?} failed");
}

/// A one-package repository with a library and NO dev-dependency (Step 0 of the plan: a dev-dependency that
/// enables a feature makes `--all-targets` a different unit from `--lib`).
fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        format!("[package]\nname = \"{KRATE}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "pub fn base() {}\n").unwrap();
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    tmp
}

/// A throwaway checkout whose rewrite adds `pub fn <marker>() {}` (`pub`: a private unused function is not in the
/// rlib, so a check for it would pass vacuously).
fn checkout(repo: &Path, marker: &str) -> RewrittenWorktree {
    RewrittenWorktree::create_with(repo, |file, source| {
        if file == "src/lib.rs" {
            Ok(format!("{source}\npub fn {marker}() {{}}\n"))
        } else {
            Ok(source.to_string())
        }
    })
    .unwrap()
}

fn config(root: &Path) -> CacheConfig {
    CacheConfig {
        root: root.to_path_buf(),
        wait: Duration::from_millis(500),
        max_bytes: u64::MAX,
    }
}

fn rlibs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rlibs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rlib") {
            out.push(path);
        }
    }
}

/// Does any `lib<KRATE>-*.rlib` under the cache contain `needle`?
fn artifact_has(cache: &Path, needle: &str) -> bool {
    let mut files = Vec::new();
    rlibs(cache, &mut files);
    files
        .iter()
        .filter(|f| {
            f.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("lib{KRATE}-")))
        })
        .any(|f| {
            let bytes = std::fs::read(f).unwrap();
            bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
        })
}

fn lib_units(out: &BuildOutput) -> Vec<&UnitReport> {
    out.units
        .iter()
        .filter(|u| u.target_name == KRATE && u.kind.iter().any(|k| k == "lib") && !u.test)
        .collect()
}

#[test]
fn a_build_after_another_run_wrote_the_same_unit_is_never_served_that_runs_artifact() {
    let repo = fixture();
    let cache = tempfile::tempdir().unwrap();
    // BOTH checkouts exist before either builds, so B's files are older than A's artifact.
    let a = checkout(repo.path(), "rewritten_by_a");
    let b = checkout(repo.path(), "rewritten_by_b");
    with_config(config(cache.path()), || {
        let out_a = build_all_targets(&a.root, Some(a.source_repo()), CAP_LINTS).unwrap();
        assert!(out_a.success, "A's build failed");
        assert!(
            artifact_has(cache.path(), "rewritten_by_a"),
            "positive control: A's own artifact contains A's marker"
        );

        let out_b = build_all_targets(&b.root, Some(b.source_repo()), CAP_LINTS).unwrap();
        assert!(out_b.success, "B's build failed");
        let units = lib_units(&out_b);
        assert!(!units.is_empty(), "no lib unit reported: {:?}", out_b.units);
        assert!(
            units.iter().all(|u| !u.fresh),
            "B was served A's lib unit as fresh: {units:?}"
        );
        assert!(artifact_has(cache.path(), "rewritten_by_b"));
        assert!(
            !artifact_has(cache.path(), "rewritten_by_a"),
            "B's artifact still contains A's rewritten symbol"
        );
    });
}

#[test]
fn the_same_slot_is_not_shared_across_checks_unsafely() {
    // The cross-CHECK case: reachability builds `--lib`, `unused-parameters` builds `--all-targets`, with the same
    // RUSTFLAGS. Step 0 measured that they share one lib unit when no dev-dependency changes feature
    // unification, so a `--lib` build with one rewrite must not be served to an `--all-targets` build with another.
    // Both checkouts are created first (the natural order, create X / build X / create Y / build Y, would give Y
    // newer files and pass on the unfixed code).
    let repo = fixture();
    let cache = tempfile::tempdir().unwrap();
    let x = checkout(repo.path(), "rewritten_by_x");
    let y = checkout(repo.path(), "rewritten_by_y");
    with_config(config(cache.path()), || {
        let out_x = build_lib_only(&x.root, Some(x.source_repo()), CAP_LINTS).unwrap();
        assert!(out_x.success);
        assert!(artifact_has(cache.path(), "rewritten_by_x"));

        let out_y = build_all_targets(&y.root, Some(y.source_repo()), CAP_LINTS).unwrap();
        assert!(out_y.success);
        let units = lib_units(&out_y);
        assert!(!units.is_empty(), "{:?}", out_y.units);
        assert!(
            units.iter().all(|u| !u.fresh),
            "the `--all-targets` build was served the `--lib` build's unit: {units:?}"
        );
        assert!(artifact_has(cache.path(), "rewritten_by_y"));
        assert!(!artifact_has(cache.path(), "rewritten_by_x"));
    });
}

// ---- freshen_checkout itself ----

fn set_old(path: &Path) {
    let old = SystemTime::now() - Duration::from_secs(3600);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(old)
        .unwrap();
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn freshen_checkout_stamps_every_file_recursively_but_not_git() {
    let tmp = tempfile::tempdir().unwrap();
    let files = ["a.rs", "sub/b.rs", "sub/deep/c.toml"];
    for f in files {
        let p = tmp.path().join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "x").unwrap();
        set_old(&p);
    }
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    let git_file = tmp.path().join(".git/config");
    std::fs::write(&git_file, "x").unwrap();
    set_old(&git_file);
    let git_before = mtime(&git_file);

    let started = SystemTime::now() - Duration::from_secs(1);
    freshen_checkout(tmp.path()).unwrap();

    for f in files {
        assert!(mtime(&tmp.path().join(f)) >= started, "{f} was not stamped");
    }
    assert_eq!(mtime(&git_file), git_before, ".git must be left alone");
}

#[test]
fn freshen_checkout_stamps_a_read_only_file_and_leaves_it_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("ro.rs");
    std::fs::write(&p, "x").unwrap();
    set_old(&p);
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&p, perms).unwrap();

    let started = SystemTime::now() - Duration::from_secs(1);
    freshen_checkout(tmp.path()).unwrap();
    assert!(mtime(&p) >= started, "the read-only file was not stamped");
    assert!(
        std::fs::metadata(&p).unwrap().permissions().readonly(),
        "its read-only attribute must be put back"
    );
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&p, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn freshen_checkout_never_follows_a_symlink() {
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("outside.rs");
    std::fs::write(&target, "x").unwrap();
    set_old(&target);
    let before = mtime(&target);
    let tmp = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&target, tmp.path().join("link.rs")).unwrap();
    freshen_checkout(tmp.path()).unwrap();
    assert_eq!(
        mtime(&target),
        before,
        "a symlink must not stamp its target"
    );
}

#[test]
fn a_file_that_cannot_be_stamped_is_reported_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("fine.rs"), "x").unwrap();
    std::fs::write(tmp.path().join("bad.rs"), "x").unwrap();
    let err = freshen_with(tmp.path(), &|p: &Path| {
        if p.file_name().is_some_and(|n| n == "bad.rs") {
            Err(std::io::Error::other("injected"))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(err.to_string().contains("bad.rs"), "{err}");
}

#[test]
fn an_unstampable_file_makes_the_build_uncached_with_a_note_and_releases_the_lock() {
    let repo = fixture();
    let cache = tempfile::tempdir().unwrap();
    let wt = checkout(repo.path(), "rewritten_by_a");
    with_config(config(cache.path()), || {
        FAIL_ON.with(|f| *f.borrow_mut() = Some("lib.rs".to_string()));
        let out = build_all_targets(&wt.root, Some(wt.source_repo()), CAP_LINTS);
        FAIL_ON.with(|f| *f.borrow_mut() = None);
        let out = out.unwrap();
        assert!(out.success, "the fallback build must still succeed");
        let note = out.cache_note.expect("a note naming the problem");
        assert!(note.contains("lib.rs"), "{note}");

        // nothing was built into the cache, and the lock is free again
        let mut built = Vec::new();
        rlibs(cache.path(), &mut built);
        assert!(built.is_empty(), "the fallback used the cache: {built:?}");
        assert!(
            matches!(
                crate::build_cache::acquire(wt.source_repo()),
                crate::build_cache::CacheChoice::Shared(_)
            ),
            "the lock was not released"
        );
    });
}
