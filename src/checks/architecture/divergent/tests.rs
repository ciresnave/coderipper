// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn commit(id: &str, files: &[&str]) -> Commit {
    Commit {
        id: id.into(),
        files: files.iter().map(|s| s.to_string()).collect(),
    }
}

fn head(files: &[&str]) -> BTreeSet<String> {
    files.iter().map(|s| s.to_string()).collect()
}

fn params() -> Params {
    Params {
        window: 500,
        min_commits: 2,
        max_files: 25,
    }
}

#[test]
fn a_file_changed_with_two_disjoint_sets_of_files_is_divergent() {
    let commits = [
        commit("c1", &["src/m.rs", "src/a.rs"]),
        commit("c2", &["src/m.rs", "src/a.rs", "src/a2.rs"]),
        commit("c3", &["src/m.rs", "src/b.rs"]),
        commit("c4", &["src/m.rs", "src/b.rs"]),
        commit("c5", &["src/x.rs", "src/y.rs"]),
    ];
    let (found, tally) = analyse(
        &commits,
        &head(&["src/m.rs", "src/a.rs", "src/a2.rs", "src/b.rs"]),
        params(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, "src/m.rs");
    assert_eq!(found[0].groups.len(), 2);
    assert_eq!(found[0].groups[0].files, head(&["src/a.rs", "src/a2.rs"]));
    assert_eq!(found[0].groups[1].files, head(&["src/b.rs"]));
    // positive control: the commits and the files were read
    assert_eq!((tally.read, tally.bulk, tally.judged), (5, 0, 4));
}

#[test]
fn a_file_whose_commits_share_a_file_is_one_group_and_is_silent() {
    let commits = [
        commit("c1", &["src/m.rs", "src/a.rs"]),
        commit("c2", &["src/m.rs", "src/a.rs", "src/b.rs"]),
        commit("c3", &["src/m.rs", "src/b.rs"]),
        commit("c4", &["src/m.rs", "src/b.rs"]),
    ];
    let (found, tally) = analyse(
        &commits,
        &head(&["src/m.rs", "src/a.rs", "src/b.rs"]),
        params(),
    );
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(tally.judged, 3, "the file was judged");
}

#[test]
fn a_group_below_the_minimum_does_not_count() {
    let commits = [
        commit("c1", &["src/m.rs", "src/a.rs"]),
        commit("c2", &["src/m.rs", "src/a.rs"]),
        commit("c3", &["src/m.rs", "src/b.rs"]),
    ];
    let h = head(&["src/m.rs", "src/a.rs", "src/b.rs"]);
    let (found, _) = analyse(&commits, &h, params());
    assert!(
        found.is_empty(),
        "one counted group of two, one commit left over: {found:?}"
    );
    let one = Params {
        min_commits: 1,
        ..params()
    };
    let (found, _) = analyse(&commits, &h, one);
    assert_eq!(
        found.len(),
        1,
        "positive control: with a minimum of one the same history is divergent"
    );
    assert_eq!(found[0].file, "src/m.rs");
}

#[test]
fn a_bulk_commit_is_set_aside_and_does_not_join_the_groups() {
    let many: Vec<String> = (0..30).map(|i| format!("src/f{i}.rs")).collect();
    let mut bulk_files: Vec<&str> = many.iter().map(String::as_str).collect();
    bulk_files.push("src/m.rs");
    bulk_files.push("src/a.rs");
    bulk_files.push("src/b.rs");
    let commits = [
        commit("c1", &["src/m.rs", "src/a.rs"]),
        commit("c2", &["src/m.rs", "src/a.rs"]),
        commit("c3", &["src/m.rs", "src/b.rs"]),
        commit("c4", &["src/m.rs", "src/b.rs"]),
        commit("bulk", &bulk_files),
    ];
    let h = head(&bulk_files);
    let (found, tally) = analyse(&commits, &h, params());
    assert_eq!(tally.bulk, 1);
    assert_eq!(
        found.len(),
        1,
        "without the bulk commit the groups stay apart: {found:?}"
    );
    let kept = Params {
        max_files: 100,
        ..params()
    };
    let (found, _) = analyse(&commits, &h, kept);
    assert!(
        found.is_empty(),
        "positive control: kept, the bulk commit joins the two groups: {found:?}"
    );
}

#[test]
fn a_file_everything_changes_with_does_not_join_the_groups() {
    let mut commits = vec![
        commit("c1", &["src/m.rs", "src/a.rs", "CHANGELOG.md"]),
        commit("c2", &["src/m.rs", "src/a.rs", "CHANGELOG.md"]),
        commit("c3", &["src/m.rs", "src/b.rs", "CHANGELOG.md"]),
        commit("c4", &["src/m.rs", "src/b.rs", "CHANGELOG.md"]),
    ];
    let h = head(&["src/m.rs", "src/a.rs", "src/b.rs"]);
    for i in 0..2 {
        commits.push(commit(&format!("o{i}"), &["CHANGELOG.md"]));
    }
    let (found, tally) = analyse(&commits, &h, params());
    assert_eq!(tally.ubiquitous, ["CHANGELOG.md"]);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_file_no_longer_at_head_is_not_judged() {
    let commits = [
        commit("c1", &["src/gone.rs", "src/a.rs"]),
        commit("c2", &["src/gone.rs", "src/a.rs"]),
        commit("c3", &["src/gone.rs", "src/b.rs"]),
        commit("c4", &["src/gone.rs", "src/b.rs"]),
    ];
    let (found, _) = analyse(&commits, &head(&["src/a.rs", "src/b.rs"]), params());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn which_paths_are_judged() {
    for yes in [
        "src/a.rs",
        "pkg/svc/handler.py",
        "web/app.tsx",
        "src/checks/x.rs",
    ] {
        assert!(judged_path(yes), "{yes}");
    }
    for no in [
        "src/lib.rs",
        "src/main.rs",
        "src/checks/mod.rs",
        "build.rs",
        "tests/a.rs",
        "src/x/tests.rs",
        "src/a_test.rs",
        "pkg/__init__.py",
        "README.md",
        "Cargo.toml",
        "examples/demo.rs",
        "web/index.ts",
        "web/app.spec.ts",
    ] {
        assert!(!judged_path(no), "{no}");
    }
}

#[test]
fn the_log_is_parsed_into_commits_and_files() {
    let text = "\u{1}aaa\nsrc/a.rs\nsrc/b.rs\n\u{1}bbb\nsrc/c.rs\n";
    let c = parse_log(text);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].id, "aaa");
    assert_eq!(c[0].files, head(&["src/a.rs", "src/b.rs"]));
    assert_eq!(c[1].files, head(&["src/c.rs"]));
}

#[test]
fn the_finding_names_the_groups_and_is_info_at_low_confidence() {
    let commits = [
        commit("c1c1c1c1c1", &["src/m.rs", "src/a.rs"]),
        commit("c2c2c2c2c2", &["src/m.rs", "src/a.rs"]),
        commit("c3c3c3c3c3", &["src/m.rs", "src/b.rs"]),
        commit("c4c4c4c4c4", &["src/m.rs", "src/b.rs"]),
    ];
    let (found, _) = analyse(
        &commits,
        &head(&["src/m.rs", "src/a.rs", "src/b.rs"]),
        params(),
    );
    let f = judge(&found, "p", params());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].confidence, Confidence::Low);
    assert_eq!(f[0].subject.as_deref(), Some("src/m.rs"));
    // the host rejects a summary that reads as an absence claim without a positive control
    assert!(f[0].clone().validate().is_ok(), "{}", f[0].summary);
    assert!(f[0].detail.contains("c1c1c1c1"), "{}", f[0].detail);
    assert!(f[0].detail.contains("src/b.rs"), "{}", f[0].detail);
}

#[test]
fn a_document_edited_with_the_file_is_not_a_second_reason_to_change() {
    let commits = [
        commit("c1", &["src/m.rs", "src/a.rs"]),
        commit("c2", &["src/m.rs", "src/a.rs"]),
        commit("c3", &["src/m.rs", "README.md", "docs/guide.md"]),
        commit("c4", &["src/m.rs", "README.md", "docs/guide.md"]),
    ];
    let (found, tally) = analyse(&commits, &head(&["src/m.rs", "src/a.rs"]), params());
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(tally.judged, 2, "the file was judged");
}
