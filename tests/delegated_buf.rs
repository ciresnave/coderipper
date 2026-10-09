#![cfg(feature = "cli")]
//! API-006 against the REAL buf: the pinned release is downloaded (through the embedded lock, checksum-verified) into a temporary
//! tools directory and run over seeded schemas and their clean twins. No fake stands in for the tool here: this is the test that
//! says the mapping reads what buf really writes.
//!
//! `buf breaking` compares two versions of the schemas, so every project here is a git repository with a history, built by the test.
//! The tests need the network once, for the download (~40 MB), and nothing else: buf runs against a local repository and every schema
//! here is written by the test, so the result does not change with the day. To run the suite offline set
//! `CODERIPPER_SKIP_NETWORK_TESTS=1`: they then print `SKIPPED` and pass, which proves nothing about buf, and CI must not set it.

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

/// A tools directory holding the real buf, installed once for the whole test binary, and the path of the tool.
fn tools() -> &'static (tempfile::TempDir, PathBuf) {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = ToolEnv::from_environment()
            .cache_root(dir.path())
            .consent(Consent::Granted)
            .resolve("buf")
            .unwrap_or_else(|e| {
                panic!("cannot install the pinned buf (offline? set CODERIPPER_SKIP_NETWORK_TESTS=1): {e}")
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

macro_rules! needs_tool {
    () => {
        if offline() {
            eprintln!("SKIPPED: CODERIPPER_SKIP_NETWORK_TESTS is set");
            return;
        }
    };
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Writes these files below `dir` (creating directories).
fn write(dir: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A", "-f"]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// The schema of `age`'s type: `int32` is the released one.
fn schema(age: &str) -> String {
    format!(
        "syntax = \"proto3\";\n\npackage a.v1;\n\nmessage Foo {{\n  string name = 1;\n  {age} age = 2;\n}}\n"
    )
}

const S: &str = "a/v1/s.proto";

/// A repository with one commit, `base`, holding `files`, and the tag `v1` on it. The branch is `main`.
fn released(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    write(dir.path(), files);
    commit(dir.path(), "base");
    git(dir.path(), &["tag", "v1"]);
    dir
}

fn coderipper(tools: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools);
    cmd
}

/// `coderipper check API-006 --profile extended --message-format json --project <project>`: exit code, stdout, stderr.
fn run_json(project: &Path) -> (i32, String, String) {
    let out = coderipper(tools_dir())
        .args([
            "check",
            "API-006",
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

/// `coderipper check API-006 --profile extended --deny info` over `project`: a gap never fails the run, a finding would.
fn run_denying(project: &Path) -> assert_cmd::assert::Assert {
    coderipper(tools_dir())
        .args([
            "check",
            "API-006",
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

/// The pinned baseline in `.coderipper.toml`: the tag `v1` of `released`.
const AGAINST_V1: (&str, &str) = (".coderipper.toml", "[buf]\nbaseline = \"v1\"\n");

#[test]
fn the_claim_is_earned_by_the_fixtures_against_the_real_tool() {
    needs_tool!();
    let env = ToolEnv::from_environment()
        .cache_root(tools_dir())
        .consent(Consent::NotGiven);
    let proofs = run(
        &DelegatedModule::new(env),
        &Options::new(fixtures()).rule("API-006"),
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
        &Options::new(fixtures()).rule("API-006"),
    )
    .unwrap();
    assert!(
        matches!(&proofs.rules[0].verdict, Verdict::Unproven(why) if why.contains("buf")),
        "{:?}",
        proofs.rules[0].verdict
    );
    assert!(!proofs.any_failed() && !proofs.any_errored());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn a_retyped_field_is_a_finding_at_its_file_and_line_and_a_compatible_change_reports_nothing() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32")), AGAINST_V1]);
    write(repo.path(), &[(S, &schema("string"))]);
    commit(repo.path(), "retype");
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    let f = &all[0];
    assert_eq!(f["check_id"], "API-006");
    assert_eq!(f["location"]["file"], S);
    assert_eq!(f["location"]["line"], 7);
    assert_eq!(f["subject"], "FIELD_SAME_TYPE");
    assert_eq!(f["severity"], "high");
    assert_eq!(f["confidence"], "high");
    run_denying(repo.path()).code(1);

    // the twin: a new field with a new number
    let twin = released(&[(S, &schema("int32")), AGAINST_V1]);
    write(
        twin.path(),
        &[(
            S,
            &schema("int32").replace("}\n", "  bool active = 3;\n}\n"),
        )],
    );
    commit(twin.path(), "add a field");
    let (code, out, err) = run_json(twin.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
    assert!(!err.contains("not run"), "a clean twin is judged: {err}");
}

#[test]
fn an_uncommitted_change_is_compared_with_the_baseline() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32")), AGAINST_V1]);
    write(repo.path(), &[(S, &schema("string"))]);
    let (_, out, err) = run_json(repo.path());
    assert_eq!(findings(&out).len(), 1, "{out}\n{err}");
}

#[test]
fn the_default_branch_is_a_gap_and_a_feature_branch_gets_a_real_verdict() {
    needs_tool!();
    // origin has the released schema; the clone's `main` is origin's: its merge-base with origin/HEAD is HEAD itself
    let origin = released(&[(S, &schema("int32"))]);
    let clones = tempfile::tempdir().unwrap();
    let clone = clones.path().join("work");
    git(
        clones.path(),
        &["clone", "-q", origin.path().to_str().unwrap(), "work"],
    );
    run_denying(&clone)
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains(
            "no baseline distinct from the tree",
        ))
        .stderr(predicate::str::contains("check error").not());
    let (_, out, _) = run_json(&clone);
    assert!(
        findings(&out).is_empty(),
        "a gap is not a finding and not a clean: {out}"
    );

    // a branch that retypes the field: a real comparison with where it left main
    git(&clone, &["checkout", "-q", "-b", "feature"]);
    write(&clone, &[(S, &schema("string"))]);
    commit(&clone, "retype");
    let (code, out, err) = run_json(&clone);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(findings(&out).len(), 1, "{out}\n{err}");
    run_denying(&clone).code(1);

    // a branch that changes no schema is a gap again, not a clean rule
    git(&clone, &["checkout", "-q", "main"]);
    git(&clone, &["checkout", "-q", "-b", "docs"]);
    write(&clone, &[("README.md", "docs\n")]);
    commit(&clone, "docs");
    run_denying(&clone).code(0).stderr(predicate::str::contains(
        "no baseline distinct from the tree",
    ));
}

#[test]
fn no_origin_is_a_gap_that_says_how_to_name_a_baseline() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32"))]);
    write(repo.path(), &[(S, &schema("string"))]);
    commit(repo.path(), "retype");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("[buf] baseline"))
        .stderr(predicate::str::contains("check error").not());
}

#[test]
fn a_named_baseline_that_does_not_exist_is_an_error_not_a_gap() {
    needs_tool!();
    let repo = released(&[
        (S, &schema("string")),
        (".coderipper.toml", "[buf]\nbaseline = \"v9\"\n"),
    ]);
    let (code, out, err) = run_json(repo.path());
    assert!(findings(&out).is_empty(), "{out}");
    assert!(
        err.contains("does not resolve") || out.contains("does not resolve"),
        "{code}\n{out}\n{err}"
    );
    // a ref that is an option is refused, not passed to git
    let repo = released(&[
        (S, &schema("string")),
        (
            ".coderipper.toml",
            "[buf]\nbaseline = \"--upload-pack=x\"\n",
        ),
    ]);
    let (_, out, err) = run_json(repo.path());
    assert!(
        err.contains("not a git ref") || out.contains("not a git ref"),
        "{out}\n{err}"
    );
}

#[test]
fn the_repository_cannot_silence_its_own_findings() {
    needs_tool!();
    let ignoring = "version: v2\nbreaking:\n  ignore:\n    - a\n";
    let repo = released(&[
        (S, &schema("int32")),
        (AGAINST_V1.0, AGAINST_V1.1),
        ("buf.yaml", ignoring),
    ]);
    write(
        repo.path(),
        &[
            (
                S,
                &format!(
                    "// buf:breaking:ignore FIELD_SAME_TYPE\n{}",
                    schema("string")
                ),
            ),
            ("a/buf.yaml", ignoring),
        ],
    );
    commit(repo.path(), "retype");

    // the control: buf itself, run the way a user would, from the project, is silenced by the config
    let plain = std::process::Command::new(&tools().1)
        .args([
            "breaking",
            ".",
            "--against",
            ".git#ref=v1",
            "--error-format",
            "json",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let plain = String::from_utf8_lossy(&plain.stdout).to_string();
    assert!(
        !plain.contains("FIELD_SAME_TYPE"),
        "the control did not silence anything: {plain}"
    );

    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["subject"], "FIELD_SAME_TYPE");
}

#[test]
fn a_removed_file_and_a_removed_field_are_findings_and_the_baseline_file_is_named() {
    needs_tool!();
    let repo = released(&[
        (S, &schema("int32")),
        (
            "a/v1/gone.proto",
            "syntax = \"proto3\";\n\npackage a.v1;\n\nmessage Gone {}\n",
        ),
        AGAINST_V1,
    ]);
    std::fs::remove_file(repo.path().join("a/v1/gone.proto")).unwrap();
    write(
        repo.path(),
        &[(S, &schema("int32").replace("  int32 age = 2;\n", ""))],
    );
    commit(repo.path(), "remove");
    let (_, out, err) = run_json(repo.path());
    let subjects: Vec<String> = findings(&out)
        .iter()
        .map(|f| {
            format!(
                "{}:{}",
                f["location"]["file"].as_str().unwrap(),
                f["subject"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        subjects.contains(&"a/v1/gone.proto:FILE_NO_DELETE".to_string()),
        "{subjects:?}\n{err}"
    );
    assert!(
        subjects.contains(&format!("{S}:FIELD_NO_DELETE")),
        "{subjects:?}\n{err}"
    );
}

#[test]
fn modules_are_read_from_buf_yaml_so_imports_between_files_resolve() {
    needs_tool!();
    // the files import each other by their path below `proto/`, which is only a root because proto/buf.yaml says so
    let t = "syntax = \"proto3\";\n\npackage a.v1;\n\nmessage T {\n  int32 x = 1;\n}\n";
    let s = |age: &str| {
        format!(
            "syntax = \"proto3\";\n\npackage a.v1;\n\nimport \"a/v1/t.proto\";\n\nmessage Foo {{\n  T t = 1;\n  {age} age = 2;\n}}\n"
        )
    };
    let repo = released(&[
        ("proto/buf.yaml", "version: v1\n"),
        ("proto/a/v1/t.proto", t),
        ("proto/a/v1/s.proto", &s("int32")),
        AGAINST_V1,
    ]);
    write(repo.path(), &[("proto/a/v1/s.proto", &s("string"))]);
    commit(repo.path(), "retype");
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], "proto/a/v1/s.proto");
    assert!(!err.contains("not run"), "{err}");
}

#[test]
fn a_member_project_is_judged_by_its_own_schemas_and_named_relative_to_itself() {
    needs_tool!();
    let repo = released(&[
        ("svc/a/v1/s.proto", &schema("int32")),
        ("svc/.coderipper.toml", AGAINST_V1.1),
        ("other/a/v1/s.proto", &schema("int32")),
    ]);
    write(
        repo.path(),
        &[
            ("svc/a/v1/s.proto", &schema("string")),
            ("other/a/v1/s.proto", &schema("string")),
        ],
    );
    commit(repo.path(), "retype both");
    let (code, out, err) = run_json(&repo.path().join("svc"));
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(
        all.len(),
        1,
        "the other member's schema is not this project's: {out}\n{err}"
    );
    assert_eq!(all[0]["location"]["file"], S);
}

#[test]
fn a_linked_worktree_is_compared_too() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32")), AGAINST_V1]);
    let trees = tempfile::tempdir().unwrap();
    let tree = trees.path().join("wt");
    git(
        repo.path(),
        &["worktree", "add", "-q", "-b", "wtb", tree.to_str().unwrap()],
    );
    write(&tree, &[(S, &schema("string"))]);
    commit(&tree, "retype");
    let (code, out, err) = run_json(&tree);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(findings(&out).len(), 1, "{out}\n{err}");
}

#[test]
fn a_schema_buf_cannot_compile_makes_a_clean_result_a_gap_and_findings_say_so() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32")), AGAINST_V1]);
    write(
        repo.path(),
        &[
            (
                "a/v1/imp.proto",
                "syntax = \"proto3\";\n\npackage a.v1;\n\nimport \"missing/dependency.proto\";\n\nmessage Z {}\n",
            ),
            (
                S,
                &schema("int32").replace("}\n", "  bool active = 3;\n}\n"),
            ),
        ],
    );
    commit(repo.path(), "add");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("could not compile"))
        .stderr(predicate::str::contains("check error").not());

    // buf builds all of the schemas or none: beside a real breaking change it still prints no violation, so the rule stays a gap
    // (the control: without the file that does not compile, the same change is found)
    write(repo.path(), &[(S, &schema("string"))]);
    commit(repo.path(), "retype");
    let (_, out, err) = run_json(repo.path());
    assert!(findings(&out).is_empty(), "{out}\n{err}");
    assert!(err.contains("API-006 not run"), "{err}");
    git(repo.path(), &["rm", "-q", "a/v1/imp.proto"]);
    commit(repo.path(), "drop the broken file");
    let (_, out, err) = run_json(repo.path());
    assert_eq!(findings(&out).len(), 1, "the control: {out}\n{err}");
}

#[test]
fn a_baseline_with_no_schema_is_a_gap_because_a_new_schema_cannot_break_anyone() {
    needs_tool!();
    let repo = released(&[("README.md", "nothing yet\n"), AGAINST_V1]);
    write(repo.path(), &[(S, &schema("int32"))]);
    commit(repo.path(), "first schema");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("check error").not());
}

#[test]
fn a_stray_untracked_schema_is_not_the_projects() {
    needs_tool!();
    let repo = released(&[
        (S, &schema("int32")),
        AGAINST_V1,
        (".gitignore", "build/\n"),
    ]);
    write(
        repo.path(),
        &[(
            S,
            &schema("int32").replace("}\n", "  bool active = 3;\n}\n"),
        )],
    );
    commit(repo.path(), "add a field");
    // an ignored directory with a schema that is in no commit
    write(
        repo.path(),
        &[(
            "build/a/v1/s.proto",
            "syntax = \"proto3\";\npackage junk;\nmessage Foo {}\n",
        )],
    );
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(findings(&out).is_empty(), "{out}");
}

#[test]
fn a_missing_tool_is_a_reported_gap_that_fails_nothing_and_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let repo = released(&[(S, &schema("string")), AGAINST_V1]);
    coderipper(empty.path())
        .args([
            "check",
            "API-006",
            "--profile",
            "extended",
            "--deny",
            "info",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains(
            "coderipper tools install buf --install-tools",
        ))
        .stderr(predicate::str::contains("check error").not());
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}

#[test]
fn the_baselines_own_buf_yaml_is_replaced_too() {
    needs_tool!();
    // the released version carried a buf.yaml with a registry dependency, which no registry token reaches here. (Measured: this passes
    // with or without `--against-config`; passing it is defensive, so that buf never reads the baseline's own configuration, and no
    // test here can tell the two apart.)
    let repo = released(&[
        (S, &schema("int32")),
        AGAINST_V1,
        (
            "buf.yaml",
            "version: v2
deps:
  - buf.build/acme/none
",
        ),
    ]);
    write(repo.path(), &[(S, &schema("string"))]);
    commit(repo.path(), "retype");
    let (code, out, err) = run_json(repo.path());
    assert_eq!(
        code, 0,
        "{out}
{err}"
    );
    assert_eq!(
        findings(&out).len(),
        1,
        "{out}
{err}"
    );
}

/// A schema of package `package` whose message `Foo` has a field `age` of type `age`.
fn pkg(package: &str, age: &str) -> String {
    format!(
        "syntax = \"proto3\";\n\npackage {package};\n\nmessage Foo {{\n  string name = 1;\n  {age} age = 2;\n}}\n"
    )
}

const WORK_A: (&str, &str) = ("buf.work.yaml", "version: v1\ndirectories:\n  - a\n");
const WORK_AB: (&str, &str) = ("buf.work.yaml", "version: v1\ndirectories:\n  - a\n  - b\n");

#[test]
fn a_changed_schema_outside_every_module_is_a_gap_not_a_clean_pass() {
    needs_tool!();
    // the work file names only module `a`; the schema that changed is in `b`, which buf is never given
    let repo = released(&[
        WORK_A,
        AGAINST_V1,
        ("a/x/s.proto", &pkg("a.x", "int32")),
        ("b/y/s.proto", &pkg("b.y", "int32")),
    ]);
    write(repo.path(), &[("b/y/s.proto", &pkg("b.y", "string"))]);
    commit(repo.path(), "retype outside the modules");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("not below a buf module"))
        .stderr(predicate::str::contains("check error").not());
}

#[test]
fn a_project_package_called_target_is_judged_like_any_other() {
    needs_tool!();
    let repo = released(&[
        AGAINST_V1,
        ("target/v1/t.proto", &pkg("target.v1", "int32")),
        ("a/m.proto", &pkg("a.v1", "int32")),
    ]);
    write(
        repo.path(),
        &[
            ("target/v1/t.proto", &pkg("target.v1", "string")),
            (
                "a/m.proto",
                &pkg("a.v1", "int32").replace("}\n", "  bool b = 3;\n}\n"),
            ),
        ],
    );
    commit(repo.path(), "retype in target/");
    let (_, out, err) = run_json(repo.path());
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], "target/v1/t.proto");
}

#[test]
fn a_module_added_since_the_baseline_does_not_stop_the_comparison_of_the_others() {
    needs_tool!();
    let repo = released(&[WORK_A, AGAINST_V1, ("a/x/s.proto", &pkg("a.x", "int32"))]);
    write(
        repo.path(),
        &[
            WORK_AB,
            ("a/x/s.proto", &pkg("a.x", "string")),
            ("b/y/s.proto", &pkg("b.y", "int32")),
        ],
    );
    commit(repo.path(), "retype a, add module b");
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert_eq!(all[0]["location"]["file"], "a/x/s.proto");
}

#[test]
fn a_module_removed_since_the_baseline_cannot_be_compared_and_is_never_clean() {
    needs_tool!();
    let repo = released(&[
        WORK_AB,
        AGAINST_V1,
        ("a/x/s.proto", &pkg("a.x", "int32")),
        ("b/y/s.proto", &pkg("b.y", "int32")),
    ]);
    git(repo.path(), &["rm", "-rq", "b"]);
    // a compatible change in a, so buf runs and says nothing: the removed module is why the answer is not clean
    write(
        repo.path(),
        &[(
            "a/x/s.proto",
            &pkg("a.x", "int32").replace("}\n", "  bool b = 3;\n}\n"),
        )],
    );
    commit(repo.path(), "drop module b");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains(
            "had schema in the baseline and have none now",
        ))
        .stderr(predicate::str::contains("check error").not());
    // beside a real break in a, the finding is reported and says what was not judged
    write(repo.path(), &[("a/x/s.proto", &pkg("a.x", "string"))]);
    commit(repo.path(), "retype a");
    let (_, out, err) = run_json(repo.path());
    let all = findings(&out);
    assert_eq!(all.len(), 1, "{out}\n{err}");
    assert!(
        all[0]["detail"]
            .as_str()
            .unwrap()
            .contains("had schema in the baseline"),
        "{out}"
    );
}

