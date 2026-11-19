// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fixtures for SUP-022 .. SUP-026, in the shapes of the licence-tools reference validator's own tests
//! (`tests/test_licence_manifest.py`), so the two tools can be compared on the same files.

use super::*;

const ENTRY: &str = r#"version = 1
[[annotations]]
path = "vendor/**"
SPDX-FileCopyrightText = "2022 RIKEN"
SPDX-License-Identifier = "MIT"
origin-url = "https://example.invalid/x"
origin-licence = "MIT"
origin-copyright = "RIKEN"
requires = ["vendor/NOTICE"]
"#;

fn no_requires() -> String {
    ENTRY.replace("requires = [\"vendor/NOTICE\"]\n", "")
}

const NOTICE: &str = "Copyright 2022 RIKEN\n";

/// Findings of one rule over an in-memory repository (`Noted` notes included); panics when the rule is not applicable.
fn run(which: Lm, files: &[(&str, &str)]) -> (Vec<Finding>, Vec<String>) {
    let mut all: Vec<(&str, &str)> = files.to_vec();
    all.sort();
    match judge(&RepoView::from_files(&all), which) {
        Verdict::Findings(f) => (f, Vec::new()),
        Verdict::Noted(f, n) => (f, n),
        Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
    }
}

fn found(which: Lm, files: &[(&str, &str)]) -> Vec<Finding> {
    run(which, files).0
}

fn all_rules(files: &[(&str, &str)]) -> Vec<(Lm, usize)> {
    [Lm::Parse, Lm::Notice, Lm::Marker, Lm::Stale, Lm::Compat]
        .into_iter()
        .map(|l| (l, found(l, files).len()))
        .collect()
}

#[test]
fn a_clean_manifest_is_silent_for_every_rule() {
    let files = [
        (MANIFEST, ENTRY),
        ("vendor/a.c", "x\n"),
        ("vendor/NOTICE", NOTICE),
    ];
    for (l, n) in all_rules(&files) {
        assert_eq!(n, 0, "{l:?}");
    }
}

#[test]
fn a_repository_with_no_manifest_is_not_applicable_to_all_five() {
    for l in [Lm::Parse, Lm::Notice, Lm::Marker, Lm::Stale, Lm::Compat] {
        let v = judge(
            &RepoView::from_files(&[("src/a.rs", "// ported from x\n")]),
            l,
        );
        assert!(matches!(v, Verdict::NotApplicable(_)), "{l:?}");
    }
}

#[test]
fn findings_name_both_ids() {
    for (l, files) in [
        (Lm::Parse, vec![(MANIFEST, "version = [")]),
        (Lm::Notice, vec![(MANIFEST, ENTRY), ("vendor/a.c", "x\n")]),
        (Lm::Marker, vec![(MANIFEST, "version = 1\n"), ("a.rs", "// vendored\n")]),
        (Lm::Stale, vec![(MANIFEST, ENTRY), ("src/a.rs", "x\n")]),
        (
            Lm::Compat,
            vec![(MANIFEST, "version = 1\n[[annotations]]\npath = \"a\"\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\norigin-url = \"u\"\norigin-licence = \"GPL-3.0-only\"\norigin-copyright = \"Y\"\nrequires = [\"a\"]\n"), ("a", "Copyright Y\n")],
        ),
    ] {
        let f = found(l, &files);
        assert!(!f.is_empty(), "{l:?}");
        let both = format!("{} ({})", l.rule(), l.lm());
        assert!(f.iter().all(|x| x.summary.contains(&both)), "{l:?}: {:?}", f[0].summary);
        assert!(f.iter().all(|x| x.severity == Severity::Low));
        f.into_iter().for_each(|x| {
            x.validate().expect("no absence word in the summary");
        });
    }
}

// ---- SUP-023 (LM-002) -------------------------------------------------------------------------------------------------

#[test]
fn a_listed_notice_that_is_not_tracked_is_the_positive_control() {
    let f = found(Lm::Notice, &[(MANIFEST, ENTRY), ("vendor/a.c", "x\n")]);
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("`vendor/NOTICE`"));
    // the same entry with the notice tracked is clean
    assert!(found(
        Lm::Notice,
        &[
            (MANIFEST, ENTRY),
            ("vendor/a.c", "x\n"),
            ("vendor/NOTICE", NOTICE)
        ]
    )
    .is_empty());
}

