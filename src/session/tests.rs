use super::*;
use crate::build_cache::{with_config, CacheConfig};
use crate::cargo_json::{build_all_targets, BuildOutput, CAP_LINTS};
use crate::worktree::RewrittenWorktree;
use std::process::Command;
use std::time::{Instant, SystemTime};

// ---------------------------------------------------------------- fixtures

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) {
    let mut all = vec!["-c", "user.email=t@t", "-c", "user.name=t"];
    all.extend_from_slice(args);
    let status = git_command(dir, &all).status().unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = git_command(dir, args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn commit_all(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

fn pkg(name: &str, dep: Option<(&str, &str)>) -> String {
    let deps = dep.map_or(String::new(), |(n, path)| {
        format!("\n[dependencies]\n{n} = {{ path = \"{path}\" }}\n")
    });
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
}

/// A workspace `a -> b -> c` (packages `sa`, `sb`, `sc`); `b` has two source files so an error between them can be tested.
fn chain_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n",
            ),
            ("a/Cargo.toml", &pkg("sa", Some(("sb", "../b")))),
            ("a/src/lib.rs", "pub fn fa() { sb::fb(); }\n"),
            ("b/Cargo.toml", &pkg("sb", Some(("sc", "../c")))),
            (
                "b/src/lib.rs",
                "pub mod extra;\npub fn fb() { sc::fc(); }\n",
            ),
            ("b/src/extra.rs", "pub fn e() {}\n"),
            ("c/Cargo.toml", &pkg("sc", None)),
            ("c/src/lib.rs", "pub fn fc() {}\n"),
        ],
    );
    commit_all(tmp.path());
    tmp
}

fn config(root: &Path, wait_ms: u64) -> CacheConfig {
    CacheConfig {
        root: root.to_path_buf(),
        wait: Duration::from_millis(wait_ms),
        max_bytes: u64::MAX,
    }
}

/// A rewrite that adds `pub fn <marker>() {}` to the member's `src/lib.rs`.
fn adding(marker: &'static str) -> impl FnMut(&str, &str) -> anyhow::Result<String> {
    move |file, source| {
        if file == "src/lib.rs" {
            Ok(format!("{source}\npub fn {marker}() {{}}\n"))
        } else {
            Ok(source.to_string())
        }
    }
}

fn analyse(repo: &Path, member: &str, marker: &'static str) -> RewrittenWorktree {
    RewrittenWorktree::create_with(&repo.join(member), adding(marker)).unwrap()
}

fn build(guard: &RewrittenWorktree) -> BuildOutput {
    let out = build_all_targets(&guard.root, Some(guard.source_repo()), CAP_LINTS).unwrap();
    assert!(out.success, "the build failed: {:?}", out.diagnostics.len());
    out
}

fn status_clean(checkout: &Path) -> bool {
    git_out(checkout, &["status", "--porcelain", "--untracked-files=no"])
        .trim()
        .is_empty()
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

/// Does any `lib<krate>-*.rlib` under `dir` contain `needle`?
fn artifact_has(dir: &Path, krate: &str, needle: &str) -> bool {
    let mut files = Vec::new();
    rlibs(dir, &mut files);
    files
        .iter()
        .filter(|f| {
            f.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("lib{krate}-")))
        })
        .any(|f| {
            let bytes = std::fs::read(f).unwrap();
            bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
        })
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

fn all_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            all_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

// ---------------------------------------------------------------- restore

#[test]
fn a_session_member_is_restored_byte_for_byte() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            let lib = session.checkout().join("b/src/lib.rs");
            let before = std::fs::read_to_string(&lib).unwrap();
            {
                let guard = analyse(repo.path(), "b", "rewritten_marker");
                assert!(
                    guard.root.starts_with(session.checkout()),
                    "the member must be analysed IN PLACE in the session checkout, not in a new one: {:?}",
                    guard.root
                );
                assert!(std::fs::read_to_string(&lib)
                    .unwrap()
                    .contains("rewritten_marker"));
            }
            assert!(!session.is_poisoned(), "{:?}", session.poison_message());
            assert_eq!(std::fs::read_to_string(&lib).unwrap(), before);
            assert!(status_clean(session.checkout()));
        });
    });
}

