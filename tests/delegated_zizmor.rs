#![cfg(feature = "cli")]
//! SUP-008 against the REAL zizmor: the pinned release is downloaded (through the embedded lock, checksum-verified) into a
//! temporary tools directory and run over seeded workflows and their clean twins. No fake stands in for the tool here: this is the
//! test that says the mapping reads what zizmor really writes.
//!
//! The pull-request tests need the network once, for the download (~9 MB), and nothing else: zizmor runs offline and every
//! workflow here is written by the test, so the result does not change with the day. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about zizmor, and CI must not set it.

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

/// A tools directory holding the real zizmor, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("zizmor")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned zizmor (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
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

/// A committed copy of a SUP-008 fixture.
fn repo_of(kind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("SUP-008").join(kind), dir.path());
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

/// `coderipper check SUP-008 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "SUP-008",
            "--deny",
            "none",
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

/// `coderipper check SUP-008 --profile extended --deny info` over `project`: a gap never fails the run, a finding would.
fn run_denying(project: &Path) -> assert_cmd::assert::Assert {
    coderipper(tools_dir())
        .args([
            "check",
            "SUP-008",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(project)
        .assert()
}

fn findings(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|l| l.contains("coderipper-finding"))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

macro_rules! needs_tool {
    () => {
        if offline() {
            eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
            return;
        }
    };
}

/// A workflow whose permissions are declared and whose one step is `uses`.
fn workflow(uses: &str) -> Vec<u8> {
    format!(
        "name: ci\non: push\npermissions:\n  contents: read\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {uses}\n        with:\n          persist-credentials: false\n"
    )
    .into_bytes()
}

const PINNED: &str = "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2";

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    needs_tool!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("SUP-008"),
    )
    .expect("the runner works");
    assert_eq!(proofs.rules.len(), 1);
    assert_eq!(proofs.rules[0].verdict, Verdict::Proven);
}