#[test]
fn the_riken_control_is_caught_without_requires_and_a_prose_credit_does_not_satisfy_it() {
    let m = no_requires();
    let base = [
        (MANIFEST, m.as_str()),
        ("vendor/a.c", "x\n"),
        ("LICENSES/MIT.txt", "MIT\n"),
    ];
    let f = found(Lm::Notice, &base);
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("`RIKEN`"), "{}", f[0].summary);
    // prose naming RIKEN-RCCS (Baracuda's ATTRIBUTION.md) is a credit, not a notice
    let mut prose = base.to_vec();
    prose.push((
        "vendor/ATTRIBUTION.md",
        "Ported from RIKEN-RCCS/accelerator_for_ozIMMU (MIT).\nRIKEN\n",
    ));
    assert_eq!(found(Lm::Notice, &prose).len(), 1);
    // the same file with a copyright line (each of the three marks) satisfies it
    for line in [
        "Copyright 2022 RIKEN",
        "(c) 2022 RIKEN",
        "\u{a9} 2022 RIKEN",
        "COPYRIGHT   2022\tRiKeN",
    ] {
        let text = format!("Credits\n{line}\n");
        let mut ok = base.to_vec();
        ok.push(("vendor/ATTRIBUTION.md", text.as_str()));
        assert!(found(Lm::Notice, &ok).is_empty(), "{line}");
    }
}

#[test]
fn a_notice_in_an_ancestor_directory_or_the_covered_files_own_header_satisfies_it_but_licences_dir_does_not(
) {
    let m = no_requires();
    let at = |extra: (&'static str, &'static str)| {
        found(
            Lm::Notice,
            &[
                (MANIFEST, m.as_str()),
                ("vendor/a.c", "x\n"),
                ("LICENSES/MIT.txt", "MIT\n"),
                extra,
            ],
        )
    };
    assert!(
        at(("NOTICE", NOTICE)).is_empty(),
        "root notice, an ancestor"
    );
    assert!(
        at(("vendor/THIRD-PARTY.txt", NOTICE)).is_empty(),
        "THIRD prefix"
    );
    assert_eq!(
        at(("other/NOTICE", NOTICE)).len(),
        1,
        "a sibling directory is not an ancestor"
    );
    assert_eq!(
        at(("LICENSES/NOTICE", NOTICE)).len(),
        1,
        "LICENSES/ is never searched"
    );
    let header = format!("// {}", NOTICE);
    assert!(found(
        Lm::Notice,
        &[
            (MANIFEST, m.as_str()),
            ("vendor/a.c", header.as_str()),
            ("LICENSES/MIT.txt", "MIT\n")
        ]
    )
    .is_empty());
    let late = format!("{}{}", "x\n".repeat(60), NOTICE);
    assert_eq!(
        found(
            Lm::Notice,
            &[
                (MANIFEST, m.as_str()),
                ("vendor/a.c", late.as_str()),
                ("LICENSES/MIT.txt", "MIT\n")
            ]
        )
        .len(),
        1,
        "line 61 is outside the header"
    );
}

#[test]
fn the_licence_text_is_required_when_requires_is_empty() {
    let m = no_requires();
    let f = found(
        Lm::Notice,
        &[
            (MANIFEST, m.as_str()),
            ("vendor/a.c", "x\n"),
            ("vendor/NOTICE", NOTICE),
        ],
    );
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("LICENSES/MIT.txt"));
    assert!(found(
        Lm::Notice,
        &[
            (MANIFEST, m.as_str()),
            ("vendor/a.c", "x\n"),
            ("vendor/NOTICE", NOTICE),
            ("LICENSES/MIT.txt", "t\n")
        ]
    )
    .is_empty());
}

#[test]
fn a_licence_without_attribution_duty_needs_no_holder() {
    for lic in ["CC0-1.0", "0BSD", "Unlicense", "MIT-0"] {
        let m = no_requires()
            .replace("origin-copyright = \"RIKEN\"\n", "")
            .replace(
                "origin-licence = \"MIT\"",
                &format!("origin-licence = \"{lic}\""),
            );
        let text = format!("LICENSES/{lic}.txt");
        let f = found(
            Lm::Notice,
            &[
                (MANIFEST, m.as_str()),
                ("vendor/a.c", "x\n"),
                (text.as_str(), "t\n"),
            ],
        );
        assert!(f.is_empty(), "{lic}");
        assert!(
            found(
                Lm::Parse,
                &[
                    (MANIFEST, m.as_str()),
                    ("vendor/a.c", "x\n"),
                    (text.as_str(), "t\n")
                ]
            )
            .is_empty(),
            "{lic}"
        );
    }
}