#[test]
fn restored_files_get_a_new_mtime_and_files_nothing_touched_keep_theirs() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            let mut files = Vec::new();
            all_files(session.checkout(), &mut files);
            let before: Vec<(PathBuf, SystemTime)> =
                files.iter().map(|f| (f.clone(), mtime(f))).collect();
            std::thread::sleep(Duration::from_millis(50));

            let guard = analyse(repo.path(), "b", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()));
            std::thread::sleep(Duration::from_millis(50));
            let build_ended = SystemTime::now();
            drop(guard);

            for (file, old) in before {
                let rewritten = file.ends_with("b/src/lib.rs") || file.ends_with("b/src/extra.rs");
                if rewritten {
                    assert!(
                        mtime(&file) >= build_ended,
                        "{file:?} must be stamped AFTER the build ended (never the old time)"
                    );
                } else {
                    assert_eq!(
                        mtime(&file),
                        old,
                        "{file:?} was not touched and must keep its mtime"
                    );
                }
            }
        });
    });
}

#[test]
fn a_restored_member_is_never_served_as_its_rewritten_self() {
    // The hazard test, with an EXPLICIT order (the CLI cannot produce it): analyse `c`, then `a` (which builds `c`).
    // An artifact compiled from c's rewritten source must not be served to `a`'s build: c's restored files are
    // newer than that artifact, so c is recompiled from the original bytes.
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            {
                let guard = analyse(repo.path(), "c", "rewritten_marker");
                assert!(guard.root.starts_with(session.checkout()), "not in place");
                build(&guard);
                assert!(
                    artifact_has(cache.path(), "sc", "rewritten_marker"),
                    "positive control: c's artifact built during its own analysis contains the marker"
                );
            }
            let out_a = {
                let guard = analyse(repo.path(), "a", "unused_marker_a");
                build(&guard)
            };
            let c_units: Vec<_> = out_a
                .units
                .iter()
                .filter(|u| u.target_name == "sc")
                .collect();
            assert!(!c_units.is_empty(), "{:?}", out_a.units);
            assert!(
                c_units.iter().all(|u| !u.fresh),
                "a's build was served c's REWRITTEN artifact as fresh: {c_units:?}"
            );
            assert!(
                !artifact_has(cache.path(), "sc", "rewritten_marker"),
                "a final c artifact still contains the rewritten symbol"
            );
        });
    });
}

#[test]
fn a_failed_restore_is_retried_then_poisons_the_session() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            // two failures are within the retry bound
            RESTORE_FAILURES.with(|c| c.set(2));
            let guard = analyse(repo.path(), "b", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            drop(guard);
            assert!(!session.is_poisoned(), "{:?}", session.poison_message());
            assert!(status_clean(session.checkout()));

            // more failures than attempts: poisoned, and nothing more is analysed
            RESTORE_FAILURES.with(|c| c.set(10));
            let guard = analyse(repo.path(), "b", "rewritten_marker");
            drop(guard);
            RESTORE_FAILURES.with(|c| c.set(0));
            assert!(session.is_poisoned());
            let err = RewrittenWorktree::create_with(&repo.path().join("c"), adding("m"))
                .err()
                .expect("a poisoned session must refuse to analyse another member");
            assert!(err.to_string().contains("dirty"), "{err}");
        });
    });
}

#[test]
fn a_dirty_checkout_after_a_clean_looking_restore_poisons_the_session() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            let guard = analyse(repo.path(), "b", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            // a check wrote a TRACKED file outside the member: the member-scoped restore cannot see it
            std::fs::write(
                session.checkout().join("c/src/lib.rs"),
                "pub fn changed() {}\n",
            )
            .unwrap();
            drop(guard);
            assert!(session.is_poisoned());
            let why = session.poison_message().unwrap();
            assert!(why.contains("c/src/lib.rs"), "{why}");
        });
    });
}

