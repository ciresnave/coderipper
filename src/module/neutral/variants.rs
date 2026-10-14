//! Planted variants for the language-neutral rules (CodeRipper's own self-check, decision 180): each rule's conformance fixture
//! seeds ONE shape of its defect, so a rule that fires on that shape and misses its siblings is "proven" and still has a false
//! negative. Each case here plants a sibling shape the rule's documentation says it judges (or a clean twin of it) and states the
//! verdict the rule must give. A case that failed when it was written is named in the CHANGELOG with its fix.

use super::{doc005, doc009, sup001, sup011, wsp001, RepoView, Verdict};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Got {
    /// The rule ran and reported this many findings.
    Findings(usize),
    NotApplicable,
}

fn got(judge: fn(&RepoView) -> Verdict, files: &[(&str, &str)]) -> Got {
    match judge(&RepoView::from_files(files)) {
        Verdict::Findings(f) => Got::Findings(f.len()),
        Verdict::NotApplicable(_) => Got::NotApplicable,
    }
}

const CARGO_APP: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LOCK: &str = "# lock\n";

fn workflow(run: &str) -> String {
    format!("name: ci\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: {run}\n")
}

fn sup001_ci(run: &str) -> Got {
    let wf = workflow(run);
    got(
        sup001::judge,
        &[
            ("Cargo.toml", CARGO_APP),
            ("src/main.rs", "fn main() {}\n"),
            ("Cargo.lock", LOCK),
            (".github/workflows/ci.yml", &wf),
        ],
    )
}

#[test]
fn sup001_cargo_commands_without_the_strict_mode_fire_in_every_spelling() {
    for run in [
        "cargo build",
        "cargo test --all",
        "cargo check",
        "cargo clippy --all-targets",
        "cargo run --release",
        "cargo doc --no-deps",
        "cargo bench",
        "cargo +nightly test",
        "cargo +stable build --release",
        "cargo build --release && cargo test",
        "echo hi; cargo build",
        "cargo -q build",
        "cargo --quiet test",
        "cargo -v clippy",
        "cargo --color always build",
        "cargo -Z unstable-options test",
        "cargo --config build.jobs=2 test",
        "cargo +nightly -q build --release",
    ] {
        assert_eq!(sup001_ci(run), Got::Findings(1), "should fire: {run}");
    }
}

#[test]
fn sup001_cargo_commands_with_the_strict_mode_are_clean() {
    for run in [
        "cargo build --locked",
        "cargo test --frozen",
        "cargo +nightly build --locked --release",
        "cargo --locked build",
        "cargo -q --locked test",
        "cargo --frozen -q build",
        "cargo --color always --locked build",
        "cargo fmt --check",
        "cargo install cargo-audit",
        "cargo metadata --format-version 1",
    ] {
        assert_eq!(sup001_ci(run), Got::Findings(0), "should be clean: {run}");
    }
}

#[test]
fn sup001_a_multiline_run_block_and_a_yaml_extension_are_read() {
    let wf = "name: ci\non: push\njobs:\n  b:\n    runs-on: x\n    steps:\n      - run: |\n          cargo fmt --check\n          cargo test\n";
    for path in [".github/workflows/ci.yml", ".github/workflows/ci.yaml"] {
        let found = got(
            sup001::judge,
            &[
                ("Cargo.toml", CARGO_APP),
                ("src/main.rs", ""),
                ("Cargo.lock", LOCK),
                (path, wf),
            ],
        );
        assert_eq!(found, Got::Findings(1), "{path}");
    }
}