// ---- SUP-022 (LM-001) -------------------------------------------------------------------------------------------------

fn parse_messages(manifest: &str) -> Vec<String> {
    found(
        Lm::Parse,
        &[
            (MANIFEST, manifest),
            ("vendor/a.c", "x\n"),
            ("vendor/NOTICE", NOTICE),
        ],
    )
    .into_iter()
    .map(|f| f.summary)
    .collect()
}

#[test]
fn a_manifest_that_does_not_parse_is_one_finding_with_a_line() {
    let f = found(Lm::Parse, &[(MANIFEST, "version = 1\nversion = [\n")]);
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("does not parse"));
    assert!(f[0].location.as_ref().is_some_and(|l| l.line.is_some()));
}

#[test]
fn an_unknown_key_is_an_error_and_a_typo_drops_the_required_origin_key_too() {
    let got = parse_messages(&ENTRY.replace("origin-url", "origin-urll"));
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got.iter().any(|g| g.contains("unknown key `origin-urll`")));
    assert!(got.iter().any(|g| g.contains("also needs `origin-url`")));
}

#[test]
fn required_keys_version_and_value_types_are_checked() {
    assert!(
        parse_messages(&ENTRY.replace("SPDX-FileCopyrightText = \"2022 RIKEN\"\n", ""))
            .iter()
            .any(|g| g.contains("`SPDX-FileCopyrightText`"))
    );
    assert!(parse_messages(&ENTRY.replace("version = 1", "version = 2"))
        .iter()
        .any(|g| g.contains("`version` must be 1")));
    assert!(
        parse_messages(&ENTRY.replace("path = \"vendor/**\"", "path = 3"))
            .iter()
            .any(|g| g.contains("`path` must be a string"))
    );
    assert!(
        parse_messages(&ENTRY.replace("origin-copyright = \"RIKEN\"\n", ""))
            .iter()
            .any(|g| g.contains("lacks `origin-copyright`"))
    );
    // a list is a valid REUSE value for the copyright text
    assert!(
        parse_messages(&ENTRY.replace("\"2022 RIKEN\"", "[\"2022 RIKEN\", \"2023 X\"]")).is_empty()
    );
}

#[test]
fn relation_precedence_and_comment_rules() {
    assert!(parse_messages(&format!("{ENTRY}relation = \"OTHER\"\n"))
        .iter()
        .any(|g| g.contains("needs `relation-comment`")));
    assert!(
        parse_messages(&format!("{ENTRY}relation = \"PORTED_FROM\"\n"))
            .iter()
            .any(|g| g.contains("`relation` must be one of"))
    );
    assert!(parse_messages(&format!(
        "{ENTRY}relation = \"OTHER\"\nrelation-comment = \"ported\"\n"
    ))
    .is_empty());
    assert!(
        parse_messages(&format!("{ENTRY}relation-comment = \"x\"\n"))
            .iter()
            .any(|g| g.contains("without `relation`"))
    );
    assert!(parse_messages(&format!("{ENTRY}precedence = \"first\"\n"))
        .iter()
        .any(|g| g.contains("`precedence`")));
    assert!(parse_messages(&format!("{ENTRY}precedence = \"override\"\n")).is_empty());
}

#[test]
fn snippet_lines_must_be_a_to_b_one_based_and_not_backwards() {
    let snip = |lines: &str| {
        format!("{ENTRY}[[snippet]]\npath = \"vendor/a.c\"\nlines = \"{lines}\"\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\n")
    };
    for bad in ["9", "0:3", "5:3", "a:b", "1:", ":2", "1-3"] {
        assert!(
            parse_messages(&snip(bad))
                .iter()
                .any(|g| g.contains("`lines`")),
            "{bad}"
        );
    }
    for good in ["1:1", "3:5", "120:188"] {
        assert!(parse_messages(&snip(good)).is_empty(), "{good}");
    }
    // a `precedence` key is not a snippet key; `lines` is not an annotations key
    assert!(
        parse_messages(&format!("{}precedence = \"closest\"\n", snip("1:2")))
            .iter()
            .any(|g| g.contains("unknown key `precedence`"))
    );
    assert!(parse_messages(&format!("{ENTRY}lines = \"1:2\"\n"))
        .iter()
        .any(|g| g.contains("unknown key `lines`")));
    // a snippet needs `lines`
    assert!(parse_messages(&format!("{ENTRY}[[snippet]]\npath = \"vendor/a.c\"\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\n")).iter().any(|g| g.contains("`lines` is required")));
}