#[test]
fn an_error_inside_a_check_still_restores_the_member() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            // files are visited in sorted order: extra.rs is rewritten, then lib.rs fails
            let extra = session.checkout().join("b/src/extra.rs");
            let err = RewrittenWorktree::create_with(&repo.path().join("b"), |file, source| {
                if file == "src/lib.rs" {
                    // extra.rs was rewritten just before: IN PLACE in the session checkout, or this is not a session run
                    assert!(
                        std::fs::read_to_string(&extra)
                            .unwrap()
                            .contains("from_the_failed_run"),
                        "the member is not being analysed in place in the session checkout"
                    );
                    anyhow::bail!("boom")
                }
                Ok(format!("{source}\npub fn from_the_failed_run() {{}}\n"))
            })
            .err()
            .expect("the rewrite fails");
            assert!(err.to_string().contains("boom"), "{err}");
            assert!(
                git_out(
                    session.checkout(),
                    &["status", "--porcelain", "--untracked-files=no"]
                )
                .trim()
                .is_empty(),
                "the half-rewritten member was not restored"
            );
            assert!(!session.is_poisoned());
            assert!(
                !std::fs::read_to_string(session.checkout().join("b/src/extra.rs"))
                    .unwrap()
                    .contains("from_the_failed_run")
            );
        });
    });
}

// ---------------------------------------------------------------- Cargo.lock

/// A repository whose workspace is at `prefix` ("" or "proj/") and whose COMMITTED Cargo.lock is out of step with
/// the manifest (the member's version was bumped without refreshing the lock), so a plain `cargo build` rewrites it.
fn stale_lock_repo(prefix: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let p = |name: &str| format!("{prefix}{name}");
    write(
        tmp.path(),
        &[
            (
                &p("Cargo.toml"),
                "[workspace]\nmembers = [\"m\"]\nresolver = \"2\"\n",
            ),
            (&p("m/Cargo.toml"), &pkg("lockprobe", None)),
            (&p("m/src/lib.rs"), "pub fn f() {}\n"),
        ],
    );
    let root = tmp.path().join(prefix.trim_end_matches('/'));
    let status = Command::new("cargo")
        .args(["generate-lockfile"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    // bump the version without refreshing the lock
    write(
        tmp.path(),
        &[(
            &p("m/Cargo.toml"),
            &pkg("lockprobe", None).replace("0.1.0", "0.1.1"),
        )],
    );
    commit_all(tmp.path());
    tmp
}

fn assert_lock_rewrite_does_not_poison(prefix: &str) {
    let repo = stale_lock_repo(prefix);
    let cache = tempfile::tempdir().unwrap();
    let member = repo.path().join(format!("{prefix}m"));
    with_config(config(cache.path(), 500), || {
        let session = Session::open(&member).unwrap();
        with_session(&session, || {
            let guard =
                RewrittenWorktree::create_with(&member, adding("rewritten_marker")).unwrap();
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            build(&guard);
            let status = git_out(
                session.checkout(),
                &["status", "--porcelain", "--untracked-files=no"],
            );
            assert!(
                status.contains(&format!("{prefix}Cargo.lock")),
                "precondition: the build must have rewritten the committed lock, got: {status:?}"
            );
            drop(guard);
            assert!(
                !session.is_poisoned(),
                "a rewritten workspace Cargo.lock must not poison the session: {:?}",
                session.poison_message()
            );
        });
    });
}

#[test]
fn a_rewritten_root_cargo_lock_does_not_poison_the_session() {
    assert_lock_rewrite_does_not_poison("");
}

#[test]
fn a_rewritten_cargo_lock_of_a_workspace_below_the_repository_root_is_exempt_at_its_own_prefix() {
    assert_lock_rewrite_does_not_poison("proj/");
}

// ---------------------------------------------------------------- restore scope

#[test]
fn a_member_nested_in_another_member_is_restored_without_touching_the_outer_one() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"outer\", \"outer/inner\"]\nresolver = \"2\"\n",
            ),
            ("outer/Cargo.toml", &pkg("outerpkg", None)),
            ("outer/src/lib.rs", "pub fn o() {}\n"),
            ("outer/inner/Cargo.toml", &pkg("innerpkg", None)),
            ("outer/inner/src/lib.rs", "pub fn i() {}\n"),
        ],
    );
    commit_all(tmp.path());
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(tmp.path()).unwrap();
        with_session(&session, || {
            let outer_lib = session.checkout().join("outer/src/lib.rs");
            let outer_before = mtime(&outer_lib);
            std::thread::sleep(Duration::from_millis(50));
            let guard = analyse(&tmp.path().join("outer"), "inner", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            drop(guard);
            assert!(!session.is_poisoned(), "{:?}", session.poison_message());
            assert_eq!(
                mtime(&outer_lib),
                outer_before,
                "the outer member must not be touched"
            );
        });
    });
}

