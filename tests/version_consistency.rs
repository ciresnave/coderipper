//! `version-consistency` against real `cargo metadata`: each rule has a case that must report and a
//! control that must stay quiet. No git and no build are involved; the check reads manifests.

use coderipper::check::{Check, CheckContext, Tier};
use coderipper::checks::VersionConsistencyCheck;
use coderipper::finding::Finding;
use std::path::Path;

fn write(root: &Path, files: &[(&str, String)]) {
    for (name, contents) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

fn member(name: &str, version: &str) -> (String, String) {
    (
        format!("{name}/Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
}

/// A virtual workspace whose members are `(name, version)`, each with an empty lib.
fn workspace(members: &[(&str, &str)], extra_root: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let names: Vec<String> = members.iter().map(|(n, _)| format!("\"{n}\"")).collect();
    let mut files = vec![(
        "Cargo.toml".to_string(),
        format!(
            "[workspace]\nmembers = [{}]\nresolver = \"2\"\n{extra_root}",
            names.join(", ")
        ),
    )];
    for (name, version) in members {
        let (path, manifest) = member(name, version);
        files.push((path, manifest));
        files.push((format!("{name}/src/lib.rs"), String::new()));
    }
    let borrowed: Vec<(&str, String)> =
        files.iter().map(|(n, c)| (n.as_str(), c.clone())).collect();
    write(tmp.path(), &borrowed);
    tmp
}

fn run(project: &Path) -> anyhow::Result<Vec<Finding>> {
    VersionConsistencyCheck.run(&CheckContext::new(project.to_path_buf()))
}

fn subjects(findings: &[Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

#[test]
fn packages_at_one_version_are_clean_the_negative_control() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.2")], "");
    assert!(run(ws.path()).unwrap().is_empty());
}

#[test]
fn one_package_at_another_version_is_reported_the_positive_control() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    let found = run(ws.path()).unwrap();
    assert_eq!(subjects(&found), vec!["c"]);
    let f = &found[0];
    assert_eq!(f.severity, coderipper::finding::Severity::High);
    assert_eq!(f.confidence, coderipper::finding::Confidence::High);
    assert_eq!(f.location.as_ref().unwrap().file, "c/Cargo.toml");
    assert!(
        f.summary.contains("0.4.1") && f.summary.contains("0.4.2"),
        "{}",
        f.summary
    );
    assert!(f.clone().validate().is_ok());
}

#[test]
fn the_workspace_package_version_is_the_project_version() {
    // two members inherit 1.0.0, one pins 2.0.0: the pinned one is the outlier even though the
    // inheriting ones are only two of three
    let tmp = tempfile::tempdir().unwrap();
    let inheriting = |n: &str| {
        (
            format!("{n}/Cargo.toml"),
            format!("[package]\nname = \"{n}\"\nversion.workspace = true\nedition = \"2021\"\n"),
        )
    };
    let pinned = member("pinned", "2.0.0");
    let (ia, ib) = (inheriting("a"), inheriting("b"));
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\", \"pinned\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"1.0.0\"\n"
                    .to_string(),
            ),
            (ia.0.as_str(), ia.1.clone()),
            ("a/src/lib.rs", String::new()),
            (ib.0.as_str(), ib.1.clone()),
            ("b/src/lib.rs", String::new()),
            (pinned.0.as_str(), pinned.1.clone()),
            ("pinned/src/lib.rs", String::new()),
        ],
    );
    assert_eq!(subjects(&run(tmp.path()).unwrap()), vec!["pinned"]);
}

#[test]
fn without_a_workspace_version_the_majority_decides_and_a_tie_goes_to_the_highest() {
    let ws = workspace(&[("a", "0.1.0"), ("b", "0.1.0"), ("c", "0.2.0")], "");
    assert_eq!(subjects(&run(ws.path()).unwrap()), vec!["c"]);
    let tie = workspace(&[("a", "0.1.0"), ("b", "0.2.0")], "");
    assert_eq!(subjects(&run(tie.path()).unwrap()), vec!["a"]);
}

#[test]
fn a_single_package_project_is_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let (path, manifest) = member("solo", "0.9.9");
    write(
        tmp.path(),
        &[
            (path.as_str(), manifest),
            ("solo/src/lib.rs", String::new()),
        ],
    );
    assert!(run(&tmp.path().join("solo")).unwrap().is_empty());
}

#[test]
fn a_member_directory_is_told_to_run_at_the_workspace_root_instead_of_guessing() {
    // Review finding: the verdict used to depend on which directory was named, because
    // `.coderipper.toml` (tracks, allow entries) is read from the directory passed while cargo shows the
    // whole workspace from any member. The comparison is about the whole workspace, so only the
    // workspace root is analyzed; a member gets one Info finding, not a verdict, and not an error.
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    let found = run(&ws.path().join("a")).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity, coderipper::finding::Severity::Info);
    assert!(
        found[0].summary.contains("workspace root"),
        "{}",
        found[0].summary
    );
    assert!(found[0].positive_control.is_some());
    assert!(found[0].clone().validate().is_ok());
    // ...and the workspace root itself still gets the real verdict
    assert_eq!(subjects(&run(ws.path()).unwrap()), vec!["c"]);
}