// ---- SUP-024 (LM-003) -------------------------------------------------------------------------------------------------

const EMPTY: &str = "version = 1\n";

fn markers(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let (f, n) = run(Lm::Marker, files);
    (
        f.iter()
            .map(|x| {
                format!(
                    "{}:{:?}",
                    x.location.as_ref().map_or("", |l| l.file.as_str()),
                    x.location.as_ref().and_then(|l| l.line)
                )
            })
            .collect(),
        n,
    )
}

#[test]
fn an_undeclared_marker_in_code_is_a_finding_with_its_line() {
    let (f, n) = markers(&[
        (MANIFEST, EMPTY),
        ("src/a.rs", "fn a() {}\n// ported from foo\n"),
    ]);
    assert_eq!(f, vec!["src/a.rs:Some(2)"]);
    assert!(n.is_empty());
    for word in ["Origin: x", "DERIVED FROM y", "vendored", "Ported From z"] {
        let text = format!("// {word}\n");
        assert_eq!(
            markers(&[(MANIFEST, EMPTY), ("a.rs", text.as_str())])
                .0
                .len(),
            1,
            "{word}"
        );
    }
    assert!(
        markers(&[(MANIFEST, EMPTY), ("a.rs", "// original author\n")])
            .0
            .is_empty(),
        "control: a near word"
    );
}

#[test]
fn only_the_first_60_lines_are_read() {
    let late = format!("{}// vendored\n", "x\n".repeat(60));
    assert!(markers(&[(MANIFEST, EMPTY), ("a.rs", late.as_str())])
        .0
        .is_empty());
    let edge = format!("{}// vendored\n", "x\n".repeat(59));
    assert_eq!(
        markers(&[(MANIFEST, EMPTY), ("a.rs", edge.as_str())]).0,
        vec!["a.rs:Some(60)"]
    );
}

#[test]
fn licence_text_files_are_never_scanned() {
    let text = "// derived from foo\n";
    let (f, _) = markers(&[
        (MANIFEST, EMPTY),
        ("LICENSE-APACHE", text),
        ("LICENSES/Apache-2.0.txt", text),
        ("crates/x/LICENSES/y.txt", text),
        ("crates/x/COPYING", text),
        ("license.md", text),
        ("src/a.rs", text),
    ]);
    assert_eq!(f, vec!["src/a.rs:Some(1)"]);
}

#[test]
fn a_marker_in_prose_is_an_info_note_and_never_a_finding() {
    for ext in ["md", "markdown", "rst", "txt", "tsv", "MD"] {
        let name = format!("docs/a.{ext}");
        let (f, n) = markers(&[(MANIFEST, EMPTY), (name.as_str(), "ported from foo\n")]);
        assert!(f.is_empty(), "{ext}");
        assert_eq!(n.len(), 1, "{ext}");
        assert!(n[0].contains("SUP-024 (LM-003)") && n[0].contains("credit-only prose"));
    }
    // control: the same text in code, and in a file whose extension merely resembles prose
    assert_eq!(
        markers(&[(MANIFEST, EMPTY), ("a.rs", "ported from foo\n")])
            .0
            .len(),
        1
    );
    assert_eq!(
        markers(&[(MANIFEST, EMPTY), ("a.mdx", "ported from foo\n")])
            .0
            .len(),
        1
    );
    // a prose-only repository: the verdict is clean of findings
    assert!(
        matches!(judge(&RepoView::from_files(&[(MANIFEST, EMPTY), ("a.md", "vendored\n")]), Lm::Marker), Verdict::Noted(f, n) if f.is_empty() && n.len() == 1)
    );
}

#[test]
fn an_annotation_covers_its_files_and_the_manifest_file_is_not_scanned() {
    let files = [
        (MANIFEST, ENTRY),
        ("vendor/a.c", "// vendored\n"),
        ("vendor/NOTICE", NOTICE),
    ];
    assert!(markers(&files).0.is_empty());
    // control: the same file outside the entry's glob is a finding
    let outside = [
        (MANIFEST, ENTRY),
        ("src/a.c", "// vendored\n"),
        ("vendor/NOTICE", NOTICE),
        ("vendor/a.c", "x\n"),
    ];
    assert_eq!(markers(&outside).0, vec!["src/a.c:Some(1)"]);
    let manifest_mentions = format!("{EMPTY}# vendored here\n");
    assert!(markers(&[(MANIFEST, manifest_mentions.as_str())])
        .0
        .is_empty());
}

