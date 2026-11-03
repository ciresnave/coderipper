// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn facts(api: &[(&str, usize)], tests: &[&str]) -> Facts {
    Facts {
        api: api.iter().map(|(f, l)| ((*f).to_string(), *l)).collect(),
        test_idents: tests.iter().map(|s| (*s).to_string()).collect(),
    }
}

#[test]
fn a_documented_public_item_is_library_api_and_the_same_item_unwritten_pub_is_not() {
    let f = facts(&[("src/lib.rs", 4)], &[]);
    assert_eq!(
        classify(&f, "src/lib.rs", 4, "foo", true),
        Class::LibraryApi
    );
    // positive control: the same site without having been written `pub`, or on another line, is plain
    assert_eq!(classify(&f, "src/lib.rs", 4, "foo", false), Class::Plain);
    assert_eq!(classify(&f, "src/lib.rs", 5, "foo", true), Class::Plain);
    assert_eq!(classify(&f, "src/other.rs", 4, "foo", true), Class::Plain);
}

#[test]
fn a_name_written_in_test_code_is_test_only_and_library_api_takes_precedence() {
    let f = facts(&[("src/lib.rs", 4)], &["foo"]);
    assert_eq!(classify(&f, "src/lib.rs", 9, "foo", false), Class::TestOnly);
    assert_eq!(classify(&f, "src/lib.rs", 9, "bar", false), Class::Plain);
    assert_eq!(
        classify(&f, "src/lib.rs", 4, "foo", true),
        Class::LibraryApi
    );
}

fn git_repo(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (p, t) in files {
        let path = dir.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    }
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    dir
}

#[test]
fn the_api_sites_are_the_documented_items_reachable_through_pub_modules() {
    let repo = git_repo(&[
        (
            "src/lib.rs",
            "/// Documented.\npub fn documented() {}\npub fn undocumented() {}\n/// In a private module.\nmod hidden { /// d\n pub fn inside() {} }\n/// A type.\npub struct S;\nimpl S {\n    /// A method.\n    pub fn m(&self) {}\n}\n",
        ),
    ]);
    let f = read_facts(repo.path(), true).unwrap();
    let lines: Vec<usize> = f.api.iter().map(|(_, l)| *l).collect();
    // `documented` (2), `S` (8) and `S::m` (11); not `undocumented` (3) and not `inside` (6)
    assert_eq!(lines, [2, 8, 11], "{:?}", f.api);
    // a package without a library has no public surface to speak of
    assert!(read_facts(repo.path(), false).unwrap().api.is_empty());
}

#[test]
fn test_idents_come_from_cfg_test_items_and_test_files_and_not_from_the_library() {
    let repo = git_repo(&[
        (
            "src/lib.rs",
            "mod tests;\npub fn plain() {}\nfn only_named_in_prod() {}\n#[cfg(test)]\nmod inline_tests { fn t() { from_inline(); } }\n#[cfg(test)]\nfn test_helper() { from_cfg_fn(); }\n",
        ),
        ("src/tests.rs", "fn t() { from_tests_file(); }\n"),
        ("src/foo_test.rs", "fn t() { from_suffix_file(); }\n"),
    ]);
    let f = read_facts(repo.path(), true).unwrap();
    for name in [
        "from_inline",
        "from_cfg_fn",
        "from_tests_file",
        "from_suffix_file",
        "test_helper",
    ] {
        assert!(f.test_idents.contains(name), "{name}: {:?}", f.test_idents);
    }
    // names that only the library writes are not test identifiers
    for name in ["plain", "only_named_in_prod"] {
        assert!(!f.test_idents.contains(name), "{name}");
    }
}