#[test]
fn a_root_package_member_restores_without_deleting_untracked_files_or_a_non_ignored_target() {
    // the audit measured `git clean -fd -- .` at a root-package member deleting a non-ignored `target/` and other
    // members' untracked files; the restore must only restore.
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                &format!(
                    "{}\n[workspace]\nmembers = [\"sub\"]\n",
                    pkg("rootpkg", None)
                ),
            ),
            ("src/lib.rs", "pub fn r() {}\n"),
            ("sub/Cargo.toml", &pkg("subpkg", None)),
            ("sub/src/lib.rs", "pub fn s() {}\n"),
        ],
    );
    commit_all(tmp.path());
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(tmp.path()).unwrap();
        with_session(&session, || {
            write(
                session.checkout(),
                &[("target/keep.txt", "x"), ("sub/untracked.txt", "x")],
            );
            let guard =
                RewrittenWorktree::create_with(tmp.path(), adding("rewritten_marker")).unwrap();
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            drop(guard);
            assert!(!session.is_poisoned(), "{:?}", session.poison_message());
            assert!(
                session.checkout().join("target/keep.txt").is_file(),
                "an untracked target/ was deleted"
            );
            assert!(
                session.checkout().join("sub/untracked.txt").is_file(),
                "another member's file was deleted"
            );
            assert!(status_clean(session.checkout()));
        });
    });
}

// ---------------------------------------------------------------- thread-local scope

#[test]
fn the_session_is_thread_local_and_other_repositories_are_ordinary() {
    let repo = chain_repo();
    let other = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            let guard = analyse(repo.path(), "b", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()), "not in place");

            // another thread is not in the session
            let repo_path = repo.path().to_path_buf();
            let checkout = session.checkout().to_path_buf();
            std::thread::spawn(move || {
                let g = RewrittenWorktree::create_with(&repo_path.join("c"), adding("m")).unwrap();
                assert!(
                    !g.root.starts_with(&checkout),
                    "a spawned thread must get its own checkout"
                );
            })
            .join()
            .unwrap();

            // another repository on this thread is an ordinary checkout
            let g = RewrittenWorktree::create_with(&other.path().join("c"), adding("m")).unwrap();
            assert!(!g.root.starts_with(session.checkout()));
            assert!(session_for(Some(&other.path().join("c"))).is_none());
            assert!(session_for(Some(&repo.path().join("c"))).is_some());
            assert!(session_for(None).is_none());
        });
    });
}

// ---------------------------------------------------------------- the lock and the target

#[test]
fn a_session_never_waits_on_its_own_lock() {
    // The session holds the cache lock through its own handle; a per-build `acquire` would try_lock a second
    // handle in the same process, get WouldBlock, wait the whole bound and fall back with a "cache busy" note.
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 2000), || {
        let session = Session::open(repo.path()).unwrap();
        assert!(
            session.cache_dir().is_some(),
            "the session must have got the cache"
        );
        with_session(&session, || {
            for member in ["c", "b"] {
                let guard = analyse(repo.path(), member, "rewritten_marker");
                assert!(guard.root.starts_with(session.checkout()), "not in place");
                let out = build(&guard);
                assert!(out.cache_note.is_none(), "{member}: {:?}", out.cache_note);
            }
        });
    });
}