#[test]
fn without_the_tool_the_rule_is_unproven_and_nothing_is_installed() {
    let empty = tempfile::tempdir().unwrap();
    let env = ToolEnv::from_environment()
        .cache_root(empty.path())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("SUP-008"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("zizmor")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn an_unpinned_action_is_a_finding_at_its_file_and_line_and_the_clean_twin_reports_nothing() {
    needs_tool!();
    let bad = repo_of("defective");
    let (code, out, err) = run_json(bad.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    let f = &all[0];
    assert_eq!(f["check_id"], "SUP-008");
    assert_eq!(f["location"]["file"], ".github/workflows/ci.yml");
    assert_eq!(f["location"]["line"], 9);
    assert_eq!(f["subject"], "actions/checkout@v4");
    assert_eq!(f["severity"], "high");
    assert_eq!(f["confidence"], "high");
    run_denying(bad.path()).code(1);
    let good = repo_of("clean");
    let (code, out, err) = run_json(good.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn the_repository_cannot_silence_its_own_findings() {
    needs_tool!();
    let ignored = String::from_utf8(workflow("actions/checkout@v4"))
        .unwrap()
        .replace(
            "actions/checkout@v4",
            "actions/checkout@v4 # zizmor: ignore[unpinned-uses]",
        );
    let repo = repo_with(&[
        (".github/workflows/ci.yml", ignored.as_bytes()),
        (
            ".github/zizmor.yml",
            b"rules:\n  unpinned-uses:\n    disable: true\n",
        ),
        (
            "zizmor.yml",
            b"rules:\n  unpinned-uses:\n    disable: true\n",
        ),
        (".gitignore", b".github/\n"),
    ]);

    // the control: zizmor itself, run the way a user would, from the project, is silenced by the config and the comment
    let plain = std::process::Command::new(&tools().1)
        .args([
            "--offline",
            "--no-progress",
            "-q",
            "--no-exit-codes",
            "--persona",
            "pedantic",
            "--format",
            "json",
            ".github/workflows/ci.yml",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let plain = String::from_utf8_lossy(&plain.stdout).to_string();
    assert!(
        !plain.contains("\"unpinned-uses\""),
        "the control did not silence anything: {plain}"
    );

    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["subject"], "actions/checkout@v4");
    let detail = all[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("zizmor: ignore") && detail.contains("does not honour"),
        "{detail}"
    );
}

#[test]
fn write_all_permissions_are_found_and_the_default_persona_would_not_find_them() {
    needs_tool!();
    let broad = format!(
        "name: ci\non: push\npermissions: write-all\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {PINNED}\n        with:\n          persist-credentials: false\n"
    );
    let repo = repo_with(&[(".github/workflows/ci.yml", broad.as_bytes())]);
    // the control: why the mapping runs the stricter persona
    let regular = std::process::Command::new(&tools().1)
        .args([
            "--offline",
            "--no-config",
            "--no-progress",
            "-q",
            "--no-exit-codes",
            "--persona",
            "regular",
            "--format",
            "json",
        ])
        .arg(repo.path().join(".github/workflows/ci.yml"))
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&regular.stdout).contains("excessive-permissions"),
        "the default persona now reports write-all: revisit the persona"
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["line"], 3);
    assert_eq!(all[0]["subject"], "permissions");
    assert_eq!(all[0]["severity"], "high");
    assert!(all[0]["summary"].as_str().unwrap().contains("write-all"));
}

#[test]
fn a_workflow_without_a_permissions_block_is_a_finding_at_the_job() {
    needs_tool!();
    let none = format!(
        "name: ci\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {PINNED}\n        with:\n          persist-credentials: false\n"
    );
    let repo = repo_with(&[(".github/workflows/ci.yml", none.as_bytes())]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let subjects: Vec<_> = findings(&out)
        .iter()
        .map(|f| f["subject"].as_str().unwrap().to_string())
        .collect();
    assert!(subjects.contains(&"jobs.build".to_string()), "{out}");
}

#[test]
fn another_audits_finding_is_not_this_rules_finding() {
    needs_tool!();
    // pinned, permissions declared, and a template injection (a different rule's business): nothing for SUP-008
    let injected = format!(
        "name: ci\non: issues\npermissions:\n  contents: read\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {PINNED}\n        with:\n          persist-credentials: false\n      - run: echo ${{{{ github.event.issue.title }}}}\n"
    );
    let repo = repo_with(&[(".github/workflows/ci.yml", injected.as_bytes())]);
    // the control: zizmor does see the injection
    let plain = std::process::Command::new(&tools().1)
        .args([
            "--offline",
            "--no-config",
            "--no-progress",
            "-q",
            "--no-exit-codes",
            "--persona",
            "pedantic",
            "--format",
            "json",
        ])
        .arg(repo.path().join(".github/workflows/ci.yml"))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&plain.stdout).contains("template-injection"),
        "the control saw no injection: {}",
        String::from_utf8_lossy(&plain.stdout)
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "{err}");
}

#[test]
fn a_composite_action_is_read_and_other_peoples_trees_are_not() {
    needs_tool!();
    let action = b"name: a\ndescription: d\nruns:\n  using: composite\n  steps:\n    - uses: actions/setup-node@v4\n    - run: echo hi\n      shell: bash\n";
    let repo = repo_with(&[
        (".github/actions/setup/action.yml", action),
        ("node_modules/dep/action.yml", action),
        ("vendor/lib/.github/workflows/ci.yml", &workflow("a/b@v1")),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let places: Vec<String> = findings(&out)
        .iter()
        .map(|f| f["location"]["file"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(places, [".github/actions/setup/action.yml"], "{out}");
}

#[test]
fn a_workflow_zizmor_skipped_makes_a_clean_result_a_gap() {
    needs_tool!();
    // measured: a workflow with a syntax error beside a valid one is skipped with a warning on stderr and zizmor exits 0 with []
    let repo = repo_with(&[
        (".github/workflows/ok.yml", &workflow(PINNED)),
        (
            ".github/workflows/broken.yml",
            b"name: x\non: [push\n  bad: : :\n",
        ),
    ]);
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("SUP-008 not run"))
        .stderr(predicate::str::contains("failed to parse"));
}

#[test]
fn findings_beside_a_skipped_workflow_say_so() {
    needs_tool!();
    let repo = repo_with(&[
        (".github/workflows/ok.yml", &workflow("actions/checkout@v4")),
        (
            ".github/workflows/broken.yml",
            b"name: x\non: [push\n  bad: : :\n",
        ),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    let detail = all[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("could not read part") && detail.contains("not judged"),
        "{detail}"
    );
}

#[test]
fn a_project_whose_every_workflow_is_unreadable_is_a_gap_not_an_error() {
    needs_tool!();
    let repo = repo_with(&[(
        ".github/workflows/broken.yml",
        b"name: x\non: [push\n  bad: : :\n",
    )]);
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("SUP-008 not run"))
        .stderr(predicate::str::contains("could not read any"))
        .stderr(predicate::str::contains("check error").not());
}

#[test]
fn a_project_with_no_workflow_is_a_gap_not_a_clean_rule() {
    needs_tool!();
    let repo = repo_with(&[("README.md", b"# nothing to audit\n")]);
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("SUP-008 not run"))
        .stderr(predicate::str::contains("no tracked GitHub Actions"));
}

#[test]
fn only_tracked_files_are_read() {
    needs_tool!();
    // an untracked workflow (another worktree's, a scratch copy) is not the project's: not read, not judged
    let repo = repo_with(&[(".github/workflows/ok.yml", &workflow(PINNED))]);
    std::fs::write(
        repo.path().join(".github/workflows/scratch.yml"),
        workflow("actions/checkout@v4"),
    )
    .unwrap();
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "{err}");
}

#[test]
fn a_relative_project_path_is_checked_not_silently_skipped() {
    needs_tool!();
    // under target/ so the path is relative to the package root, which is the working directory of `cargo test`
    std::fs::create_dir_all("target").unwrap();
    let holder = tempfile::tempdir_in("target").unwrap();
    let path = holder.path().join(".github/workflows");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("ci.yml"), workflow("actions/checkout@v4")).unwrap();
    commit(holder.path());
    let relative = PathBuf::from("target").join(holder.path().file_name().unwrap());
    assert!(relative.is_relative());
    let (code, out, err) = run_json(&relative);
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], ".github/workflows/ci.yml");
}

#[test]
fn a_project_in_a_directory_with_spaces_brackets_and_non_ascii_is_read_and_named_relative() {
    needs_tool!();
    let holder = tempfile::tempdir().unwrap();
    let project = holder.path().join("pr[12] \u{fc} x");
    let flows = project.join(".github/workflows");
    std::fs::create_dir_all(&flows).unwrap();
    std::fs::write(flows.join("ci.yml"), workflow("actions/checkout@v4")).unwrap();
    commit(&project);
    let (code, out, err) = run_json(&project);
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], ".github/workflows/ci.yml");
}

