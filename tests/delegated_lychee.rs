#![cfg(feature = "cli")]
//! DOC-010 against the REAL lychee: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over seeded documents and their clean twins. No fake stands in for the tool here: this is the
//! test that says the mapping reads what lychee really writes.
//!
//! The pull-request tests need the network once, for the download (~19 MB), and nothing else: every link in them is to a file in
//! the fixture, or to `127.0.0.1:9` (a port nothing listens on, which answers "connection refused" the same way every day). To run
//! the suite offline set `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about lychee, and
//! CI must not set it. A test that needs a live web site is `#[ignore]` and runs only in the scheduled workflow.

use assert_cmd::Command;
use coderipper::conformance::{run, Options, Verdict};
use coderipper::module::DelegatedModule;
use coderipper::tools::{Consent, ToolEnv};
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn offline() -> bool {
    std::env::var("CODERIPPER_SKIP_NETWORK_TESTS").is_ok_and(|v| v == "1")
}

/// A tools directory holding the real lychee, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("lychee")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned lychee (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
            });
        assert!(path.is_file(), "{path:?}");
        (dir, path)
    })
}

fn tools_dir() -> &'static Path {
    tools().0.path()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// Commits everything in `dir` (force-adding, so a `.gitignore` in the fixture does not keep a file out of the repository).
fn commit(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["add", "-A", "-f"]);
    git(dir, &["commit", "-q", "-m", "fixture"]);
}

/// A committed repository holding these files.
fn repo_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    commit(dir.path());
    dir
}

/// A committed copy of a DOC-010 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("DOC-010").join(kind), dir.path());
    std::fs::remove_file(dir.path().join("expect.toml")).unwrap();
    commit(dir.path());
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

/// `coderipper check DOC-010 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--message-format",
            "json",
            "--project",
        ])
        .arg(project)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn findings(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|l| l.contains("coderipper-finding"))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

macro_rules! needs_network {
    () => {
        if offline() {
            eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
            return;
        }
    };
}

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    needs_network!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).network(true).rule("DOC-010"),
    )
    .expect("the runner works");
    assert_eq!(proofs.rules.len(), 1);
    assert_eq!(proofs.rules[0].verdict, Verdict::Proven);
}

#[test]
fn without_the_network_flag_the_same_fixtures_are_unproven_not_failed() {
    needs_network!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("DOC-010"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(_)),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
}

