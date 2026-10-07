//! The library's public API as an OUTSIDE crate sees it.
//!
//! An integration test is a separate crate, so it can do only what a user of the published library can do. The
//! types below are `#[non_exhaustive]` (a field or variant can be added later without breaking anyone), which means an
//! outsider cannot write a struct literal or an exhaustive `match` for them: they build values through constructors
//! and match enums with a wildcard arm. This file is that contract, and the `compile_fail` doctests in the type docs
//! prove the other half (that the literal and the exhaustive match really are rejected).

use coderipper::build_cache::CacheConfig;
use coderipper::check::{CheckContext, Network, Scope, Tier, Unit};
use coderipper::finding::{Confidence, Finding, FindingError, Location, Severity};
use coderipper::github::ApiError;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn a_finding_is_built_through_its_constructor_and_setters() {
    let finding = Finding::new(
        "my-check",
        Severity::Low,
        Confidence::High,
        "proj",
        "a thing",
        "why it matters",
    )
    .location(Location::new("src/lib.rs", Some(3)))
    .subject("sym")
    .positive_control("the control that proves the claim")
    .member("pkg");
    assert_eq!(finding.check_id, "my-check");
    assert_eq!(finding.severity, Severity::Low);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.project, "proj");
    assert_eq!(finding.summary, "a thing");
    assert_eq!(finding.detail, "why it matters");
    assert_eq!(finding.location, Some(Location::new("src/lib.rs", Some(3))));
    assert_eq!(finding.subject.as_deref(), Some("sym"));
    assert_eq!(
        finding.positive_control.as_deref(),
        Some("the control that proves the claim")
    );
    assert_eq!(finding.member.as_deref(), Some("pkg"));
}

#[test]
fn a_new_finding_has_no_optional_parts() {
    let finding = Finding::new("c", Severity::Info, Confidence::Low, "p", "s", "d");
    assert!(finding.location.is_none());
    assert!(finding.subject.is_none());
    assert!(finding.positive_control.is_none());
    assert!(finding.member.is_none());
}

#[test]
fn a_location_may_have_no_line() {
    let whole_project = Location::new("Cargo.toml", None);
    assert_eq!(whole_project.file, "Cargo.toml");
    assert_eq!(whole_project.line, None);
}

#[test]
fn a_check_context_takes_anything_path_like() {
    let a = CheckContext::new("/some/project", "/some");
    assert_eq!(a.project_root, PathBuf::from("/some/project"));
    assert_eq!(a.portfolio_root, PathBuf::from("/some"));
    let b = CheckContext::new(PathBuf::from("p"), std::path::Path::new("q"));
    assert_eq!(b.project_root, PathBuf::from("p"));
    assert_eq!(b.portfolio_root, PathBuf::from("q"));
}

#[test]
fn a_cache_config_has_defaults_and_setters() {
    let config = CacheConfig::new("/cache");
    assert_eq!(config.root, PathBuf::from("/cache"));
    assert_eq!(config.wait, Duration::from_secs(5), "the documented wait");
    assert_eq!(
        config.max_bytes,
        20 * (1u64 << 30),
        "the documented 20 GB cap"
    );
    let tuned = CacheConfig::new("/cache")
        .wait(Duration::from_secs(1))
        .max_bytes(1024);
    assert_eq!(tuned.wait, Duration::from_secs(1));
    assert_eq!(tuned.max_bytes, 1024);
}

#[test]
fn an_api_error_is_built_for_a_fake_and_displays_its_status() {
    let with_status = ApiError::new(Some(404), "not found");
    assert_eq!(with_status.status, Some(404));
    assert_eq!(with_status.to_string(), "not found (HTTP 404)");
    let without = ApiError::new(None, "no gh on this machine");
    assert_eq!(without.to_string(), "no gh on this machine");
}

#[test]
fn enums_are_matched_with_a_wildcard_arm() {
    // `#[non_exhaustive]`: a variant added later must not break this match, so the compiler demands the `_` arm.
    fn severity(s: Severity) -> &'static str {
        match s {
            Severity::High => "high",
            _ => "other",
        }
    }
    fn confidence(c: Confidence) -> &'static str {
        match c {
            Confidence::High => "high",
            _ => "other",
        }
    }
    fn scope(s: Scope) -> &'static str {
        match s {
            Scope::Project => "project",
            _ => "other",
        }
    }
    fn unit(u: Unit) -> &'static str {
        match u {
            Unit::Package => "package",
            _ => "other",
        }
    }
    fn network(n: Network) -> &'static str {
        match n {
            Network::LocalOnly => "local",
            _ => "other",
        }
    }
    fn tier(t: Tier) -> &'static str {
        match t {
            Tier::Fast => "fast",
            _ => "other",
        }
    }
    fn finding_error(e: &FindingError) -> &'static str {
        match e {
            FindingError::AbsenceClaimMissingControl { .. } => "absence",
            _ => "other",
        }
    }
    assert_eq!(severity(Severity::High), "high");
    assert_eq!(confidence(Confidence::Low), "other");
    assert_eq!(scope(Scope::Project), "project");
    assert_eq!(unit(Unit::Repository), "other");
    assert_eq!(network(Network::NetworkRequired), "other");
    assert_eq!(tier(Tier::Fast), "fast");
    let invalid = Finding::new("c", Severity::Low, Confidence::Low, "p", "no callers", "d");
    let err = invalid.validate().unwrap_err();
    assert_eq!(finding_error(&err), "absence");
}

fn assert_send_sync<T: Send + Sync + ?Sized>() {}

#[test]
fn checks_and_github_clients_can_cross_threads() {
    // A hosted service, a thread pool or an async runtime needs this; adding the bounds after publishing would break
    // every implementer that holds an Rc or a RefCell, so they are part of the traits from the first release.
    assert_send_sync::<dyn coderipper::check::Check>();
    assert_send_sync::<dyn coderipper::github::Github>();
    assert_send_sync::<Box<dyn coderipper::check::Check>>();
    assert_send_sync::<coderipper::RunResult>();
    assert_send_sync::<Finding>();
    assert_send_sync::<CheckContext>();
}

#[test]
fn the_built_in_checks_are_values_you_can_hold_and_print() {
    let all = coderipper::registered_checks();
    assert_send_sync::<Vec<Box<dyn coderipper::check::Check>>>();
    assert!(!all.is_empty());
    let ctx = CheckContext::new("a", "b");
    let copy = ctx.clone();
    assert_eq!(format!("{ctx:?}"), format!("{copy:?}"));
}

#[test]
fn results_and_findings_have_the_ordinary_derives() {
    use std::collections::HashSet;
    let finding = Finding::new("c", Severity::Low, Confidence::High, "p", "s", "d");
    assert_eq!(finding.clone(), finding, "Finding is PartialEq + Clone");
    let result = coderipper::RunResult::new(vec![finding.clone()], vec!["e".to_string()]);
    let again = result.clone();
    assert_eq!(format!("{result:?}"), format!("{again:?}"));
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.errors, ["e"]);
    let severities: HashSet<Severity> = [Severity::Low, Severity::Low, Severity::High].into();
    assert_eq!(severities.len(), 2, "Severity is Hash");
    let confidences: HashSet<Confidence> = [Confidence::Low, Confidence::High].into();
    assert_eq!(confidences.len(), 2, "Confidence is Hash");
}