#[test]
fn packages_without_a_version_are_left_out_of_the_comparison() {
    // Review finding: since Cargo 1.75 `version` may be omitted; cargo reports 0.0.0, and three such
    // crates outvoted the two real ones, so the real ones were flagged.
    let tmp = tempfile::tempdir().unwrap();
    let unversioned = |n: &str| {
        (
            format!("{n}/Cargo.toml"),
            format!("[package]\nname = \"{n}\"\nedition = \"2021\"\n"),
        )
    };
    let (a, b) = (member("a", "1.0.0"), member("b", "1.0.0"));
    let (x1, x2, x3) = (unversioned("x1"), unversioned("x2"), unversioned("x3"));
    write(
        tmp.path(),
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\", \"x1\", \"x2\", \"x3\"]\nresolver = \"2\"\n"
                    .to_string(),
            ),
            (a.0.as_str(), a.1.clone()),
            ("a/src/lib.rs", String::new()),
            (b.0.as_str(), b.1.clone()),
            ("b/src/lib.rs", String::new()),
            (x1.0.as_str(), x1.1.clone()),
            ("x1/src/lib.rs", String::new()),
            (x2.0.as_str(), x2.1.clone()),
            ("x2/src/lib.rs", String::new()),
            (x3.0.as_str(), x3.1.clone()),
            ("x3/src/lib.rs", String::new()),
        ],
    );
    assert!(run(tmp.path()).unwrap().is_empty());
}

// ---- tracking exceptions ----

fn tracked_fixture(emit_version: &str, reference_version: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "proj/Cargo.toml",
                "[workspace]\nmembers = [\"core\", \"core-two\", \"emit\"]\nresolver = \"2\"\n".to_string(),
            ),
            member_at("proj/core", "0.3.0"),
            ("proj/core/src/lib.rs", String::new()),
            member_at("proj/core-two", "0.3.0"),
            ("proj/core-two/src/lib.rs", String::new()),
            member_at("proj/emit", emit_version),
            ("proj/emit/src/lib.rs", String::new()),
            (
                "other/Cargo.toml",
                format!("[package]\nname = \"other\"\nversion = \"{reference_version}\"\nedition = \"2021\"\n"),
            ),
            ("other/src/lib.rs", String::new()),
            (
                "proj/.coderipper.toml",
                "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"emits for other\"\n"
                    .to_string(),
            ),
        ],
    );
    tmp
}

fn member_at(dir: &str, version: &str) -> (&'static str, String) {
    let name = dir.rsplit('/').next().unwrap();
    let path: &'static str = Box::leak(format!("{dir}/Cargo.toml").into_boxed_str());
    (
        path,
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
}

#[test]
fn a_tracking_package_that_matches_its_reference_is_clean_and_not_an_outlier() {
    // emit is at 7.0.0 and everything else at 0.3.0: without the exception it would be flagged
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    assert!(run(&tmp.path().join("proj")).unwrap().is_empty());
}

#[test]
fn a_tracking_package_that_drifted_from_its_reference_is_reported() {
    let tmp = tracked_fixture("7.0.0", "7.1.0");
    let found = run(&tmp.path().join("proj")).unwrap();
    assert_eq!(subjects(&found), vec!["emit"]);
    assert!(
        found[0].summary.contains("7.1.0") && found[0].summary.contains("7.0.0"),
        "{}",
        found[0].summary
    );
    assert!(
        found[0].detail.contains("emits for other"),
        "the reason must be shown: {}",
        found[0].detail
    );
}

#[test]
fn an_unreadable_reference_is_an_error_never_a_pass() {
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    std::fs::remove_file(tmp.path().join("other/Cargo.toml")).unwrap();
    let err = run(&tmp.path().join("proj")).unwrap_err().to_string();
    assert!(err.contains("other") && err.contains("Cargo.toml"), "{err}");
}

#[test]
fn a_tracks_entry_for_a_package_that_does_not_exist_is_an_error() {
    let ws = workspace(&[("a", "0.1.0"), ("b", "0.1.0")], "");
    std::fs::write(
        ws.path().join(".coderipper.toml"),
        "[[tracks]]\npackage = \"ghost\"\nmanifest = \"Cargo.toml\"\nreason = \"typo\"\n",
    )
    .unwrap();
    let err = run(ws.path()).unwrap_err().to_string();
    assert!(
        err.contains("ghost") && err.contains("not a package"),
        "{err}"
    );
}

// ---- through the host: the ordinary allowlist ----

#[test]
fn an_allowlist_entry_silences_one_package_and_goes_stale_when_the_versions_agree() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    std::fs::write(
        ws.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"version-consistency\"\nfile = \"c/Cargo.toml\"\nsymbol = \"c\"\nreason = \"c is frozen for a release\"\n",
    )
    .unwrap();
    let ctx = || CheckContext::new(ws.path().to_path_buf());
    let quiet = coderipper::run_checks(&ctx(), Tier::Fast, Some("version-consistency"));
    assert!(quiet.errors.is_empty(), "{:?}", quiet.errors);
    assert!(quiet.findings.is_empty(), "{:?}", quiet.findings);

    // fix the version: the entry now suppresses nothing and says so
    std::fs::write(
        ws.path().join("c/Cargo.toml"),
        "[package]\nname = \"c\"\nversion = \"0.4.2\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let stale = coderipper::run_checks(&ctx(), Tier::Fast, Some("version-consistency"));
    assert_eq!(stale.findings.len(), 1, "{:?}", stale.findings);
    assert_eq!(stale.findings[0].check_id, "allowlist");
}

#[test]
fn a_directory_that_is_not_a_cargo_project_is_an_error_not_clean() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(run(tmp.path()).is_err());
}

