#![cfg(feature = "cli")]
//! The build cache through the real CLI. Each test sets its cache environment on the CHILD process only (no
//! race between tests), and uses `unused-parameters`, which builds the package, so the `cache:` line the CLI
//! prints on stderr says how many compilation units were reused (`fresh`) and how many were built.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn run_cli(args: &[&str], envs: &[(&str, &str)]) -> Run {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn check(repo: &Path, envs: &[(&str, &str)]) -> Run {
    run_cli(
        &[
            "check",
            "unused-parameters",
            "--project",
            repo.to_str().unwrap(),
        ],
        envs,
    )
}

fn cached(cache: &Path) -> [(&'static str, String); 1] {
    [("CODERIPPER_CACHE_DIR", cache.to_string_lossy().into_owned())]
}

fn check_cached(repo: &Path, cache: &Path) -> Run {
    let envs = cached(cache);
    let refs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    check(repo, &refs)
}

fn check_uncached(repo: &Path) -> Run {
    check(repo, &[("CODERIPPER_CACHE", "off")])
}

/// `(fresh, compiled)` from the `coderipper: cache ...` line.
fn stats(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|l| l.starts_with("coderipper: cache "))
        .unwrap_or_else(|| panic!("no `coderipper: cache` line in stderr:\n{stderr}"));
    let tail = line.split(" - ").nth(1).unwrap();
    let number = |s: &str| s.trim().parse::<u64>().unwrap();
    if let Some((fresh, rest)) = tail.split_once(" units fresh, ") {
        (
            number(fresh),
            number(rest.trim_end_matches(" compiled").trim()),
        )
    } else {
        (0, number(tail.trim_end_matches(" units compiled")))
    }
}

const APP_UNUSED: &str = "pub fn a(x: u32, unused: u32) -> u32 {\n    depcrate::d();\n    x\n}\n";
const APP_USED: &str = "pub fn a(x: u32) -> u32 {\n    depcrate::d();\n    x\n}\n";
const DEP_LIB: &str = "pub fn d() {}\n";

fn manifest(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
}

/// A dependency that is NOT in the repository, named by an ABSOLUTE path (a relative path outside the repository
/// does not resolve from the throwaway checkout), and the repository `app` that uses it.
fn external_dep_fixture() -> (tempfile::TempDir, tempfile::TempDir) {
    let dep = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dep.path().join("src")).unwrap();
    std::fs::write(dep.path().join("Cargo.toml"), manifest("depcrate", "")).unwrap();
    std::fs::write(dep.path().join("src/lib.rs"), DEP_LIB).unwrap();
    let dep_path = dep.path().to_string_lossy().replace('\\', "/");
    let app_manifest = manifest(
        "app",
        &format!("\n[dependencies]\ndepcrate = {{ path = \"{dep_path}\" }}\n"),
    );
    let repo = common::git_repo_with(&[
        ("Cargo.toml", app_manifest.as_str()),
        ("src/lib.rs", APP_UNUSED),
    ]);
    (dep, repo)
}

/// The dependency is a SIBLING MEMBER inside the repository. Returns the repository and the `app` member dir.
fn in_repo_dep_fixture() -> (tempfile::TempDir, PathBuf) {
    let app_manifest = manifest(
        "app",
        "\n[dependencies]\ndepcrate = { path = \"../dep\" }\n",
    );
    let repo = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\", \"dep\"]\nresolver = \"2\"\n",
        ),
        ("app/Cargo.toml", app_manifest.as_str()),
        ("app/src/lib.rs", APP_UNUSED),
        ("dep/Cargo.toml", manifest("depcrate", "").as_str()),
        ("dep/src/lib.rs", DEP_LIB),
    ]);
    let app = repo.path().join("app");
    (repo, app)
}

