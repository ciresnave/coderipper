//! The README and the package metadata are what a stranger reads first on crates.io. These tests keep them honest about
//! what the crate does: every check is described, nothing unbuilt is promised, and no link points at a file the
//! published package does not contain.

const README: &str = include_str!("../README.md");
const CARGO_TOML: &str = include_str!("../Cargo.toml");

#[test]
fn every_registered_check_is_described() {
    let checks = coderipper::registered_checks();
    assert!(!checks.is_empty());
    for check in checks {
        assert!(
            README.contains(check.id()),
            "the README does not mention the check `{}`",
            check.id()
        );
    }
}

#[test]
fn the_readme_promises_nothing_that_is_not_built() {
    // Each of these was in the README or the metadata while the thing it named did not exist.
    for stale in [
        "stale dependencies",
        "dependency staleness",
        "coderipper serve",
        "three checks",
    ] {
        assert!(!README.contains(stale), "the README still says {stale:?}");
    }
    let description = CARGO_TOML
        .lines()
        .find(|l| l.starts_with("description"))
        .expect("a description");
    for stale in ["staleness", "server"] {
        assert!(
            !description.contains(stale),
            "the description still says {stale:?}: {description}"
        );
    }
}

#[test]
fn no_link_points_into_docs_which_the_package_does_not_contain() {
    assert!(
        !README.contains("docs/superpowers"),
        "docs/ is excluded from the published package, so a link into it is dead on crates.io"
    );
    assert!(CARGO_TOML.contains("exclude"), "docs/ must be excluded");
    assert!(CARGO_TOML.contains("\"docs/\""));
}

#[test]
fn the_exit_codes_and_the_deny_flag_are_documented() {
    for needle in [
        "## Exit codes",
        "--deny medium",
        "--message-format json",
        "cargo coderipper",
        "default-features = false",
    ] {
        assert!(
            README.contains(needle),
            "the README does not mention {needle:?}"
        );
    }
}

#[test]
fn the_package_declares_its_minimum_rust_version() {
    // 1.89 is where `File::try_lock` (the build cache's advisory lock) was stabilised; CI's MSRV job proves it builds.
    assert!(CARGO_TOML.contains("rust-version = \"1.89\""));
}

#[test]
fn public_docs_do_not_point_at_what_a_crates_io_reader_cannot_open() {
    // These files' doc comments are rendered on docs.rs; the design docs and plans are in the source repository only, and
    // `suppression` is a private module.
    for (name, text) in [
        ("src/lib.rs", include_str!("../src/lib.rs")),
        ("src/check.rs", include_str!("../src/check.rs")),
        (
            "src/checks/reachability/mod.rs",
            include_str!("../src/checks/reachability/mod.rs"),
        ),
        (
            "src/checks/unused_parameters/mod.rs",
            include_str!("../src/checks/unused_parameters/mod.rs"),
        ),
        (
            "src/checks/unused_return_values/mod.rs",
            include_str!("../src/checks/unused_return_values/mod.rs"),
        ),
    ] {
        for stale in ["design doc", "see `suppression`", "docs/superpowers"] {
            let hits: Vec<&str> = text
                .lines()
                .filter(|l| {
                    (l.trim_start().starts_with("//!") || l.trim_start().starts_with("///"))
                        && l.contains(stale)
                })
                .collect();
            assert!(hits.is_empty(), "{name} documents {stale:?}: {hits:?}");
        }
    }
}

#[test]
fn the_readme_history_lives_in_the_changelog_and_the_library_section_explains_the_cache() {
    assert!(
        !README.contains("Fixed in 0.2.9"),
        "a bug fixed in a version that was never published is changelog material, not a reader's concern"
    );
    assert!(
        README.contains("set_cache_config"),
        "a library user gets no cache unless they ask for one"
    );
    assert!(
        README.contains("unknown check id"),
        "the exit-code table says what an unknown check id does"
    );
}

#[test]
fn the_newest_changelog_entry_is_the_version_in_cargo_toml() {
    let version = CARGO_TOML
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|v| v.strip_suffix('"'))
        .expect("a version");
    let changelog = include_str!("../CHANGELOG.md");
    let newest = changelog
        .lines()
        .find_map(|l| l.strip_prefix("## "))
        .expect("a changelog entry");
    assert!(
        newest.starts_with(&format!("{version} ")),
        "the newest CHANGELOG heading is {newest:?}, but Cargo.toml says {version}"
    );
    let mut parts = version.split('.');
    let series = format!("{}.{}", parts.next().unwrap(), parts.next().unwrap());
    assert!(
        README.contains(&format!("version = \"{series}\"")),
        "the README's dependency line does not name the {series} series"
    );
}