// ---- review fixes: how a tracking reference is resolved ----

#[test]
fn a_reference_that_inherits_its_version_from_its_own_workspace_is_resolved() {
    // Review finding: a reference manifest saying `version.workspace = true` (the version lives in
    // ANOTHER file, the reference project's workspace root) was reported as "declares no version".
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    write(
        tmp.path(),
        &[
            (
                "refws/Cargo.toml",
                "[workspace]\nmembers = [\"emitref\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"7.0.0\"\n"
                    .to_string(),
            ),
            (
                "refws/emitref/Cargo.toml",
                "[package]\nname = \"emitref\"\nversion.workspace = true\nedition = \"2021\"\n".to_string(),
            ),
            ("refws/emitref/src/lib.rs", String::new()),
            (
                "proj/.coderipper.toml",
                "[[tracks]]\npackage = \"emit\"\nmanifest = \"../refws/emitref/Cargo.toml\"\nreason = \"works with refws\"\n"
                    .to_string(),
            ),
        ],
    );
    assert!(run(&tmp.path().join("proj")).unwrap().is_empty());
    // and it still notices drift against the INHERITED value
    write(tmp.path(), &[member_at("proj/emit", "7.1.0")]);
    assert_eq!(
        subjects(&run(&tmp.path().join("proj")).unwrap()),
        vec!["emit"]
    );
}

#[test]
fn a_reference_that_is_a_virtual_workspace_means_that_workspaces_one_version() {
    // The README's own example points at a project's workspace manifest.
    let tmp = tracked_fixture("0.14.1", "0.14.1");
    write(
        tmp.path(),
        &[
            (
                "other/Cargo.toml",
                "[workspace]\nmembers = [\"m1\", \"m2\"]\nresolver = \"2\"\n".to_string(),
            ),
            member_at("other/m1", "0.14.1"),
            ("other/m1/src/lib.rs", String::new()),
            member_at("other/m2", "0.14.1"),
            ("other/m2/src/lib.rs", String::new()),
        ],
    );
    assert!(run(&tmp.path().join("proj")).unwrap().is_empty());

    // a referenced workspace that is itself inconsistent has no single version to track
    write(tmp.path(), &[member_at("other/m2", "0.15.0")]);
    let err = run(&tmp.path().join("proj")).unwrap_err().to_string();
    assert!(err.contains("no single version"), "{err}");
}

#[test]
fn two_tracks_entries_for_one_package_are_an_error_not_last_one_wins() {
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    std::fs::write(
        tmp.path().join("proj/.coderipper.toml"),
        "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"one\"\n\n[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"two\"\n",
    )
    .unwrap();
    let err = run(&tmp.path().join("proj")).unwrap_err().to_string();
    assert!(
        err.contains("emit") && err.contains("more than once"),
        "{err}"
    );
}

#[test]
fn a_tracks_entry_with_an_unknown_field_is_rejected() {
    // a stray `version = "9.9.9"` inside an entry used to be ignored silently
    let tmp = tracked_fixture("7.0.0", "7.0.0");
    std::fs::write(
        tmp.path().join("proj/.coderipper.toml"),
        "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"x\"\nversion = \"9.9.9\"\n",
    )
    .unwrap();
    assert!(run(&tmp.path().join("proj")).is_err());
}

#[test]
fn a_member_outside_the_workspace_directory_gets_a_relative_path_not_an_absolute_one() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        &[
            (
                "ws/Cargo.toml",
                "[workspace]\nmembers = [\"../ext\", \"in\"]\nresolver = \"2\"\n".to_string(),
            ),
            member_at("ws/in", "0.2.0"),
            ("ws/in/src/lib.rs", String::new()),
            (
                "ext/Cargo.toml",
                "[package]\nname = \"ext\"\nversion = \"0.1.0\"\nedition = \"2021\"\nworkspace = \"../ws\"\n".to_string(),
            ),
            ("ext/src/lib.rs", String::new()),
        ],
    );
    let found = run(&tmp.path().join("ws")).unwrap();
    // two packages, a tie: the highest wins, so `ext` (0.1.0) is the outlier
    assert_eq!(subjects(&found), vec!["ext"]);
    assert_eq!(
        found[0].location.as_ref().unwrap().file,
        "../ext/Cargo.toml"
    );
}
