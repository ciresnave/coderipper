// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-010: a binary can report which version and build it is (Rust).
//!
//! **What this judges (Rust, from the source and the manifest):** a package is a *binary package* when it has a `main.rs` or a file under
//! `src/bin/` (outside vendored directories, `tests/`, `examples/`, `benches/`). It *exposes its version* when any of these appears in
//! the text of its sources, its `Cargo.toml` or its `build.rs`:
//!
//! - Cargo's own variables: `CARGO_PKG_VERSION` (`env!`, `option_env!`), or clap's `crate_version!`;
//! - a clap `version`: `#[command(.. version ..)]` or `#[clap(.. version ..)]` (the derive form), `.version(` (the builder form), or
//!   `command!(` (which reads the version from the manifest);
//! - a literal `--version` or `-V`, which is how a hand-rolled argument loop offers it;
//! - a build-information crate or a build script that sets build variables: `vergen`, `built`, `shadow-rs`, `git-version`, `git_version`,
//!   `cargo:rustc-env=`.
//!
//! A package under a `fixtures`, `bench(es)`, `benchmarks`, `measure`, `xtask` or `testdata` directory, or named `*-bench`, is not read (nobody ships its binary).
//! A binary package with none of them is reported once, at its first binary source: Info severity, Medium confidence.
//!
//! **What it cannot see:** a version printed by a different package of the workspace that the binary calls into (the library crate's
//! `--version` handler is not in this package's text); a version exposed only through a macro the rule does not know; whether the
//! version printed carries a source revision (the record asks for "which build"; a bare `CARGO_PKG_VERSION` is what is checked, which ties
//! a report to a release, not to a commit); other languages. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::OPS_010;

/// Text that shows a package offers its version, build or revision.
const MARKERS: &[&str] = &[
    "CARGO_PKG_VERSION",
    "crate_version!",
    "--version",
    "\"-V\"",
    ".version(",
    "command!(",
    "vergen",
    "shadow-rs",
    "built = \"",
    "built = {",
    "built::",
    "shadow_rs",
    "git-version",
    "git_version",
    "cargo:rustc-env=",
    "cargo::rustc-env=",
];

/// Whether a package directory is a fixture, benchmark or measurement helper, whose binary nobody ships.
fn internal(dir: &str) -> bool {
    dir.split('/').any(|seg| {
        matches!(
            seg,
            "fixtures"
                | "fixture"
                | "testdata"
                | "measure"
                | "xtask"
                | "bench"
                | "benches"
                | "benchmarks"
        ) || seg.ends_with("-bench")
            || seg.ends_with("_bench")
    })
}

fn is_binary_source(path: &str) -> bool {
    path == "src/main.rs"
        || path.ends_with("/src/main.rs")
        || path == "main.rs"
        || path.starts_with("src/bin/")
        || path.contains("/src/bin/")
}

/// Whether `text` has a clap attribute `#[command(..)]` or `#[clap(..)]` that names `version`.
fn clap_attribute_with_version(text: &str) -> bool {
    for open in ["#[command(", "#[clap(", "#![command(", "#![clap("] {
        let mut rest = text;
        while let Some(i) = rest.find(open) {
            let after = &rest[i + open.len()..];
            let end = after.find(")]").unwrap_or(after.len());
            let args = &after[..end];
            if args
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| w == "version")
            {
                return true;
            }
            rest = after;
        }
    }
    false
}