fn snippet(lines: &str) -> String {
    format!("{EMPTY}[[snippet]]\npath = \"src/a.rs\"\nlines = \"{lines}\"\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\n")
}

#[test]
fn a_snippet_covers_a_marker_only_inside_its_inclusive_one_based_range() {
    // markers on lines 2 and 4
    let src = "fn a() {}\n// ported from a\nfn b() {}\n// ported from b\n";
    let at = |lines: &str| markers(&[(MANIFEST, snippet(lines).as_str()), ("src/a.rs", src)]).0;
    assert_eq!(at("2:4"), Vec::<String>::new(), "both ends inclusive");
    assert_eq!(
        at("3:4"),
        vec!["src/a.rs:Some(2)"],
        "line 2 is one before the range"
    );
    assert_eq!(
        at("1:2"),
        vec!["src/a.rs:Some(4)"],
        "line 4 is one past the range"
    );
    assert_eq!(at("2:2"), vec!["src/a.rs:Some(4)"], "a single-line range");
    assert_eq!(
        at("5:9").len(),
        1,
        "a range with no marker covers nothing, and one finding per file"
    );
    // the snippet does not cover another file, and does not cover its own file's marker outside the range
    let other = markers(&[
        (MANIFEST, snippet("1:9").as_str()),
        ("src/b.rs", src),
        ("src/a.rs", "x\n"),
    ])
    .0;
    assert_eq!(other, vec!["src/b.rs:Some(2)"]);
}

// ---- SUP-025 (LM-004) -------------------------------------------------------------------------------------------------

#[test]
fn an_entry_whose_path_matches_no_tracked_file_is_stale_and_globs_follow_reuse() {
    let f = found(Lm::Stale, &[(MANIFEST, ENTRY), ("src/a.rs", "x\n")]);
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("SUP-025 (LM-004)"));
    assert!(
        found(
            Lm::Stale,
            &[(MANIFEST, ENTRY), ("vendor/deep/er/a.c", "x\n")]
        )
        .is_empty(),
        "** crosses directories"
    );
    let single = ENTRY.replace("vendor/**", "vendor/*");
    assert_eq!(
        found(
            Lm::Stale,
            &[(MANIFEST, single.as_str()), ("vendor/deep/a.c", "x\n")]
        )
        .len(),
        1,
        "* stays inside a directory"
    );
    assert!(found(
        Lm::Stale,
        &[(MANIFEST, single.as_str()), ("vendor/a.c", "x\n")]
    )
    .is_empty());
    let exact = ENTRY.replace("vendor/**", "vendor/a.c");
    assert!(found(
        Lm::Stale,
        &[(MANIFEST, exact.as_str()), ("vendor/a.c", "x\n")]
    )
    .is_empty());
    assert_eq!(
        found(
            Lm::Stale,
            &[(MANIFEST, exact.as_str()), ("vendor/b.c", "x\n")]
        )
        .len(),
        1
    );
    // a snippet's path is checked too
    let snip = snippet("1:2");
    assert_eq!(
        found(
            Lm::Stale,
            &[(MANIFEST, snip.as_str()), ("src/other.rs", "x\n")]
        )
        .len(),
        1
    );
    assert!(found(Lm::Stale, &[(MANIFEST, snip.as_str()), ("src/a.rs", "x\n")]).is_empty());
}

// ---- SUP-026 (LM-005) -------------------------------------------------------------------------------------------------

fn compat(origin: &str, ours: &str) -> (Vec<Finding>, Vec<String>) {
    let m = ENTRY
        .replace(
            "origin-licence = \"MIT\"",
            &format!("origin-licence = \"{origin}\""),
        )
        .replace(
            "SPDX-License-Identifier = \"MIT\"",
            &format!("SPDX-License-Identifier = \"{ours}\""),
        );
    run(
        Lm::Compat,
        &[
            (MANIFEST, m.as_str()),
            ("vendor/a.c", "x\n"),
            ("vendor/NOTICE", NOTICE),
        ],
    )
}