#[test]
fn the_session_target_is_outside_the_checkout_when_the_cache_is_busy() {
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 300), || {
        // someone else holds the cache lock
        let holder = build_cache::acquire(repo.path());
        assert!(matches!(holder, CacheChoice::Shared(_)));
        let session = Session::open(repo.path()).unwrap();
        assert!(
            session.cache_dir().is_none(),
            "the session cannot have the cache"
        );
        assert!(!session.target().starts_with(session.checkout()));
        with_session(&session, || {
            let guard = analyse(repo.path(), "c", "rewritten_marker");
            assert!(guard.root.starts_with(session.checkout()), "not in place");
            build(&guard);
            let mut built = Vec::new();
            rlibs(session.target(), &mut built);
            assert!(
                !built.is_empty(),
                "the session built nowhere near its own target"
            );
            assert!(
                !session.checkout().join("target").exists(),
                "cargo built inside the checkout, where a restore could delete it"
            );
        });
        drop(holder);
    });
}

#[test]
fn the_session_target_is_the_same_slot_a_project_run_uses() {
    // an external (out-of-repository) dependency built during a session must be found fresh by an ordinary run
    let dep = tempfile::tempdir().unwrap();
    write(
        dep.path(),
        &[
            ("Cargo.toml", &pkg("depcrate", None)),
            ("src/lib.rs", "pub fn d() {}\n"),
        ],
    );
    let dep_path = dep.path().to_string_lossy().replace('\\', "/");
    let repo = tempfile::tempdir().unwrap();
    write(
        repo.path(),
        &[
            ("Cargo.toml", &pkg("app", Some(("depcrate", &dep_path)))),
            ("src/lib.rs", "pub fn a() { depcrate::d(); }\n"),
        ],
    );
    commit_all(repo.path());
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 500), || {
        {
            let session = Session::open(repo.path()).unwrap();
            with_session(&session, || {
                let guard = RewrittenWorktree::create_with(repo.path(), adding("m1")).unwrap();
                assert!(guard.root.starts_with(session.checkout()), "not in place");
                build(&guard);
            });
        } // the session ends: its lock is released
        let guard = RewrittenWorktree::create_with(repo.path(), adding("m2")).unwrap();
        let out = build(&guard);
        let dep_units: Vec<_> = out
            .units
            .iter()
            .filter(|u| u.target_name == "depcrate")
            .collect();
        assert!(!dep_units.is_empty());
        assert!(
            dep_units.iter().all(|u| u.fresh),
            "an ordinary run did not find the session's external dependency in the shared slot: {dep_units:?}"
        );
    });
}

#[test]
fn an_overlapping_run_cannot_poison_the_session() {
    // The audit's R4-B1 interleave. The session builds `a` (so c is compiled from its pristine files), then ANOTHER
    // run, on another thread outside the session, rewrites and builds sibling `c`, then the session builds `a`
    // again. With the lock held the other run waits its bound and builds uncached, so the shared slot never holds
    // its rewritten `c`. (If the session let go of the lock, the other run would write its rewrite into the slot
    // and the session's older `c` files would call it fresh.)
    let repo = chain_repo();
    let cache = tempfile::tempdir().unwrap();
    with_config(config(cache.path(), 300), || {
        let session = Session::open(repo.path()).unwrap();
        with_session(&session, || {
            {
                let guard = analyse(repo.path(), "a", "m_first");
                assert!(guard.root.starts_with(session.checkout()), "not in place");
                build(&guard);
            }

            let other_config = config(cache.path(), 300);
            let other_repo = repo.path().to_path_buf();
            let (note, waited) = std::thread::spawn(move || {
                with_config(other_config, || {
                    let guard = RewrittenWorktree::create_with(
                        &other_repo.join("c"),
                        adding("rewritten_by_other_run"),
                    )
                    .unwrap();
                    let started = Instant::now();
                    let out = build_all_targets(&guard.root, Some(guard.source_repo()), CAP_LINTS)
                        .unwrap();
                    (out.cache_note, started.elapsed())
                })
            })
            .join()
            .unwrap();
            let note = note.expect("the other run must have been told the cache is busy");
            assert!(note.contains("cache busy"), "{note}");
            assert!(waited >= Duration::from_millis(250), "{waited:?}");

            {
                let guard = analyse(repo.path(), "a", "m_second");
                build(&guard);
            }
            assert!(
                !artifact_has(cache.path(), "sc", "rewritten_by_other_run"),
                "the other run's rewritten c reached the shared slot while the session held the lock"
            );
        });
    });
}