#[test]
fn removing_every_schema_is_a_gap_with_the_right_reason() {
    needs_tool!();
    let repo = released(&[(S, &schema("int32")), AGAINST_V1]);
    git(repo.path(), &["rm", "-q", S]);
    commit(repo.path(), "remove the schema");
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("every schema was removed"))
        .stderr(predicate::str::contains("holds no .proto file").not())
        .stderr(predicate::str::contains("check error").not());
}

#[test]
fn a_schema_renamed_away_from_proto_is_a_deletion_not_a_missed_change() {
    needs_tool!();
    let repo = released(&[
        AGAINST_V1,
        ("a/m.proto", &pkg("a.v1", "int32")),
        ("a/n.proto", &pkg("a.w1", "int32")),
    ]);
    git(repo.path(), &["mv", "a/n.proto", "a/n.txt"]);
    commit(repo.path(), "rename away from .proto");
    let (code, out, err) = run_json(repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let subjects: Vec<String> = findings(&out)
        .iter()
        .map(|f| f["subject"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(subjects, ["FILE_NO_DELETE"], "{out}\n{err}");
    run_denying(repo.path()).code(1);
}

#[test]
fn a_stray_schema_that_conflicts_with_the_projects_makes_the_rule_a_gap_not_a_clean_pass() {
    needs_tool!();
    let repo = released(&[
        (S, &schema("int32")),
        AGAINST_V1,
        (".gitignore", "node_modules/\n"),
    ]);
    write(
        repo.path(),
        &[(
            S,
            &schema("int32").replace("}\n", "  bool active = 3;\n}\n"),
        )],
    );
    commit(repo.path(), "add a field");
    // an ignored directory whose schema declares the same message: buf cannot build the tree
    write(
        repo.path(),
        &[("node_modules/x/a/dup.proto", &schema("int32"))],
    );
    run_denying(repo.path())
        .code(0)
        .stderr(predicate::str::contains("API-006 not run"))
        .stderr(predicate::str::contains("check error").not());
}