fn git(dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

/// `<cache>/<toolchain>/<repository>/.coderipper.lock` of the one repository a priming run created.
fn the_lock_file(cache: &Path) -> PathBuf {
    let mut found = Vec::new();
    for toolchain in std::fs::read_dir(cache).unwrap().flatten() {
        if !toolchain.path().is_dir() {
            continue;
        }
        for repo in std::fs::read_dir(toolchain.path()).unwrap().flatten() {
            let lock = repo.path().join(".coderipper.lock");
            if lock.is_file() {
                found.push(lock);
            }
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

#[test]
fn a_second_run_reuses_an_out_of_repo_dependency() {
    let (_dep, repo) = external_dep_fixture();
    let cache = tempfile::tempdir().unwrap();
    let first = check_cached(repo.path(), cache.path());
    assert!(first.ok, "{}", first.stderr);
    let second = check_cached(repo.path(), cache.path());
    assert!(second.ok, "{}", second.stderr);
    let (fresh1, compiled1) = stats(&first.stderr);
    let (fresh2, compiled2) = stats(&second.stderr);
    assert_eq!(fresh1, 0, "a cold cache reuses nothing: {}", first.stderr);
    assert!(compiled1 >= 2, "{}", first.stderr);
    assert!(
        fresh2 >= 1,
        "the dependency is reused in the second run: {}",
        second.stderr
    );
    assert!(compiled2 < compiled1, "{} vs {}", compiled2, compiled1);
    assert_eq!(
        first.stdout, second.stdout,
        "the cache must not change the findings"
    );
}

#[test]
fn an_in_repo_path_crate_is_still_recompiled_and_the_findings_are_still_right() {
    // The measured limit (plan, row H): a sibling member inside the repository is rebuilt in every new worktree,
    // because its checkout is newer than the cached artifact. This test pins that; if a later change makes
    // siblings reusable, THIS test is the one that is consciously updated.
    let (repo, app) = in_repo_dep_fixture();
    let _keep = &repo;
    let cache = tempfile::tempdir().unwrap();
    let first = check_cached(&app, cache.path());
    let second = check_cached(&app, cache.path());
    assert!(first.ok && second.ok, "{}\n{}", first.stderr, second.stderr);
    let (_, compiled1) = stats(&first.stderr);
    let (fresh2, compiled2) = stats(&second.stderr);
    assert_eq!(fresh2, 0, "{}", second.stderr);
    assert_eq!(compiled2, compiled1, "{}", second.stderr);
    assert_eq!(first.stdout, second.stdout);
    assert!(
        first.stdout.contains("unused-parameters"),
        "{}",
        first.stdout
    );
}

#[test]
fn a_changed_source_is_never_served_from_the_cache() {
    // Cargo decides "fresh" by file mtime and replays a fresh unit's cached warnings, so a stale hit would
    // keep reporting the OLD source's finding. Run 1 reports the unused parameter; the source then changes
    // (the parameter becomes used) and run 2, from a NEW worktree on the SAME cache, must not.
    let (_dep, repo) = external_dep_fixture();
    let cache = tempfile::tempdir().unwrap();
    let first = check_cached(repo.path(), cache.path());
    assert!(
        first.stdout.contains("unused-parameters"),
        "precondition: run 1 must report the unused parameter: {}",
        first.stdout
    );
    std::fs::write(repo.path().join("src/lib.rs"), APP_USED).unwrap();
    git(repo.path(), &["commit", "-aq", "-m", "use the parameter"]);
    let second = check_cached(repo.path(), cache.path());
    assert!(second.ok, "{}", second.stderr);
    assert!(
        !second.stdout.contains("unused-parameters"),
        "a stale cache hit replayed the old warning: {}",
        second.stdout
    );
    // The finding alone is NOT a sensitive oracle here: `unused-parameters` only reports a warning that matches
    // a parameter site in the CURRENT source, so a replayed stale warning is filtered out by the check itself.
    // The unit counts are: a package whose source changed must be COMPILED again, never served as fresh, while
    // its unchanged external dependency is still reused.
    let (fresh2, compiled2) = stats(&second.stderr);
    assert!(
        compiled2 >= 1,
        "the changed package was served from the cache: {}",
        second.stderr
    );
    assert!(
        fresh2 >= 1,
        "and the dependency was still reused: {}",
        second.stderr
    );
}

#[test]
fn an_unusable_cache_gives_identical_findings_and_one_note() {
    let (_dep, repo) = external_dep_fixture();
    let reference = check_uncached(repo.path());
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a-file");
    std::fs::write(&file, "x").unwrap();
    let under_a_file = file.join("cache");
    let run = check_cached(repo.path(), &under_a_file);
    assert_eq!(run.ok, reference.ok, "{}", run.stderr);
    assert_eq!(run.stdout, reference.stdout);
    let notes = run
        .stderr
        .lines()
        .filter(|l| l.starts_with("coderipper: note:"))
        .count();
    assert_eq!(notes, 1, "{}", run.stderr);
    assert!(run.stderr.contains("cache off"), "{}", run.stderr);
}

#[test]
fn a_busy_cache_is_not_waited_on() {
    // The 11-hour failure mode: a build that silently queues behind a lock. The TEST holds the repository's lock
    // itself (forced overlap, deterministic), and the run must give up after its bounded wait, say so, name
    // the last holder, and still produce the same findings.
    let (_dep, repo) = external_dep_fixture();
    let cache = tempfile::tempdir().unwrap();
    let priming = check_cached(repo.path(), cache.path());
    assert!(priming.ok, "{}", priming.stderr);
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(the_lock_file(cache.path()))
        .unwrap();
    lock.try_lock().unwrap();

    let envs = [
        ("CODERIPPER_CACHE_DIR", cache.path().to_str().unwrap()),
        ("CODERIPPER_CACHE_WAIT_SECS", "1"),
    ];
    let started = std::time::Instant::now();
    let busy = check(repo.path(), &envs);
    let elapsed = started.elapsed();
    assert!(busy.ok, "{}", busy.stderr);
    assert!(busy.stderr.contains("cache busy"), "{}", busy.stderr);
    assert!(
        busy.stderr.contains("pid "),
        "names the last holder: {}",
        busy.stderr
    );
    assert_eq!(busy.stdout, priming.stdout);
    assert!(
        elapsed < std::time::Duration::from_secs(120),
        "bounded, not a hang: {elapsed:?}"
    );
    drop(lock);
}

#[test]
fn cache_status_and_prune_work_through_the_cli() {
    let (_dep, repo) = external_dep_fixture();
    let cache = tempfile::tempdir().unwrap();
    assert!(check_cached(repo.path(), cache.path()).ok);
    let envs = [("CODERIPPER_CACHE_DIR", cache.path().to_str().unwrap())];

    let status = run_cli(&["cache", "status"], &envs);
    assert!(status.ok, "{}", status.stderr);
    assert!(status.stdout.contains("bytes"), "{}", status.stdout);
    assert_eq!(status.stdout.lines().count(), 1, "{}", status.stdout);

    let pruned = run_cli(&["cache", "prune", "--max-gb", "0"], &envs);
    assert!(pruned.ok, "{}", pruned.stderr);
    assert!(pruned.stdout.contains("removed 1"), "{}", pruned.stdout);
    let after = run_cli(&["cache", "status"], &envs);
    assert_eq!(after.stdout.trim(), "(empty)", "{}", after.stdout);

    // with the cache switched off there is nothing to report on
    let off = run_cli(&["cache", "status"], &[("CODERIPPER_CACHE", "off")]);
    assert!(!off.ok);
    assert!(off.stderr.contains("off"), "{}", off.stderr);
}