#[test]
fn an_incompatible_pair_is_a_finding_and_a_compatible_one_is_not() {
    let (f, _) = compat("GPL-3.0-only", "MIT");
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.contains("`MIT`") && f[0].summary.contains("SUP-026 (LM-005)"));
    assert!(compat("MIT", "MIT OR Apache-2.0").0.is_empty());
    assert!(compat("GPL-3.0-or-later", "AGPL-3.0-or-later").0.is_empty());
    assert_eq!(
        compat("GPL-3.0-only", "GPL-3.0-only OR MIT").0.len(),
        1,
        "A OR B offers both"
    );
    assert!(
        compat("GPL-3.0-only", "GPL-3.0-only WITH Classpath-exception-2.0")
            .0
            .is_empty()
    );
    assert_eq!(compat("Apache-2.0", "GPL-2.0-only").0.len(), 1);
    assert!(compat("Apache-2.0", "GPL-3.0-only").0.is_empty());
    assert_eq!(compat("MPL-2.0", "MIT").0.len(), 1);
    assert!(compat("MPL-2.0", "LGPL-2.1-or-later").0.is_empty());
}

#[test]
fn an_unknown_licence_is_an_info_note_never_a_finding_and_still_needs_a_notice() {
    let (f, n) = compat("Weird-1.0", "MIT");
    assert!(f.is_empty());
    assert_eq!(n.len(), 1);
    assert!(n[0].starts_with("info SUP-026 (LM-005)") && n[0].contains("Weird-1.0"));
    let (f, n) = compat("GPL-3.0-only", "LicenseRef-Proprietary");
    assert!(f.is_empty() && n.len() == 1);
    // the unknown origin falls back to attribution-requiring: SUP-023 asks for the holder's notice
    let m = ENTRY.replace("origin-licence = \"MIT\"", "origin-licence = \"Weird-1.0\"");
    assert_eq!(
        found(Lm::Notice, &[(MANIFEST, m.as_str()), ("vendor/a.c", "x\n")]).len(),
        1
    );
}

#[test]
fn a_snippet_is_checked_for_compatibility_too() {
    let body = "version = 1\n[[snippet]]\npath = \"src/a.rs\"\nlines = \"1:2\"\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\norigin-url = \"u\"\norigin-licence = \"AGPL-3.0-only\"\norigin-copyright = \"Y\"\n";
    let f = found(
        Lm::Compat,
        &[
            (MANIFEST, body),
            ("src/a.rs", "a\nb\n// Copyright Y\n"),
            ("LICENSES/AGPL-3.0-only.txt", "t\n"),
        ],
    );
    assert_eq!(f.len(), 1);
    assert!(found(
        Lm::Notice,
        &[
            (MANIFEST, body),
            ("src/a.rs", "a\nb\n// Copyright Y\n"),
            ("LICENSES/AGPL-3.0-only.txt", "t\n")
        ]
    )
    .is_empty());
}

// Divergences settled with licence-tools 2026-11-19: the same fixtures as `class Divergences` in its tests/test_licence_manifest.py.
const HEAD: &str = "version = 1\n[[annotations]]\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\n";

#[test]
fn a_wrong_typed_value_is_a_parse_finding_never_a_panic_and_nothing_else_fires() {
    let cases: Vec<(&str, String)> = vec![
        ("path int", "version = 1\n[[annotations]]\npath = 5\nSPDX-FileCopyrightText = \"x\"\nSPDX-License-Identifier = \"MIT\"\n".into()),
        ("requires string", format!("{HEAD}path = \"a.c\"\nrequires = \"NOTICE\"\n")),
        ("origin-licence int", format!("{HEAD}path = \"a.c\"\norigin-url = \"u\"\norigin-licence = 3\n")),
        ("annotations not an array", "version = 1\nannotations = \"x\"\n".into()),
        ("entry not a table", "version = 1\nannotations = [1]\n".into()),
        ("snippet lines int", format!("{}path = \"a.c\"\nlines = 5\n", HEAD.replace("annotations", "snippet"))),
    ];
    for (name, body) in cases {
        let counts = all_rules(&[(MANIFEST, &body), ("a.c", "x\n")]);
        for (l, n) in counts {
            if l == Lm::Parse {
                assert!(n > 0, "{name}: no parse finding");
            } else {
                assert_eq!(n, 0, "{name}: {l:?} fired on a skipped entry");
            }
        }
    }
}