#[test]
fn sup001_a_missing_lockfile_is_reported_for_each_application_shape() {
    // a binary by src/main.rs, by [[bin]], and a private npm package
    let bin_target = format!("{CARGO_APP}[[bin]]\nname = \"tool\"\npath = \"src/tool.rs\"\n");
    assert_eq!(
        got(
            sup001::judge,
            &[("Cargo.toml", CARGO_APP), ("src/main.rs", "")]
        ),
        Got::Findings(1)
    );
    assert_eq!(
        got(
            sup001::judge,
            &[("Cargo.toml", &bin_target), ("src/tool.rs", "")]
        ),
        Got::Findings(1)
    );
    assert_eq!(
        got(
            sup001::judge,
            &[("package.json", "{\"name\":\"a\",\"private\":true}")]
        ),
        Got::Findings(1)
    );
    // a member application below a workspace root with no lock anywhere
    assert_eq!(
        got(
            sup001::judge,
            &[
                ("Cargo.toml", "[workspace]\nmembers = [\"app\"]\n"),
                ("app/Cargo.toml", CARGO_APP),
                ("app/src/main.rs", ""),
            ]
        ),
        Got::Findings(1)
    );
}

#[test]
fn sup001_every_lockfile_kind_satisfies_an_npm_application_and_a_library_is_not_judged() {
    for lock in [
        "package-lock.json",
        "npm-shrinkwrap.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lock",
        "bun.lockb",
    ] {
        assert_eq!(
            got(
                sup001::judge,
                &[("package.json", "{\"private\":true}"), (lock, "x")]
            ),
            Got::Findings(0),
            "{lock}"
        );
    }
    // a workspace lock at the root covers a member
    assert_eq!(
        got(
            sup001::judge,
            &[
                ("Cargo.toml", "[workspace]\nmembers = [\"app\"]\n"),
                ("Cargo.lock", LOCK),
                ("app/Cargo.toml", CARGO_APP),
                ("app/src/main.rs", ""),
            ]
        ),
        Got::Findings(0)
    );
    // a library (no main.rs, no [[bin]]) needs no lock
    assert!(matches!(
        got(
            sup001::judge,
            &[("Cargo.toml", CARGO_APP), ("src/lib.rs", "")]
        ),
        Got::Findings(0) | Got::NotApplicable
    ));
}

#[test]
fn sup001_npm_and_yarn_installs_fire_in_ci_only_when_a_lock_is_tracked() {
    let with = |lock: &str, run: &str| {
        let wf = workflow(run);
        got(
            sup001::judge,
            &[
                ("package.json", "{\"private\":true}"),
                (lock, "x"),
                (".github/workflows/ci.yml", &wf),
            ],
        )
    };
    assert_eq!(with("package-lock.json", "npm install"), Got::Findings(1));
    assert_eq!(with("package-lock.json", "npm ci"), Got::Findings(0));
    assert_eq!(with("yarn.lock", "yarn install"), Got::Findings(1));
    assert_eq!(
        with("yarn.lock", "yarn install --frozen-lockfile"),
        Got::Findings(0)
    );
    assert_eq!(
        with("yarn.lock", "yarn install --immutable"),
        Got::Findings(0)
    );
}

// ---------------------------------------------------------------- SUP-011

fn sup011(files: &[(&str, &str)]) -> Got {
    got(sup011::judge, files)
}

#[test]
fn sup011_each_vendored_directory_name_and_each_config_kind_is_judged() {
    for dir in [
        "vendor",
        "vendors",
        "third_party",
        "third-party",
        "node_modules",
        "bower_components",
    ] {
        let path = format!("{dir}/lib/a.js");
        for config in [
            "codecov.yml",
            ".codecov.yml",
            "codecov.yaml",
            ".codacy.yml",
            ".codacy.yaml",
            "sonar-project.properties",
            ".eslintignore",
            ".deepsource.toml",
        ] {
            assert_eq!(
                sup011(&[(&path, "x"), (config, "ignore: []\n")]),
                Got::Findings(1),
                "{dir} with {config}"
            );
            assert_eq!(
                sup011(&[(&path, "x"), (config, &format!("ignore:\n  - {dir}\n"))]),
                Got::Findings(0),
                "{dir} named in {config}"
            );
        }
    }
}

