#![cfg(feature = "cli")]
//! What the exit-3 message says about WHY a crate's build failed, and where each finding is, in the text output.
//!
//! Found by the 2026-10-11 dogfood run: a crate whose build script could not find OpenSSL ended with "build reported real compiler
//! error(s) (or failed without any diagnostic)" and nothing else, and eight identical "actions/checkout@v4 is not pinned" lines
//! gave no file or line (the JSON had them).

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const BUILD_RS_MANIFEST: &str =
    "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n";

fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

fn run(args: &[&str], project: &std::path::Path) -> (i32, String, String) {
    let out = coderipper()
        .args(args)
        .arg("--project")
        .arg(project)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_build_script_that_cannot_find_what_it_needs_says_so_in_the_error() {
    let repo = common::git_repo_with(&[
        ("Cargo.toml", BUILD_RS_MANIFEST),
        (
            "build.rs",
            "fn main() { panic!(\"Could not find directory of OpenSSL installation\"); }\n",
        ),
        ("src/lib.rs", "pub fn f() {}\n"),
    ]);
    for check in ["reachability", "unused-parameters", "unused-return-values"] {
        let (code, _out, err) = run(&["check", check], repo.path());
        assert_eq!(code, 3, "{check}: {err}");
        assert!(
            err.contains("Cause:") && err.contains("failed to run custom build command"),
            "{check}: the message must name cargo's own error: {err}"
        );
        assert!(
            err.contains("Could not find directory of OpenSSL installation"),
            "{check}: the message must carry what the build script said: {err}"
        );
    }
}

#[test]
fn a_compile_error_is_named_with_its_code_and_place() {
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn main() {\n    let n: i32 = missing_name;\n}\n",
        ),
    ]);
    let (code, _out, err) = run(&["check", "unused-return-values"], repo.path());
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("Cause: error[E0425]"), "{err}");
    assert!(err.contains("missing_name"), "{err}");
    assert!(err.contains("src/main.rs:2"), "{err}");
}

#[test]
fn a_finding_names_its_file_and_line_in_the_text_output() {
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            "fn f() -> i32 { 1 }\nfn main() {\n    f();\n}\n",
        ),
    ]);
    let (code, out, err) = run(
        &["check", "unused-return-values", "--deny", "none"],
        repo.path(),
    );
    assert_eq!(code, 0, "{err}");
    let line = out
        .lines()
        .find(|l| l.contains("(unused-return-values)"))
        .unwrap_or_else(|| panic!("no finding line: {out}"));
    assert!(line.contains(" at src/main.rs:1 "), "{line}");
    // a line still ends in its check id, as scripts and the workspace tests read it
    assert!(line.ends_with("(unused-return-values)"), "{line}");
}

#[test]
fn a_waived_finding_names_its_place_too() {
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() {\n    f();\n}\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nreason = \"fixture\"\n",
        ),
    ]);
    let (code, out, err) = run(&["check", "unused-return-values"], repo.path());
    assert_eq!(code, 0, "{out}\n{err}");
    let line = out
        .lines()
        .find(|l| l.starts_with("[waived/"))
        .unwrap_or_else(|| panic!("no waived line: {out}"));
    assert!(line.contains(" at src/main.rs:1 "), "{line}");
}

/// A fake secret, assembled at run time: this repository's own CI runs a secret scanner over it.
fn fake_token() -> String {
    ["ghp_", &"aB3dE5fG7h".repeat(4)].concat()
}

#[test]
fn redact_withholds_what_a_failed_build_script_printed_and_names_the_failure() {
    let token = fake_token();
    let build_rs = format!(
        "fn main() {{ println!(\"cargo:warning=TOKEN={token}\"); eprintln!(\"login https://bot:{token}@registry.example/\"); \
         std::process::exit(7); }}\n"
    );
    let repo = common::git_repo_with(&[
        ("Cargo.toml", BUILD_RS_MANIFEST),
        ("build.rs", &build_rs),
        ("src/lib.rs", "pub fn f() {}\n"),
    ]);
    // the control: without the flag the script's own words are what a developer sees
    let (code, _out, err) = run(&["check", "reachability"], repo.path());
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("registry.example"), "off: {err}");

    let (code, out, err) = run(&["check", "reachability", "--redact"], repo.path());
    assert_eq!(code, 3, "redaction must not change the exit code: {err}");
    assert!(
        err.contains(
            "build script of fixture 0.1.0 failed (exit status 7); its output is withheld"
        ),
        "{err}"
    );
    for text in [&out, &err] {
        assert!(
            !text.contains(&token),
            "the token reached the output: {text}"
        );
        assert!(
            !text.contains("aB3dE5fG7h"),
            "part of the token reached the output: {text}"
        );
        assert!(
            !text.contains("registry.example"),
            "the script's words reached the output: {text}"
        );
    }
}

#[test]
fn redact_removes_a_secret_a_compiler_message_quotes_and_keeps_the_error() {
    let token = fake_token();
    let lib_rs = format!("compile_error!(\"set API_KEY={token} first\");\n");
    let repo = common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", &lib_rs)]);
    let (code, _out, err) = run(&["check", "unused-return-values", "--redact"], repo.path());
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("Cause:"), "{err}");
    assert!(err.contains("set API_KEY=<redacted>"), "{err}");
    assert!(
        !err.contains(&token) && !err.contains("aB3dE5fG7h"),
        "{err}"
    );
}
