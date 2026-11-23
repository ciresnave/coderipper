// SPDX-License-Identifier: MIT OR Apache-2.0
//! `SUP-016` with `[sup016] gate = true`: the contract of the portfolio's `spdx_gate.py`, checked against what that script reported.
//!
//! DIFFERENTIAL RECORD: the repository below was built twice, from the same seeded defects, once as a clone of `ciresnave/consent` at
//! b5b8c41 (37 tracked source files, `spdx_gate.py` exit 0 on it unseeded) and run through `.github/spdx_gate.py`, and once through
//! this rule. The script reported MISSING for three files, DIFFERENT for one, one UNCOVERED EXTENSION and one COPYRIGHT NOTICE; the rule
//! reports the same three, one, one and one (the paths asserted here), and reports none of them unseeded. The script's own prose names
//! five of its files as expected copyright matches; the rule takes that list from `copyright_expected`.

use assert_cmd::Command;
use std::path::Path;
use std::process::Command as Git;

const STAMPED: &str = "// SPDX-License-Identifier: MIT OR Apache-2.0\nfn f() {}\n";

fn git(dir: &Path, args: &[&str]) {
    let out = Git::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository of 24 stamped `.rs` files plus `extra`, with the gate on, committed.
fn repo(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    write(root, "LICENSE-MIT", "MIT License\n");
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense = \"MIT OR Apache-2.0\"\n",
    );
    write(
        root,
        ".coderipper.toml",
        "[sup016]\ngate = true\nlicence = \"MIT OR Apache-2.0\"\nextensions = [\".rs\"]\nmin_files = 20\nheader_window = 10\n",
    );
    for i in 0..24 {
        write(root, &format!("src/m{i}.rs"), STAMPED);
    }
    for (rel, text) in extra {
        write(root, rel, text);
    }
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    );
    dir
}

fn gate_findings(root: &Path) -> (i32, Vec<serde_json::Value>) {
    let out = Command::cargo_bin("coderipper")
        .unwrap()
        .current_dir(root)
        .args(["check", "SUP-016", "--message-format", "json"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let found = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["subject"].as_str().is_some_and(|s| s.starts_with("gate")))
        .collect();
    (out.status.code().unwrap_or(-1), found)
}

#[test]
fn a_fully_stamped_tree_has_no_gate_finding() {
    let dir = repo(&[]);
    let (_, found) = gate_findings(dir.path());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_seeded_defects_the_script_reported_are_reported_at_the_same_places() {
    let late = format!(
        "{}// SPDX-License-Identifier: MIT OR Apache-2.0\n",
        "//\n".repeat(11)
    );
    let dir = repo(&[
        ("src/m0.rs", "fn no_header() {}\n"),
        ("src/m1.rs", "// SPDX-License-Identifier: GPL-3.0-only\n"),
        ("src/m2.rs", &late),
        ("UPPER.RS", "fn x() {}\n"),
        ("x.go", "package main\n"),
        ("notes.txt", "# Copyright 2020 Somebody\n"),
    ]);
    let (code, found) = gate_findings(dir.path());
    assert_ne!(code, 0, "an opted-in gate that finds something reads red");
    let by = |subject: &str| -> String {
        found
            .iter()
            .find(|f| f["subject"] == subject)
            .unwrap_or_else(|| panic!("no `{subject}` finding in {found:?}"))["detail"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let missing = by("gate: missing header");
    for path in ["UPPER.RS", "src/m0.rs", "src/m2.rs"] {
        assert!(missing.contains(path), "{path}: {missing}");
    }
    assert!(!missing.contains("src/m1.rs"), "{missing}");
    assert!(by("gate: different licence").contains("src/m1.rs (GPL-3.0-only)"));
    assert!(by("gate: copyright notice").contains("notes.txt"));
    let census = found
        .iter()
        .find(|f| f["subject"] == "gate: uncovered extension")
        .unwrap();
    assert!(census["summary"].as_str().unwrap().contains(".go"));
    assert_eq!(found.len(), 4, "{found:?}");
}

#[test]
fn without_the_opt_in_the_gate_reports_nothing_even_on_a_broken_tree() {
    let dir = repo(&[("src/m0.rs", "fn no_header() {}\n")]);
    write(dir.path(), ".coderipper.toml", "[sup016]\ngate = false\n");
    git(dir.path(), &["add", "-A"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "off",
        ],
    );
    let (_, found) = gate_findings(dir.path());
    assert!(found.is_empty(), "{found:?}");
}