#[test]
fn lm002_is_once_per_entry_and_missing_requires_file_naming_both() {
    let body = format!(
        "{HEAD}path = \"a/**\"\nrequires = [\"N1\", \"N2\"]\n[[annotations]]\nSPDX-FileCopyrightText = \"y\"\nSPDX-License-Identifier = \"MIT\"\npath = \"b/**\"\nrequires = [\"N1\"]\n"
    );
    let f = found(
        Lm::Notice,
        &[(MANIFEST, &body), ("a/x", "x\n"), ("b/x", "x\n")],
    );
    assert_eq!(f.len(), 3, "{f:?}");
    assert_eq!(
        f.iter()
            .filter(|x| x.summary.contains("`N1`") && x.summary.contains("#2"))
            .count(),
        1
    );
    assert_eq!(
        f.iter()
            .filter(|x| x.summary.contains("`N2`") && x.summary.contains("#1"))
            .count(),
        1
    );
}

/// NOT adopted from licence-tools' Divergences (their "licence list" case): REUSE.toml lets `SPDX-License-Identifier` hold a list, and the
/// schema is a superset of REUSE.toml, so a list is valid here and only skipped by the compatibility check. Open with licence-tools.
#[test]
fn a_list_of_licence_identifiers_is_valid_reuse_and_not_a_parse_finding() {
    let body = format!("{}path = \"a.c\"\n", HEAD.replace("\"MIT\"", "[\"MIT\"]"));
    assert!(found(Lm::Parse, &[(MANIFEST, &body), ("a.c", "x\n")]).is_empty());
}

#[test]
fn a_list_of_licence_identifiers_is_checked_element_wise_against_the_origin_licence() {
    let body = format!(
        "{}path = \"a.c\"\norigin-url = \"u\"\norigin-licence = \"GPL-3.0-only\"\norigin-copyright = \"x\"\nrequires = [\"N\"]\n",
        HEAD.replace("\"MIT\"", "[\"GPL-3.0-only\", \"MIT\"]")
    );
    let f = found(
        Lm::Compat,
        &[(MANIFEST, &body), ("a.c", "x\n"), ("N", "Copyright x\n")],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].summary.contains("`MIT`"), "{}", f[0].summary);
}

#[test]
fn a_wrong_typed_precedence_list_is_a_parse_finding_not_a_panic() {
    let body = format!("{HEAD}path = \"a.c\"\nprecedence = [\"x\"]\n");
    assert!(!found(Lm::Parse, &[(MANIFEST, &body), ("a.c", "x\n")]).is_empty());
}

#[test]
fn a_path_list_covers_the_union_and_is_stale_only_when_no_element_matches() {
    let live = HEAD.to_string()
        + "path = [\"a.c\", \"b/**\"]
";
    let files = [
        (MANIFEST, live.as_str()),
        (
            "a.c", "x
",
        ),
        (
            "b/y.c", "x
",
        ),
        (
            "c.c", "x
",
        ),
    ];
    assert!(
        found(Lm::Stale, &files).is_empty(),
        "a list with live elements is not stale"
    );
    let stale_beside_live = HEAD.to_string()
        + "path = [\"a.c\", \"gone/**\"]
";
    assert!(found(
        Lm::Stale,
        &[
            (MANIFEST, stale_beside_live.as_str()),
            (
                "a.c", "x
"
            )
        ]
    )
    .is_empty());
    let all_stale = HEAD.to_string()
        + "path = [\"x/**\", \"y/**\"]
";
    assert_eq!(
        found(
            Lm::Stale,
            &[
                (MANIFEST, all_stale.as_str()),
                (
                    "a.c", "x
"
                )
            ]
        )
        .len(),
        1
    );
    let bad = HEAD.to_string()
        + "path = [\"a.c\", 3]
";
    assert!(!found(
        Lm::Parse,
        &[
            (MANIFEST, bad.as_str()),
            (
                "a.c", "x
"
            )
        ]
    )
    .is_empty());
    let covered = HEAD.to_string()
        + "path = [\"a.c\"]
";
    let marked = [
        (MANIFEST, covered.as_str()),
        (
            "a.c",
            "// Origin: elsewhere
",
        ),
        (
            "c.c",
            "// Origin: elsewhere
",
        ),
    ];
    let m = found(Lm::Marker, &marked);
    assert_eq!(m.len(), 1, "only c.c is uncovered: {m:?}");
}
