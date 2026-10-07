#![cfg(feature = "cli")]
//! What `--help`, `--version` and the usage line say. The help is the documentation a user reads first, and it is the
//! one place a doc comment placed above the wrong attribute silently describes the wrong option.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;

fn coderipper(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("coderipper")
        .unwrap()
        .env("CODERIPPER_CACHE", "off")
        .args(args)
        .output()
        .unwrap()
}

/// The help text of one option or argument: the lines from its heading to the next heading.
fn help_for(help: &str, heading: &str) -> String {
    let mut block = String::new();
    let mut inside = false;
    for line in help.lines() {
        let is_heading = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_start().starts_with(['-', '<'])
            || line.starts_with("      --");
        if is_heading {
            inside = line.contains(heading);
            continue;
        }
        if inside {
            block.push_str(line.trim());
            block.push(' ');
        }
    }
    block
}

fn help_of(subcommand: &str) -> String {
    let out = coderipper(&[subcommand, "--help"]);
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn each_option_of_fast_is_described_by_its_own_text() {
    let help = help_of("fast");
    assert!(
        help_for(&help, "--deny").contains("Exit 1 when a finding"),
        "{help}"
    );
    assert!(
        !help_for(&help, "--deny").contains("Root of the portfolio"),
        "the --deny text swallowed the portfolio-root text: {help}"
    );
    assert!(
        help_for(&help, "--portfolio-root").contains("Root of the portfolio"),
        "{help}"
    );
    assert!(
        help_for(&help, "--project").contains("Project to check"),
        "{help}"
    );
    assert!(
        help_for(&help, "--workspace").contains("every member"),
        "{help}"
    );
    assert!(
        help_for(&help, "--message-format").contains("json"),
        "{help}"
    );
}

#[test]
fn sweep_and_check_describe_their_options_too() {
    for sub in ["sweep", "check"] {
        let help = help_of(sub);
        assert!(
            help_for(&help, "--project").contains("Project to check"),
            "{sub}: {help}"
        );
        assert!(
            help_for(&help, "--portfolio-root").contains("Root of the portfolio"),
            "{sub}: {help}"
        );
        assert!(
            help_for(&help, "--deny").contains("Exit 1 when a finding"),
            "{sub}: {help}"
        );
    }
    assert!(
        help_for(&help_of("check"), "<ID>").contains("id of the check"),
        "{}",
        help_of("check")
    );
}

#[test]
fn the_version_is_printed() {
    let out = coderipper(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
    assert!(text.starts_with("coderipper "), "{text}");
}

#[test]
fn the_cargo_subcommand_prints_its_version_and_names_itself_in_usage() {
    let version = Command::cargo_bin("cargo-coderipper")
        .unwrap()
        .args(["coderipper", "--version"])
        .output()
        .unwrap();
    assert_eq!(version.status.code(), Some(0));
    assert!(String::from_utf8(version.stdout)
        .unwrap()
        .contains(env!("CARGO_PKG_VERSION")));

    let bad = Command::cargo_bin("cargo-coderipper")
        .unwrap()
        .args(["coderipper", "--no-such-flag"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(bad.stderr).unwrap();
    assert!(
        stderr.contains("cargo coderipper"),
        "usage should say how it is invoked: {stderr}"
    );
    assert!(!stderr.contains(".exe"), "{stderr}");
}

#[test]
fn a_windows_verbatim_path_prefix_never_reaches_the_output() {
    // `canonicalize` on Windows returns `\\?\C:\...`; it is noise in a message and in the JSON a CI job parses.
    let repo = tempfile::tempdir().unwrap(); // not a git repository: the run fails with a message naming the path
    let out = coderipper(&[
        "fast",
        "--message-format",
        "json",
        "--project",
        repo.path().to_str().unwrap(),
    ]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("not inside a git repository") || text.contains("git"),
        "{text}"
    );
    assert!(!text.contains(r"\\?\"), "{text}");
    assert!(!text.contains(r"\\\\?\\"), "{text}");
}
