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