fn shows_version(text: &str) -> bool {
    MARKERS.iter().any(|m| text.contains(m)) || clap_attribute_with_version(text)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    let mut findings = Vec::new();
    let mut binaries = 0usize;
    for p in &loaded.packages {
        let Some(first) = p.sources.iter().find(|s| is_binary_source(&s.path)) else {
            continue;
        };
        if internal(&p.dir) {
            continue;
        }
        binaries += 1;
        let mut texts: Vec<String> = p.sources.iter().map(|s| s.text.clone()).collect();
        for extra in ["Cargo.toml", "build.rs"] {
            if let Some(t) = view.read(&join(&p.dir, extra)) {
                texts.push(t);
            }
        }
        if texts.iter().any(|t| shows_version(t)) {
            continue;
        }
        let subject = if p.dir.is_empty() {
            view.name().to_string()
        } else {
            p.dir.clone()
        };
        findings.push(
            Finding::new(
                ID,
                Severity::Info,
                Confidence::Medium,
                view.name(),
                format!(
                    "the binary package `{subject}` has no way to report its version or build: no `CARGO_PKG_VERSION`, no clap `version`, no `--version`, no build-information crate"
                ),
                "Report the version from the program (clap: `#[command(version)]`; otherwise print `env!(\"CARGO_PKG_VERSION\")` on `--version`), and add the source revision through a build script or `vergen`/`built`/`shadow-rs`, so a bug report can be matched to the code that produced it.",
            )
            .location(Location::new(first.path.as_str(), Some(1)))
            .subject(subject)
            .positive_control(format!(
                "{binaries} binary package(s) were read for the same version markers, and a package that names one is not reported"
            )),
        );
    }
    if binaries == 0 {
        return Verdict::NotApplicable(
            "the repository tracks no Rust binary package (no main.rs or src/bin file outside tests, examples and vendored trees)".into(),
        );
    }
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_binary_with_no_version_is_reported_once() {
        let f = found(&[
            ("Cargo.toml", MANIFEST),
            ("src/main.rs", "fn main() { println!(\"hi\"); }\n"),
        ]);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn each_way_to_show_a_version_is_clean() {
        for body in [
            "fn main() { println!(\"{}\", env!(\"CARGO_PKG_VERSION\")); }",
            "#[derive(Parser)]\n#[command(author, version, about)]\nstruct A {}\nfn main() {}",
            "#[derive(Parser)]\n#[clap(version = \"1\")]\nstruct A {}\nfn main() {}",
            "fn main() { let c = Command::new(\"p\").version(\"1\"); }",
            "fn main() { if std::env::args().any(|a| a == \"--version\") {} }",
        ] {
            assert!(
                found(&[("Cargo.toml", MANIFEST), ("src/main.rs", body)]).is_empty(),
                "{body}"
            );
        }
    }

    #[test]
    fn a_clap_attribute_without_version_is_reported() {
        let f = found(&[
            ("Cargo.toml", MANIFEST),
            (
                "src/main.rs",
                "#[derive(Parser)]\n#[command(author, about)]\nstruct A {}\nfn main() {}",
            ),
        ]);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn a_version_in_a_library_file_or_the_build_script_counts() {
        assert!(found(&[
            ("Cargo.toml", MANIFEST),
            ("src/main.rs", "fn main() { p::run(); }"),
            (
                "src/lib.rs",
                "pub fn run() { println!(\"{}\", env!(\"CARGO_PKG_VERSION\")); }"
            ),
        ])
        .is_empty());
        assert!(found(&[
            ("Cargo.toml", MANIFEST),
            ("src/main.rs", "fn main() {}"),
            (
                "build.rs",
                "fn main() { println!(\"cargo:rustc-env=GIT_SHA=abc\"); }"
            ),
        ])
        .is_empty());
    }

    #[test]
    fn a_fixture_or_benchmark_binary_is_not_reported() {
        for dir in ["fixtures/guest", "crates/x-bench", "measure/classify"] {
            let manifest = format!("{dir}/Cargo.toml");
            let main = format!("{dir}/src/main.rs");
            let v = judge(&RepoView::from_files(&[
                ("Cargo.toml", MANIFEST),
                ("src/lib.rs", "pub fn f() {}"),
                (manifest.as_str(), MANIFEST),
                (main.as_str(), "fn main() {}"),
            ]));
            // the only binary is internal, so nothing is reported
            assert!(
                matches!(&v, Verdict::NotApplicable(_))
                    || matches!(&v, Verdict::Findings(f) if f.is_empty()),
                "{dir}"
            );
        }
    }

    #[test]
    fn a_library_only_package_is_not_applicable() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub fn f() {}"),
        ]));
        assert!(matches!(v, Verdict::NotApplicable(_)));
    }
}
