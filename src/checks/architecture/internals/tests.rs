// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::checks::rust_sources::SourceFile;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn reaches(files: &[SourceFile]) -> Vec<Reach> {
    reach_throughs(&modgraph::read(files).unwrap().unwrap())
}

fn crate_with(a_rs: &str, b_rs: &str, user: &str) -> Vec<SourceFile> {
    vec![
        file("src/lib.rs", "mod a;\nmod user;\n"),
        file("src/a.rs", a_rs),
        file("src/a/b.rs", b_rs),
        file("src/user.rs", user),
    ]
}

#[test]
fn a_deep_path_is_a_reach_through_when_the_owner_re_exports_the_item() {
    let r = reaches(&crate_with(
        "mod b;\npub use b::X;\n",
        "pub struct X;\n",
        "fn f() { let _ = crate::a::b::X; }\n",
    ));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].file, "src/user.rs");
    assert_eq!(r[0].written, "crate::a::b::X");
    assert_eq!(r[0].via, "crate::a::X");
    assert_eq!(r[0].line, 1);
}

#[test]
fn the_entry_point_path_and_a_path_with_no_re_export_are_not() {
    // the shorter path the owner offers
    assert!(reaches(&crate_with(
        "mod b;\npub use b::X;\n",
        "pub struct X;\n",
        "fn f() { let _ = crate::a::X; }\n"
    ))
    .is_empty());
    // no re-export: the owner never offered a shorter path, so nothing was bypassed
    assert!(reaches(&crate_with(
        "pub mod b;\n",
        "pub struct X;\n",
        "fn f() { let _ = crate::a::b::X; }\n"
    ))
    .is_empty());
}

#[test]
fn a_use_is_read_like_any_other_path_and_a_renamed_re_export_counts() {
    let r = reaches(&crate_with(
        "mod b;\npub use b::X as Y;\n",
        "pub struct X;\n",
        "use crate::a::b::X;\nfn f(_: X) {}\n",
    ));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].via, "crate::a::Y");
}

#[test]
fn a_glob_re_export_of_the_module_offers_every_item() {
    let r = reaches(&crate_with(
        "mod b;\npub use b::*;\n",
        "pub struct X;\n",
        "fn f() { let _ = crate::a::b::X; }\n",
    ));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].via, "crate::a::X");
}

#[test]
fn the_use_that_builds_the_re_export_and_paths_inside_the_subtree_are_not_reaches() {
    // `a` names `b::X` to re-export it; `a::c` (a sibling of b inside a) names `super::b::X`: both are inside a
    let files = vec![
        file("src/lib.rs", "mod a;\n"),
        file("src/a.rs", "mod b;\nmod c;\npub use b::X;\n"),
        file("src/a/b.rs", "pub struct X;\n"),
        file("src/a/c.rs", "fn f() { let _ = super::b::X; }\n"),
    ];
    assert!(reaches(&files).is_empty());
}

#[test]
fn test_items_are_not_read() {
    let files = vec![
        file(
            "src/lib.rs",
            "mod a;\n#[cfg(test)]\nmod t { fn f() { let _ = crate::a::b::X; } }\n",
        ),
        file("src/a.rs", "mod b;\npub use b::X;\n"),
        file("src/a/b.rs", "pub struct X;\n"),
    ];
    assert!(reaches(&files).is_empty());
}

#[test]
fn a_path_into_another_crate_is_not_a_reach() {
    let r = reaches(&crate_with(
        "mod b;\npub use b::X;\n",
        "pub struct X;\n",
        "fn f() { let _ = serde::a::b::X; }\n",
    ));
    assert!(r.is_empty(), "{r:?}");
}

#[test]
fn one_finding_per_file_with_the_count_the_first_line_and_the_shorter_path() {
    let reaches = vec![
        Reach {
            file: "src/user.rs".into(),
            line: 7,
            written: "crate::a::b::X".into(),
            via: "crate::a::X".into(),
        },
        Reach {
            file: "src/user.rs".into(),
            line: 9,
            written: "crate::a::b::Y".into(),
            via: "crate::a::Y".into(),
        },
        Reach {
            file: "src/other.rs".into(),
            line: 2,
            written: "crate::a::b::X".into(),
            via: "crate::a::X".into(),
        },
    ];
    let f = judge(&reaches, "p");
    assert_eq!(f.len(), 2);
    let user = f
        .iter()
        .find(|x| x.subject.as_deref() == Some("src/user.rs"))
        .unwrap();
    assert!(
        user.summary.contains("2 item(s)") && user.summary.contains("line 7"),
        "{}",
        user.summary
    );
    assert!(user
        .detail
        .contains("`crate::a::b::X` (offered as `crate::a::X`)"));
    assert_eq!(user.severity, Severity::Low);
    assert_eq!(user.confidence, Confidence::Medium);
    assert_eq!(user.location.as_ref().unwrap().line, Some(7));
}

#[test]
fn a_package_with_no_root_file_is_noted_not_judged() {
    assert!(modgraph::read(&[file("src/a.rs", "pub fn f() {}\n")])
        .unwrap()
        .is_none());
}