#[test]
fn sup011_a_nested_vendored_directory_is_found_and_judged_by_its_path() {
    assert_eq!(
        sup011(&[
            ("src/third_party/x.c", "x"),
            ("codecov.yml", "ignore: []\n")
        ]),
        Got::Findings(1)
    );
    assert_eq!(
        sup011(&[
            ("src/third_party/x.c", "x"),
            ("codecov.yml", "ignore:\n  - src/third_party\n")
        ]),
        Got::Findings(0)
    );
    // two vendored directories, one excluded: one finding
    assert_eq!(
        sup011(&[
            ("vendor/a.c", "x"),
            ("web/node_modules/b.js", "x"),
            ("codecov.yml", "ignore:\n  - vendor\n")
        ]),
        Got::Findings(1)
    );
}

#[test]
fn sup011_vendored_code_without_any_analysis_config_is_not_applicable_and_a_file_named_vendor_is_not_a_directory(
) {
    assert_eq!(sup011(&[("vendor/a.c", "x")]), Got::NotApplicable);
    assert_eq!(
        sup011(&[("src/vendor", "x"), ("codecov.yml", "ignore: []\n")]),
        Got::Findings(0)
    );
}

// ---------------------------------------------------------------- WSP-001

#[test]
fn wsp001_a_missing_or_defaultless_codeowners_fires_for_every_shape() {
    for files in [
        vec![("README.md", "x")],
        vec![(".github/CODEOWNERS", "# only a comment\n")],
        vec![(".github/CODEOWNERS", "")],
        vec![(".github/CODEOWNERS", "/src/ @alice\n")],
        vec![(".github/CODEOWNERS", "*.rs @alice\n")],
        vec![(".github/CODEOWNERS", "*\n")],
        vec![(".github/CODEOWNERS", "* alice\n")],
        vec![(".github/CODEOWNERS", "**/docs @alice\n")],
        // a CODEOWNERS GitHub does not read
        vec![("src/CODEOWNERS", "* @alice\n")],
    ] {
        assert_eq!(got(wsp001::judge, &files), Got::Findings(1), "{files:?}");
    }
}

#[test]
fn wsp001_a_default_owner_in_any_accepted_spelling_and_place_is_clean() {
    for files in [
        vec![(".github/CODEOWNERS", "* @alice\n")],
        vec![("CODEOWNERS", "* @alice\n")],
        vec![("docs/CODEOWNERS", "* @alice\n")],
        vec![(".github/CODEOWNERS", "* @org/team\n")],
        vec![(".github/CODEOWNERS", "* alice@example.com\n")],
        vec![(".github/CODEOWNERS", "*   @alice   # why\n")],
        vec![(
            ".github/CODEOWNERS",
            "# header\n\n* @alice @bob\n/src/ @carol\n",
        )],
        vec![(".github/CODEOWNERS", "* @alice\r\n/docs @bob\r\n")],
        vec![(".github/CODEOWNERS", "/src/ @carol\n* @alice\n")],
    ] {
        assert_eq!(got(wsp001::judge, &files), Got::Findings(0), "{files:?}");
    }
}

// ---------------------------------------------------------------- DOC-009

fn pages(dir: &str, n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("{dir}/page-{i}.md"), "# Page\n".to_string()))
        .collect()
}

fn doc009(extra: &[(&str, &str)], dir: &str, n: usize) -> Got {
    let owned = pages(dir, n);
    let mut files: Vec<(&str, &str)> = owned
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    files.extend_from_slice(extra);
    got(doc009::judge, &files)
}

#[test]
fn doc009_ten_pages_in_docs_or_doc_without_a_glossary_fire_and_nine_do_not() {
    assert_eq!(doc009(&[], "docs", 10), Got::Findings(1));
    assert_eq!(doc009(&[], "doc", 10), Got::Findings(1));
    assert_eq!(doc009(&[], "docs", 9), Got::NotApplicable);
    // a nested docs directory is deliberately not documentation (see the rule's own limits), so it must stay N/A, not clean
    assert_eq!(doc009(&[], "packages/x/docs", 12), Got::NotApplicable);
}