#[test]
fn a_project_with_more_workflows_than_one_command_line_holds_is_read_whole() {
    needs_tool!();
    // 250 workflows with 100-character names do not fit in a Windows command line (32 767 characters), and the mapping splits them
    // across runs: each is a finding, and none is lost at a seam. (Where the limit is far higher the splitting is not needed for
    // this to pass: the unit test of `chunks` is what pins it there.)
    let files: Vec<(String, Vec<u8>)> = (0..250)
        .map(|i| {
            (
                format!(".github/workflows/wf{i:03}-{}.yml", "x".repeat(90)),
                workflow("actions/checkout@v4"),
            )
        })
        .collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let repo = repo_with(&refs);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(findings(&out).len(), 250, "{err}");
}

#[test]
fn container_services_and_docker_images_are_findings() {
    needs_tool!();
    let images = "name: w\non: push\npermissions: {}\njobs:\n  a:\n    runs-on: ubuntu-latest\n    container: node:18\n    services:\n      db:\n        image: postgres:15\n    steps:\n      - uses: docker://alpine:3.8\n      - run: echo hi\n";
    let repo = repo_with(&[(".github/workflows/w.yml", images.as_bytes())]);
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let mut lines: Vec<u64> = findings(&out)
        .iter()
        .map(|f| f["location"]["line"].as_u64().unwrap())
        .collect();
    lines.sort();
    assert_eq!(lines, [7, 10, 12], "{out}");
}

#[test]
fn a_warning_from_another_audit_does_not_make_a_fully_read_project_a_gap() {
    needs_tool!();
    // measured: a `pull_request_target` workflow on a self-hosted runner makes zizmor warn about the job's shell while it reads every file
    let wf = format!(
        "name: w\non: pull_request_target\npermissions:\n  contents: read\njobs:\n  a:\n    runs-on: self-hosted\n    steps:\n      - uses: {PINNED}\n        with:\n          persist-credentials: false\n      - run: echo hi\n"
    );
    let repo = repo_with(&[(".github/workflows/w.yml", wf.as_bytes())]);
    // the control: the warning is really there, on a run that read everything
    let plain = std::process::Command::new(&tools().1)
        .args([
            "--offline",
            "--no-config",
            "--no-progress",
            "-q",
            "--no-exit-codes",
            "--persona",
            "pedantic",
            "--format",
            "json",
        ])
        .arg(repo.path().join(".github/workflows/w.yml"))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&plain.stderr).contains("WARN"),
        "the control saw no warning: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("not run").not());
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = repo_of("defective");
    coderipper(empty.path())
        .args([
            "check",
            "SUP-008",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("SUP-008 not run"))
        .stderr(predicate::str::contains(
            "coderipper tools install zizmor --install-tools",
        ))
        .stderr(predicate::str::contains("check error").not());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
