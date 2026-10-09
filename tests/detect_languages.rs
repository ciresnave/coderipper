//! Which languages a project contains that no active module checks. The stderr line of the classic profile and the coverage report
//! both need it. It enumerates from git's index (`git ls-files`), never by walking the disk: a disk walk sees other lanes' worktrees,
//! ignored directories and `node_modules`.

mod common;

use coderipper::coverage::{unchecked_languages, UncheckedLanguage};
use common::{git_repo_with, MANIFEST};

fn found(files: &[(&str, &str)]) -> Vec<(String, usize)> {
    let repo = git_repo_with(files);
    unchecked_languages(repo.path())
        .into_iter()
        .map(|u: UncheckedLanguage| (u.language, u.source_files))
        .collect()
}

#[test]
fn a_rust_only_project_has_nothing_unchecked() {
    assert!(found(&[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")]).is_empty());
}

#[test]
fn a_manifest_and_source_files_make_a_language_unchecked_and_count_the_files() {
    let got = found(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn main() {}\n"),
        ("web/package.json", "{}\n"),
        ("web/a.ts", ""),
        ("web/b.tsx", ""),
        ("web/c.js", ""),
        ("web/readme.md", ""),
    ]);
    assert_eq!(got, vec![("typescript".to_string(), 3)]);
}

#[test]
fn source_files_without_a_manifest_are_not_a_project() {
    // A stray script is not "a TypeScript project we skipped": the manifest is what says the language is present.
    assert!(found(&[("Cargo.toml", MANIFEST), ("tools/helper.py", "print(1)\n")]).is_empty());
}

#[test]
fn a_manifest_without_source_files_is_not_reported() {
    assert!(found(&[("Cargo.toml", MANIFEST), ("package.json", "{}\n")]).is_empty());
}

#[test]
fn python_is_found_by_any_of_its_manifests() {
    for manifest in [
        "pyproject.toml",
        "setup.cfg",
        "setup.py",
        "requirements.txt",
        "requirements-dev.txt",
    ] {
        let got = found(&[("Cargo.toml", MANIFEST), (manifest, ""), ("app.py", "")]);
        // setup.py is itself a Python source file, so it counts as one.
        let files = if manifest == "setup.py" { 2 } else { 1 };
        assert_eq!(got, vec![("python".to_string(), files)], "{manifest}");
    }
}

#[test]
fn a_file_that_git_does_not_track_is_not_counted() {
    // The project is a git repo with one tracked .ts file; an untracked one on disk must not be counted.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("package.json", "{}\n"),
        ("a.ts", ""),
    ]);
    std::fs::write(repo.path().join("untracked.ts"), "").unwrap();
    let got = unchecked_languages(repo.path());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].source_files, 1);
}

#[test]
fn a_directory_that_is_not_a_git_repository_reports_nothing_rather_than_walking_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();
    std::fs::write(dir.path().join("a.ts"), "").unwrap();
    assert!(unchecked_languages(dir.path()).is_empty());
}

#[test]
fn both_languages_are_reported_in_a_stable_order() {
    let got = found(&[
        ("Cargo.toml", MANIFEST),
        ("package.json", "{}\n"),
        ("a.ts", ""),
        ("pyproject.toml", ""),
        ("b.py", ""),
        ("c.py", ""),
    ]);
    assert_eq!(
        got,
        vec![("python".to_string(), 2), ("typescript".to_string(), 1)]
    );
}
