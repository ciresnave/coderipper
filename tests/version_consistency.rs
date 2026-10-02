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
    VersionConsistencyCheck.run(&CheckContext {
        project_root: project.to_path_buf(),
        portfolio_root: project.to_path_buf(),
    })
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
fn a_member_directory_is_enough_to_see_the_whole_workspace() {
    let ws = workspace(&[("a", "0.4.2"), ("b", "0.4.2"), ("c", "0.4.1")], "");
    // paths are relative to the WORKSPACE root even when the project is one member's directory
    let found = run(&ws.path().join("a")).unwrap();
    assert_eq!(subjects(&found), vec!["c"]);
    assert_eq!(found[0].location.as_ref().unwrap().file, "c/Cargo.toml");
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
    let ctx = || CheckContext {
        project_root: ws.path().to_path_buf(),
        portfolio_root: ws.path().to_path_buf(),
    };
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
