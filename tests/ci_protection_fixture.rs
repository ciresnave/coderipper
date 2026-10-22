// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ci-protection-presence` through the conformance runner, with a stub forge: the one rule that had no seeded defect and clean
//! twin because it asks GitHub, and a fixture cannot make GitHub answer. The fixtures under `tests/fixtures/forge/` name an
//! `origin` (an `expect.toml` key), and the check here is built over a [`Github`] that answers for those two repositories from
//! the real response shapes of `tests/ci_protection.rs`. No network is involved.
//!
//! This is NOT a claim the built-in module can make for itself: a real run of the same fixtures would ask the real GitHub about
//! `acme/unprotected`. So the Rust module's `proof` for this rule stays unset, and this test is where the rule is exercised
//! end to end (fixture, runner, module, check, finding location, clean twin).

use coderipper::check::Check;
use coderipper::checks::CiProtectionPresenceCheck;
use coderipper::conformance::{run, Options, Verdict};
use coderipper::github::{ApiError, Github};
use coderipper::module::RustModule;
use serde_json::{json, Value};
use std::path::Path;

/// What a stub forge says about the two fixture repositories: `acme/protected` requires three contexts,
/// `acme/unprotected` enforces nothing. `swap` serves each repository the other's answer (the non-vacuity control).
struct Forge {
    swap: bool,
}

fn branch(protected: bool) -> Value {
    if protected {
        json!({
            "name": "main",
            "commit": { "sha": "627f001d0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
            "protected": true,
            "protection": {
                "enabled": true,
                "required_status_checks": {
                    "contexts": ["build (ubuntu)", "build (windows)", "build (macos)"],
                    "checks": [],
                    "enforcement_level": "non_admins"
                }
            }
        })
    } else {
        json!({
            "name": "main",
            "commit": { "sha": "b69e34e4eaf803ea4b34405518805cc32b6814ce" },
            "protected": false,
            "protection": {
                "enabled": false,
                "required_status_checks": { "enforcement_level": "off", "contexts": [], "checks": [] }
            }
        })
    }
}

impl Github for Forge {
    fn get(&self, path: &str) -> Result<Value, ApiError> {
        let protected_repo = path.starts_with("repos/acme/protected");
        let unprotected_repo = path.starts_with("repos/acme/unprotected");
        if !(protected_repo || unprotected_repo) {
            return Err(ApiError::new(
                Some(404),
                format!("no canned answer for {path}"),
            ));
        }
        let serves_protection = protected_repo != self.swap;
        let rest = path.split_once('/').unwrap().1.split_once('/').unwrap().1;
        let rest = rest.split_once('/').map_or("", |(_, r)| r);
        match rest {
            "" => Ok(json!({ "default_branch": "main", "archived": false, "private": false })),
            "branches/main" => Ok(branch(serves_protection)),
            r if r.starts_with("rules/branches/") => Ok(json!([])),
            other => Err(ApiError::new(
                Some(404),
                format!("no canned answer for {other}"),
            )),
        }
    }
}

fn verdict(swap: bool) -> Verdict {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/forge");
    let checks: Vec<Box<dyn Check>> = vec![Box::new(CiProtectionPresenceCheck::with_api(
        Box::new(Forge { swap }),
    ))];
    let module = RustModule::new(&checks);
    let conformance = run(
        &module,
        &Options::new(fixtures)
            .rule("ci-protection-presence")
            .network(true),
    )
    .unwrap();
    assert_eq!(conformance.rules.len(), 1, "{conformance:?}");
    conformance.rules.into_iter().next().unwrap().verdict
}

#[test]
fn the_unprotected_repository_is_reported_and_the_protected_twin_is_clean() {
    assert_eq!(verdict(false), Verdict::Proven);
}

#[test]
fn the_same_fixtures_fail_when_the_forge_answers_the_other_way_round() {
    // the control that the proof above is not vacuous: protection served to the defective repository and none to the clean
    // twin must contradict both fixtures, not pass
    assert!(
        matches!(verdict(true), Verdict::Failed(_)),
        "swapped answers must fail the claim"
    );
}

#[test]
fn a_fixture_without_an_origin_is_an_error_not_a_pass() {
    // The check cannot see which repository it is in, so it errors; the runner reports that as Errored, never Proven.
    let fixtures = tempfile::tempdir().unwrap();
    for kind in ["defective", "clean"] {
        let dir = fixtures.path().join("ci-protection-presence").join(kind);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("README.md"), "x\n").unwrap();
        let expect = if kind == "defective" {
            "needs_network = true\n[[expect]]\nrule = \"ci-protection-presence\"\nfile = \"github:branch-protection\"\n"
        } else {
            "needs_network = true\n"
        };
        std::fs::write(dir.join("expect.toml"), expect).unwrap();
    }
    let checks: Vec<Box<dyn Check>> = vec![Box::new(CiProtectionPresenceCheck::with_api(
        Box::new(Forge { swap: false }),
    ))];
    let module = RustModule::new(&checks);
    let result = run(
        &module,
        &Options::new(fixtures.path())
            .rule("ci-protection-presence")
            .network(true),
    )
    .unwrap();
    assert!(
        matches!(result.rules[0].verdict, Verdict::Errored(_)),
        "{:?}",
        result.rules[0].verdict
    );
}