#[test]
fn doc009_a_glossary_in_any_place_or_case_satisfies_and_duplicates_fire_in_each_term_shape() {
    for place in [
        "GLOSSARY.md",
        "glossary.md",
        "docs/Glossary.md",
        "docs/reference/terms-glossary.md",
        "docs/GLOSSARY.MD",
        "docs/glossary.txt",
    ] {
        assert_eq!(
            doc009(&[(place, "## Alpha\n\nx\n")], "docs", 10),
            Got::Findings(0),
            "{place}"
        );
    }
    for glossary in [
        "## Rule\n\na\n\n## rule\n\nb\n",
        "### Rule\n\na\n\n## Rule\n\nb\n",
        "**Rule** - a\n\n**rule** - b\n",
        "- **Rule**: a\n- **Rule**: b\n",
        "* **Rule**: a\n\n## Rule\n",
    ] {
        assert_eq!(
            doc009(&[("docs/glossary.md", glossary)], "docs", 10),
            Got::Findings(1),
            "{glossary:?}"
        );
    }
}

// ---------------------------------------------------------------- DOC-005

fn doc005(files: &[(&str, &str)]) -> Got {
    got(doc005::judge, files)
}

#[test]
fn doc005_each_status_spelling_is_read_and_a_missing_or_unknown_one_fires() {
    for (name, text) in [
        ("front matter", "---\nstatus: accepted\n---\n# T\n"),
        ("status line", "# T\n\nStatus: Accepted\n"),
        ("heading", "# T\n\n## Status\n\nProposed\n"),
        (
            "status line, deprecated",
            "# T\nStatus: deprecated since 2024\n",
        ),
    ] {
        for dir in ["docs/adr", "adr", "docs/adrs", "doc/decisions"] {
            let path = format!("{dir}/0001-x.md");
            assert_eq!(
                doc005(&[(&path, text)]),
                Got::Findings(0),
                "{name} in {dir}"
            );
        }
    }
    for (name, text) in [
        ("none", "# T\n\nNo status here.\n"),
        ("unknown word", "# T\n\nStatus: maybe\n"),
        (
            "empty heading section",
            "# T\n\n## Status\n\n## Context\nx\n",
        ),
    ] {
        assert_eq!(
            doc005(&[("docs/adr/0001-x.md", text)]),
            Got::Findings(1),
            "{name}"
        );
    }
}

#[test]
fn doc005_supersession_must_point_at_a_record_that_exists_and_says_so_back() {
    let old_ok = "# One\n\nStatus: Superseded by [ADR-2](0002-b.md)\n";
    let new_ok = "# Two\n\nStatus: Accepted\n\nSupersedes [ADR-1](0001-a.md)\n";
    assert_eq!(
        doc005(&[
            ("docs/adr/0001-a.md", old_ok),
            ("docs/adr/0002-b.md", new_ok)
        ]),
        Got::Findings(0)
    );
    // a dangling link
    assert_eq!(doc005(&[("docs/adr/0001-a.md", old_ok)]), Got::Findings(1));
    // not reciprocated: the newer record does not say it supersedes
    assert_eq!(
        doc005(&[
            ("docs/adr/0001-a.md", old_ok),
            ("docs/adr/0002-b.md", "# Two\n\nStatus: Accepted\n")
        ]),
        Got::Findings(1)
    );
    // the newer says it supersedes, the old is still marked accepted
    assert_eq!(
        doc005(&[
            ("docs/adr/0001-a.md", "# One\n\nStatus: Accepted\n"),
            ("docs/adr/0002-b.md", new_ok)
        ]),
        Got::Findings(1)
    );
    // README, index and template files in an ADR directory are not records
    assert_eq!(
        doc005(&[
            ("docs/adr/README.md", "# ADRs\n"),
            ("docs/adr/index.md", "# Index\n"),
            ("docs/adr/template.md", "# T\n"),
            ("docs/adr/0001-a.md", "# One\n\nStatus: Accepted\n"),
        ]),
        Got::Findings(0)
    );
    assert_eq!(doc005(&[("README.md", "x")]), Got::NotApplicable);
}