#[test]
fn without_the_tool_the_rule_is_unproven_and_nothing_is_installed() {
    let empty = tempfile::tempdir().unwrap();
    let env = ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).network(true).rule("DOC-010"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("lychee")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_dead_local_link_is_a_finding_at_its_document_and_line_and_the_clean_twin_reports_nothing() {
    needs_network!();
    let bad = repo_of("defective");
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    let f = &all[0];
    assert_eq!(f["check_id"], "DOC-010");
    assert_eq!(f["location"]["file"], "README.md");
    assert_eq!(f["location"]["line"], 3);
    assert_eq!(f["subject"], "docs/missing.md");
    assert_eq!(f["severity"], "low");
    assert_eq!(f["confidence"], "high");
    coderipper(tools_dir())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--deny",
            "low",
            "--project",
        ])
        .arg(bad.path())
        .assert()
        .code(1);
    let good = repo_of("clean");
    let (code, out, err) = run_json(good.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn the_repository_cannot_silence_its_own_dead_links() {
    needs_network!();
    let repo = repo_with(&[
        ("README.md", b"# T\n[dead](nothing.md)\n"),
        ("lychee.toml", b"exclude = [\".*\"]\n"),
        (".lycheeignore", b"nothing.md\n"),
        (".gitignore", b"README.md\n"),
    ]);

    // the control: lychee itself, run the way a user would, from the project, is silenced by these files
    let plain = std::process::Command::new(&tools().1)
        .args([
            "--offline",
            "--no-progress",
            "--format",
            "json",
            "README.md",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let plain = String::from_utf8_lossy(&plain.stdout).to_string();
    assert!(
        plain.contains("\"errors\": 0") && plain.contains("\"excludes\": 1"),
        "the control did not silence anything: {plain}"
    );

    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["subject"], "nothing.md");
}

#[test]
fn a_dead_link_in_a_hidden_directory_or_html_is_found_and_other_peoples_documents_are_not_judged() {
    needs_network!();
    let repo = repo_with(&[
        ("README.md", b"# T\n[ok](.github/CONTRIBUTING.md)\n"),
        (".github/CONTRIBUTING.md", b"[dead](gone.md)\n"),
        ("site/page.html", b"<a href=\"gone.html\">x</a>\n"),
        ("node_modules/dep/README.md", b"[dead](theirs.md)\n"),
        ("vendor/lib/README.md", b"[dead](theirs.md)\n"),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let mut places: Vec<String> = findings(&out)
        .iter()
        .map(|f| f["location"]["file"].as_str().unwrap().to_string())
        .collect();
    places.sort();
    assert_eq!(
        places,
        [".github/CONTRIBUTING.md", "site/page.html"],
        "{out}"
    );
}

#[test]
fn a_project_with_no_links_is_a_gap_not_a_clean_rule() {
    needs_network!();
    let repo = repo_with(&[("README.md", b"# nothing to follow\n")]);
    coderipper(tools_dir())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("DOC-010 not run"))
        .stderr(predicate::str::contains("no links"));
}

#[test]
fn a_link_that_could_not_be_settled_is_not_a_clean_rule() {
    needs_network!();
    // 127.0.0.1:9 answers "connection refused" every time: lychee reports an error that is not "gone"
    let repo = repo_with(&[
        (
            "README.md",
            b"# T\n[ok](docs/a.md)\n[closed](http://127.0.0.1:9/)\n",
        ),
        ("docs/a.md", b"# A\n"),
    ]);
    coderipper(tools_dir())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("DOC-010 not run"))
        .stderr(predicate::str::contains("1 other link"));
}

#[test]
fn dead_links_beside_a_link_that_could_not_be_settled_say_so_in_the_finding() {
    needs_network!();
    let repo = repo_with(&[(
        "README.md",
        b"# T\n[dead](nothing.md)\n[closed](http://127.0.0.1:9/)\n",
    )]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    let detail = all[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("1 other link") && detail.contains("not judged"),
        "{detail}"
    );
}

#[test]
fn a_document_lychee_skips_makes_a_clean_result_a_gap() {
    needs_network!();
    // measured: a file that is not UTF-8 is skipped with a warning on stderr and lychee still exits 0
    let repo = repo_with(&[
        ("README.md", b"# T\n[ok](docs/a.md)\n"),
        ("docs/a.md", b"# A\n"),
        ("docs/binary.md", b"\xff\xfe[bad](x.md)\n"),
    ]);
    coderipper(tools_dir())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("DOC-010 not run"))
        .stderr(predicate::str::contains("invalid UTF-8"));
}

#[test]
fn a_relative_project_path_is_checked_not_silently_skipped() {
    needs_network!();
    // under target/ so the path is relative to the package root, which is the working directory of `cargo test`
    std::fs::create_dir_all("target").unwrap();
    let holder = tempfile::tempdir_in("target").unwrap();
    std::fs::write(holder.path().join("README.md"), "# T\n[dead](nothing.md)\n").unwrap();
    commit(holder.path());
    let relative = PathBuf::from("target").join(holder.path().file_name().unwrap());
    assert!(relative.is_relative());
    let (code, out, err) = run_json(&relative);
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], "README.md");
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args([
            "check",
            "DOC-010",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("DOC-010 not run"))
        .stderr(predicate::str::contains(
            "coderipper tools install lychee --install-tools",
        ))
        .stderr(predicate::str::contains("check error").not());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_fast_run_leaves_the_network_rule_alone_and_a_sweep_reports_it_as_a_gap() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args(["fast", "--profile", "extended", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("DOC-010").not());
    coderipper(empty.path())
        .args(["sweep", "--profile", "extended", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("DOC-010 not run"));
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

/// The scheduled run's canary (`.github/workflows/live-tools.yml`): a web page that is gone is a finding, and a page that exists is
/// clean. It depends on the live web, so it is not part of a pull request's checks: a failure here is a signal that the page moved
/// or that a site or lychee changed, never a reason to block a merge.
#[test]
#[ignore = "requests live web pages; run by the scheduled workflow with --include-ignored"]
fn a_gone_web_page_is_a_finding_and_a_live_one_is_not() {
    let repo = repo_with(&[(
        "README.md",
        b"# T\n[live](https://example.com/)\n[gone](https://github.com/ciresnave/this-repository-does-not-exist-zz9)\n",
    )]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["line"], 3);
    assert_eq!(all[0]["confidence"], "medium");
}

#[test]
fn a_project_in_a_directory_with_spaces_brackets_and_non_ascii_is_read_and_named_relative() {
    needs_network!();
    // lychee reads its inputs as globs and prints file:// URLs percent-encoded: both broke on such a path (measured)
    let holder = tempfile::tempdir().unwrap();
    let project = holder.path().join("pr[12] \u{fc} x");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("README.md"),
        "# T\n[dead](gone%20f\u{fc}le.md#top)\n",
    )
    .unwrap();
    commit(&project);
    let (code, out, err) = run_json(&project);
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["subject"], "gone f\u{fc}le.md");
    assert_eq!(all[0]["location"]["file"], "README.md");
}

#[test]
fn only_tracked_documents_are_read() {
    needs_network!();
    // an untracked document (a virtualenv's, another worktree's, a scratch note) is not the project's: not read, not judged
    let repo = repo_with(&[
        ("README.md", b"# T\n[ok](docs/a.md)\n"),
        ("docs/a.md", b"# A\n"),
    ]);
    std::fs::create_dir_all(repo.path().join(".venv/lib")).unwrap();
    std::fs::write(
        repo.path().join(".venv/lib/README.md"),
        "[dead](theirs.md)\n",
    )
    .unwrap();
    std::fs::write(repo.path().join("scratch.md"), "[dead](nothing.md)\n").unwrap();
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "{err}");
}
