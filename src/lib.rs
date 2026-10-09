//! CodeRipper audits a Rust project for integration defects that the compiler and clippy do not report: public
//! functions nothing calls, return values every caller discards, parameters nothing reads, crates in one project
//! that disagree on their version, and a repository whose default branch enforces no CI.
//!
//! It is one engine with three faces:
//!
//! - **A library** (this crate): run the built-in checks with [`run_checks`], or your own [`check::Check`] through the
//!   same host with [`run_checks_with`].
//! - **A command**, `coderipper`, and **a cargo subcommand**, `cargo coderipper`, which run next to `cargo clippy`.
//!   They are built by the default `cli` feature; a library user writes `default-features = false` and compiles
//!   neither them nor `clap`. The command is documented in the README, not here: its code is not API.
//! - **A hosted service** is planned; nothing of it is in this crate.
//!
//! # Running the built-in checks
//!
//! ```no_run
//! use coderipper::check::{CheckContext, Tier};
//! use coderipper::finding::Severity;
//!
//! // The project must be a git repository with a commit: the checks analyse HEAD in a throwaway worktree and never
//! // touch your working tree.
//! let ctx = CheckContext::new("path/to/project");
//! let result = coderipper::run_checks(&ctx, Tier::Fast, None);
//! for finding in &result.findings {
//!     println!("{:?} {} ({})", finding.severity, finding.summary, finding.check_id);
//! }
//! // A check that could not run is an entry in `errors`, never a silent absence of findings.
//! assert!(result.errors.is_empty());
//! let worst = result.findings.iter().map(|f| f.severity).max();
//! assert!(worst.is_none_or(|s| s < Severity::High));
//! ```
//!
//! # Writing a check
//!
//! Implement [`check::Check`] and run it with [`run_checks_with`]. The host validates every finding (a claim of
//! absence needs a `positive_control`: see [`finding::Finding::validate`]), applies the project's `.coderipper.toml`
//! allowlist, and reports allowlist entries that no longer match anything. See [`run_checks_with`] for a complete,
//! runnable example.
//!
//! # Stability
//!
//! The traits [`check::Check`] and [`github::Github`] are `Send + Sync`, and a method added to either later will have a
//! default body, so an existing implementation keeps compiling. Types you receive from this crate (`Finding`, `Severity`, `CheckContext`, ...) are `#[non_exhaustive]`, so a field
//! or a variant can be added in a minor release without breaking you: build them with their constructors and match
//! enums with a wildcard arm. The JSON the command prints (`--message-format json`) is a separate contract, pinned by
//! golden tests.
//!
//! The crate re-exports [`anyhow`] and [`serde_json`], whose types appear in the signatures of [`check::Check::run`] and
//! [`github::Github::get`], so an implementer uses the version this crate was built with. A new major version of either
//! is a new major version of this crate.
//!
//! # Features
//!
//! - `cli` (default): the `coderipper` and `cargo-coderipper` binaries.
#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
pub use anyhow;
pub use serde_json;

pub(crate) mod allowlist;
pub mod build_cache;
pub(crate) mod cargo_json;
pub mod catalog;
pub mod check;
pub mod checks;
#[cfg(feature = "cli")]
#[doc(hidden)]
pub mod cli;
pub mod conformance;
pub mod coverage;
pub mod finding;
pub mod github;
pub mod module;
pub(crate) mod package;
pub(crate) mod session;
pub(crate) mod suppression;
pub(crate) mod worktree;

use allowlist::Allowlist;
use check::{Check, CheckContext, Tier};
use finding::Finding;
use suppression::Suppression;

/// Every compiled-in check, in the order they run.
///
/// The list is fixed rather than loaded dynamically: there is no known need yet for a check this project's own
/// maintainers didn't write. To run your own, pass a list to [`run_checks_with`].
pub fn registered_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(checks::ReachabilityCheck::new()),
        Box::new(checks::UnusedReturnValuesCheck::new()),
        Box::new(checks::UnusedParametersCheck::new()),
        Box::new(checks::VersionConsistencyCheck::new()),
        Box::new(checks::CiProtectionPresenceCheck::new()),
    ]
}

/// Run every registered check at or below the requested tier, collect and validate their
/// findings, apply the project's allowlist to them (checks return RAW findings; suppression is the
/// host's job), and return what is left plus one `Info` finding per allowlist
/// entry that no longer suppresses anything. A check whose `run` returns an invalid absence-claim finding is
/// dropped with an error noted in `errors`, not silently included — see `Finding::validate`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RunResult {
    /// The validated findings that the allowlist did not suppress, plus the allowlist's own `Info` findings.
    pub findings: Vec<Finding>,
    /// What went wrong: a check that failed to run, a finding the host rejected as invalid, an unreadable allowlist.
    /// A non-empty list means the audit is incomplete, whatever `findings` says.
    pub errors: Vec<String>,
    /// How each rule that was asked for fared: clean, findings, skipped (did not apply) or could not run. This is what the
    /// coverage report's "this run" counts come from; it is empty for a result built with [`RunResult::new`].
    pub outcomes: Vec<coverage::RuleRun>,
}

impl RunResult {
    /// A result with these findings and errors: for a wrapper or a test double that must return one.
    pub fn new(findings: Vec<Finding>, errors: Vec<String>) -> Self {
        Self {
            findings,
            errors,
            outcomes: Vec::new(),
        }
    }
}

/// Which checks a run takes, by what they judge (see [`check::Unit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitFilter {
    /// Every check, whatever it judges (what `run_checks` always did).
    Any,
    /// Only the checks that judge this unit.
    Only(check::Unit),
}

/// What a `--workspace` run did, beyond its findings and errors.
#[non_exhaustive]
pub struct WorkspaceRun {
    /// Every member's findings (each marked with its [`Finding::member`](finding::Finding::member)) and every error.
    pub result: RunResult,
    /// Members analysed (a poisoned session stops the loop early).
    pub members: usize,
    /// Members whose run reported at least one error.
    pub members_with_errors: usize,
}

/// Runs the checks over every member of the cargo workspace containing `ctx.project_root` (the root or any member).
///
/// Checks that judge the whole repository ([`check::Unit::Repository`]) run ONCE, at the workspace root. Checks that
/// judge a package run once per member, in sorted order, inside one session checkout (see `session`), each exactly as
/// `--project <member>` runs them: the member's own `.coderipper.toml`, the same stale-entry judgement. Every finding
/// from a member carries that member's package name. An error in one member does not stop the others; a session
/// whose restore failed does (nothing is analysed in a dirty checkout). `on_member(name, i, n)` is told as each member
/// starts, so a long run can show where it is.
pub fn run_workspace(
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    on_member: &mut dyn FnMut(&str, usize, usize),
) -> WorkspaceRun {
    run_workspace_in(Profile::Classic, ctx, tier, only_check_id, on_member)
}

/// Which rules a run takes (the CLI's `--profile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Profile {
    /// The five original checks and nothing else: what [`run_checks`] and [`run_workspace`] always ran.
    Classic,
    /// `Classic`, plus the rules of the built-in language-neutral module ([`module::NeutralModule`]). Those rules judge the
    /// whole repository, so a workspace run takes them once, at the workspace root.
    Extended,
}

impl Profile {
    /// The rule ids this profile adds to the five original checks.
    fn extra_rules(self) -> Vec<&'static str> {
        match self {
            Profile::Classic => Vec::new(),
            Profile::Extended => module::neutral_rule_ids(),
        }
    }
}

/// [`run_workspace`] under a profile.
pub fn run_workspace_in(
    profile: Profile,
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    on_member: &mut dyn FnMut(&str, usize, usize),
) -> WorkspaceRun {
    run_workspace_over_in(
        profile,
        &registered_checks(),
        ctx,
        tier,
        only_check_id,
        on_member,
    )
}

fn run_workspace_over_in(
    profile: Profile,
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    on_member: &mut dyn FnMut(&str, usize, usize),
) -> WorkspaceRun {
    let failed = |message: String| WorkspaceRun {
        result: RunResult::new(Vec::new(), vec![message]),
        members: 0,
        members_with_errors: 0,
    };
    // before visiting any member: a wrong id would otherwise repeat its error once per member
    if let Some(problem) = check_list_problem(checks, &profile.extra_rules(), only_check_id) {
        return failed(problem);
    }
    let (root, members) = match package::workspace_members(&ctx.project_root) {
        Ok(found) => found,
        Err(e) => return failed(e.to_string()),
    };

    let mut findings = Vec::new();
    let mut errors = Vec::new();
    let mut outcomes: Vec<coverage::RuleRun> = Vec::new();
    // The same allowlist file can be judged twice (a root package is both the repository's unit and a member): a stale
    // or unknown-check entry is reported once.
    let mut judged = std::collections::HashSet::new();

    let repository = run_checks_over_in(
        profile,
        checks,
        &CheckContext {
            project_root: root.clone(),
            portfolio_root: ctx.portfolio_root.clone(),
        },
        tier,
        only_check_id,
        UnitFilter::Only(check::Unit::Repository),
    );
    for finding in repository.findings {
        if finding.check_id != suppression::ALLOWLIST_CHECK_ID
            || judged.insert(allowlist_key(&finding))
        {
            findings.push(finding);
        }
    }
    errors.extend(repository.errors);
    coverage::merge_runs(&mut outcomes, repository.outcomes);

    let session = match session::Session::open(&root) {
        Ok(session) => session,
        Err(e) => {
            errors.push(format!("cannot open the workspace session: {e}"));
            return WorkspaceRun {
                result: RunResult::new(findings, errors),
                members: 0,
                members_with_errors: 0,
            };
        }
    };

    let total = members.len();
    let mut analysed = 0;
    let mut with_errors = 0;
    session::with_session(&session, || {
        for (index, member) in members.iter().enumerate() {
            on_member(&member.name, index + 1, total);
            let run = run_checks_over_in(
                profile,
                checks,
                &CheckContext {
                    project_root: member.dir.clone(),
                    portfolio_root: ctx.portfolio_root.clone(),
                },
                tier,
                only_check_id,
                UnitFilter::Only(check::Unit::Package),
            );
            analysed += 1;
            coverage::merge_runs(&mut outcomes, run.outcomes);
            if !run.errors.is_empty() {
                with_errors += 1;
            }
            for mut finding in run.findings {
                if finding.check_id == suppression::ALLOWLIST_CHECK_ID
                    && !judged.insert(allowlist_key(&finding))
                {
                    continue;
                }
                finding.member = Some(member.name.clone());
                findings.push(finding);
            }
            // when a restore failed for good the session's message replaces the member's own errors: it is the cause
            if let Some(why) = session.poison_message() {
                let rest: Vec<&str> = members[index + 1..]
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect();
                errors.push(format!(
                    "{}: {why}; not analysed: {}",
                    member.name,
                    if rest.is_empty() {
                        "(no more members)".to_string()
                    } else {
                        rest.join(", ")
                    }
                ));
                with_errors += 1;
                return;
            }
            errors.extend(
                run.errors
                    .into_iter()
                    .map(|e| format!("{}: {e}", member.name)),
            );
        }
    });

    WorkspaceRun {
        result: RunResult {
            findings,
            errors,
            outcomes,
        },
        members: analysed,
        members_with_errors: with_errors,
    }
}

/// What makes two allowlist-judgement findings the same one.
fn allowlist_key(finding: &Finding) -> (String, Option<String>, String) {
    (
        finding.project.clone(),
        finding.subject.clone(),
        finding.summary.clone(),
    )
}

/// Runs the built-in checks (see [`registered_checks`]) over the project at `ctx.project_root`: every check at or below
/// `tier`, or only the check named `only_check_id` (at any tier). See [`run_checks_with`] for what the host does with
/// what the checks return.
pub fn run_checks(ctx: &CheckContext, tier: Tier, only_check_id: Option<&str>) -> RunResult {
    run_checks_with(&registered_checks(), ctx, tier, only_check_id)
}

/// [`run_checks`] under a [`Profile`]: `Extended` also asks the language-neutral module for its rules, in the same run, so one
/// allowlist and one set of rule ids apply to all of them. `only_check_id` may then name one of those rules (`SUP-001`).
pub fn run_checks_in(
    profile: Profile,
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
) -> RunResult {
    run_checks_over_in(
        profile,
        &registered_checks(),
        ctx,
        tier,
        only_check_id,
        UnitFilter::Any,
    )
}

/// Runs the checks you pass, the way [`run_checks`] runs the built-in ones: through the host, not by calling
/// [`Check::run`] yourself.
///
/// What the host does that a direct `check.run(ctx)` does not:
/// - **Validates every finding** ([`finding::Finding::validate`]). A finding whose summary claims an absence (contains
///   the whole word "zero", "no", "none", "missing", "unreachable" or "0"; "10 threads" and "casino" do not count)
///   without a `positive_control` is dropped and reported in `errors`.
/// - **Applies the project's allowlist**, read from `.coderipper.toml` in `ctx.project_root`. An entry suppresses a
///   finding only when its `check`, `file` and `symbol` equal the finding's `check_id`, `location.file` and `subject`,
///   so a check that wants to be suppressible must set a location and a subject.
/// - **Judges stale entries against the checks you pass.** An entry that suppressed nothing is reported as an `Info`
///   finding from the check id `allowlist`, and an entry naming a check that is not in `checks` is reported as an
///   unknown check. If you pass only your own check and the project's `.coderipper.toml` names built-in checks, each
///   of those entries is reported as unknown: pass the built-in checks too, or use a project without such entries.
/// - **Keeps the tier rule.** With `only_check_id = None`, a [`check::Tier::Sweep`] check does not run at
///   [`check::Tier::Fast`]; naming it with `only_check_id` runs it at any tier.
///
/// An error from one check (it could not run) is recorded in `errors` and does not stop the others.
///
/// ```
/// use coderipper::check::{Check, CheckContext, Network, Scope, Tier};
/// use coderipper::finding::{Confidence, Finding, Location, Severity};
///
/// /// Reports a `TODO.md` in the project root.
/// struct TodoFile;
///
/// impl Check for TodoFile {
///     fn id(&self) -> &'static str { "todo-file" }
///     fn scope(&self) -> Scope { Scope::Project }
///     fn network(&self) -> Network { Network::LocalOnly }
///     fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
///         if !ctx.project_root.join("TODO.md").exists() {
///             return Ok(Vec::new());
///         }
///         Ok(vec![Finding::new(
///             "todo-file", Severity::Low, Confidence::High, "demo",
///             "the project keeps a TODO.md", "TODO.md belongs in the issue tracker",
///         )
///         .location(Location::new("TODO.md", None))
///         .subject("TODO.md")])
///     }
/// }
///
/// let dir = tempfile::tempdir().unwrap();
/// std::fs::write(dir.path().join("TODO.md"), "- later\n").unwrap();
/// let ctx = CheckContext::new(dir.path());
/// let result = coderipper::run_checks_with(&[Box::new(TodoFile)], &ctx, Tier::Fast, None);
/// assert!(result.errors.is_empty());
/// assert_eq!(result.findings.len(), 1);
/// ```
pub fn run_checks_with(
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
) -> RunResult {
    run_checks_over(checks, ctx, tier, only_check_id, UnitFilter::Any)
}

/// Why this list of checks and this `only_check_id` cannot be run, if they cannot: a named check that does not exist
/// (a typo must not read as a clean run) or two checks sharing an id (the id keys allowlist entries and `only_check_id`).
fn check_list_problem(
    checks: &[Box<dyn Check>],
    extra_rules: &[&str],
    only_check_id: Option<&str>,
) -> Option<String> {
    let mut ids: Vec<&str> = checks.iter().map(|c| c.id()).collect();
    if let Some(id) = ids
        .iter()
        .enumerate()
        .find_map(|(i, id)| ids[..i].contains(id).then_some(id))
    {
        return Some(format!(
            "the check id \"{id}\" is used by more than one check; ids must be unique"
        ));
    }
    ids.extend(extra_rules);
    let wanted = only_check_id?;
    (!ids.contains(&wanted)).then(|| {
        format!(
            "no check named \"{wanted}\"; the checks are: {}",
            ids.join(", ")
        )
    })
}

/// `run_checks` over an explicit list, so tests can drive the loop with fake checks.
fn run_checks_over(
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    units: UnitFilter,
) -> RunResult {
    run_checks_over_in(Profile::Classic, checks, ctx, tier, only_check_id, units)
}

fn run_checks_over_in(
    profile: Profile,
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    units: UnitFilter,
) -> RunResult {
    let extra = profile.extra_rules();
    if let Some(problem) = check_list_problem(checks, &extra, only_check_id) {
        return RunResult::new(Vec::new(), vec![problem]);
    }
    // The checks to run, decided here (the host chooses what is in scope), then asked of the Rust module as one request.
    let mut selected: Vec<String> = checks
        .iter()
        .filter(|check| match units {
            UnitFilter::Only(unit) => check.unit() == unit,
            UnitFilter::Any => true,
        })
        .filter(|check| match only_check_id {
            Some(id) => check.id() == id,
            // Sweep mode runs everything; fast mode runs only fast-tier checks.
            None => !(check.tier() != tier && tier == Tier::Fast),
        })
        .map(|check| check.id().to_string())
        .collect();
    let unit = match units {
        UnitFilter::Only(unit) => Some(unit),
        UnitFilter::Any => None,
    };
    // The neutral rules judge the whole repository, so they run in the repository pass (and in a plain run), once.
    if !matches!(units, UnitFilter::Only(check::Unit::Package)) {
        selected.extend(
            extra
                .iter()
                .filter(|id| only_check_id.is_none_or(|only| only == **id))
                .map(|id| (*id).to_string()),
        );
    }
    let mut registered: Vec<&str> = checks.iter().map(|c| c.id()).collect();
    registered.extend(&extra);
    let rust = module::RustModule::new(checks);
    let neutral = module::NeutralModule::new();
    let composite = module::Composite::new(vec![&rust, &neutral]);
    let module: &dyn module::Module = match profile {
        Profile::Classic => &rust,
        _ => &composite,
    };
    run_module_over(
        module,
        ctx,
        tier,
        unit,
        selected,
        &registered,
        module::Limits::default(),
    )
}

/// Asks `module` to check `rules` and turns what it says into a [`RunResult`]: the findings are validated and
/// suppressed by the allowlist exactly as a built-in check's are, and a rule that gave no verdict is an entry in `errors`.
/// `registered` is every rule id the allowlist may name (for the stale-entry judgement).
fn run_module_over(
    module: &dyn module::Module,
    ctx: &CheckContext,
    tier: Tier,
    unit: Option<check::Unit>,
    rules: Vec<String>,
    registered: &[&str],
    limits: module::Limits,
) -> RunResult {
    let mut findings = Vec::new();
    let mut errors = Vec::new();
    let mut outcomes = Vec::new();

    // A malformed allowlist must not swallow the findings: report it, and run unsuppressed.
    let allowlist = Allowlist::load(&ctx.project_root).unwrap_or_else(|e| {
        errors.push(format!("allowlist: {e}"));
        Allowlist::empty()
    });
    let mut suppression = Suppression::new(&allowlist);

    let request = module::Request::new(ctx, unit, tier, rules, limits);
    let reconciled = module::reconcile(&request, module.check(&request));

    for outcome in reconciled.rules {
        let skipped = matches!(outcome.status, module::RuleStatus::Skipped { .. });
        let completed = match &outcome.status {
            module::RuleStatus::Ran => true,
            module::RuleStatus::Skipped { .. } => false,
            module::RuleStatus::Error { kind, detail } => {
                errors.push(match kind {
                    module::ErrorKind::Internal => {
                        format!("{} failed to run: {detail}", outcome.rule)
                    }
                    kind => format!("{} failed to run ({kind}): {detail}", outcome.rule),
                });
                false
            }
        };
        let mut valid = Vec::new();
        let mut all_valid = true;
        for f in outcome.findings {
            match f.validate() {
                Ok(f) => valid.push(f),
                Err(e) => {
                    all_valid = false;
                    errors.push(format!("{}: {e}", outcome.rule));
                }
            }
        }
        // A check that produced an invalid finding, or did not complete, is not trusted to have completed, so its
        // allowlist entries are not judged either.
        if completed && all_valid {
            suppression.mark_completed(&outcome.rule, valid.len());
        }
        let kept = suppression.apply(valid);
        outcomes.push(coverage::RuleRun::new(
            &outcome.rule,
            if skipped {
                coverage::RunOutcome::Skipped
            } else if !(completed && all_valid) {
                coverage::RunOutcome::CouldNotRun
            } else if kept.is_empty() {
                coverage::RunOutcome::Clean
            } else {
                coverage::RunOutcome::Findings(kept.len())
            },
        ));
        findings.extend(kept);
    }
    errors.extend(reconciled.run_errors);

    let project = ctx
        .project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    findings.extend(suppression.stale_findings(registered, &project));

    RunResult {
        findings,
        errors,
        outcomes,
    }
}

/// Runs a [`module::Module`] (an [`module::ExternalModule`] over a child process, or your own) over the project at
/// `ctx.project_root`, the way [`run_checks`] runs the built-in checks: findings are validated and the project's
/// allowlist applied, and a rule that gave no verdict (the module crashed, hung, printed too much, printed garbage, or
/// said nothing about it) is an entry in `errors`, never a silent absence of findings.
///
/// A rule the module skips as not applicable is a coverage gap, not a failure: it adds neither a finding nor an error, and
/// this result cannot yet say which rules were skipped (the coverage report, a later phase, will).
///
/// It asks for every rule the module's hello claims, at `tier`. Choosing rules by tier and unit needs the rule catalog,
/// which is not built yet.
pub fn run_module(
    module: &dyn module::Module,
    ctx: &CheckContext,
    tier: Tier,
    limits: module::Limits,
) -> RunResult {
    let hello = match module.describe() {
        Ok(hello) => hello,
        Err(e) => return RunResult::new(Vec::new(), vec![format!("the module's hello: {e}")]),
    };
    if let Some(problem) = hello.problem() {
        return RunResult::new(Vec::new(), vec![format!("the module's hello: {problem}")]);
    }
    let claimed: Vec<&str> = hello
        .rules
        .iter()
        .filter(|r| r.status != "not-applicable")
        .map(|r| r.id.as_str())
        .collect();
    let registered: Vec<&str> = hello.rules.iter().map(|r| r.id.as_str()).collect();
    run_module_over(
        module,
        ctx,
        tier,
        None,
        claimed.iter().map(|id| id.to_string()).collect(),
        &registered,
        limits,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_check_is_actually_registered() {
        // Review finding: this test used to run the real ReachabilityCheck against
        // std::env::current_dir() (the shared repo itself) and assert zero findings/errors --
        // which only passed because that self-scan silently failed (see the reachability check's
        // own build-failure handling) and looked like "nothing registered". Now that a check IS
        // registered, assert that directly instead of running it against a real repo from a unit
        // test -- a unit test has no business writing into a shared .git via `git worktree add`.
        let checks = registered_checks();
        let ids: Vec<_> = checks.iter().map(|c| c.id()).collect();
        assert_eq!(
            ids,
            vec![
                "reachability",
                "unused-return-values",
                "unused-parameters",
                "version-consistency",
                "ci-protection-presence"
            ]
        );
    }

    /// A check that only says what it judges.
    struct UnitFake {
        id: &'static str,
        unit: check::Unit,
    }

    impl Check for UnitFake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn scope(&self) -> check::Scope {
            check::Scope::Project
        }
        fn network(&self) -> check::Network {
            check::Network::LocalOnly
        }
        fn unit(&self) -> check::Unit {
            self.unit
        }
        fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
            Ok(vec![Finding {
                check_id: self.id.into(),
                severity: finding::Severity::Info,
                confidence: finding::Confidence::High,
                project: "p".into(),
                location: None,
                subject: Some("s".into()),
                summary: "ran".into(),
                detail: "ran".into(),
                positive_control: Some("ran".into()),
                member: None,
            }])
        }
    }

    fn ran(result: &RunResult) -> Vec<String> {
        let mut ids: Vec<String> = result.findings.iter().map(|f| f.check_id.clone()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_package_unit_filter_skips_repository_checks_and_the_reverse() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = CheckContext {
            project_root: tmp.path().to_path_buf(),
            portfolio_root: tmp.path().to_path_buf(),
        };
        let checks: Vec<Box<dyn Check>> = vec![
            Box::new(UnitFake {
                id: "per-package",
                unit: check::Unit::Package,
            }),
            Box::new(UnitFake {
                id: "per-repository",
                unit: check::Unit::Repository,
            }),
        ];
        let any = run_checks_over(&checks, &ctx, Tier::Fast, None, UnitFilter::Any);
        assert_eq!(
            ran(&any),
            vec!["per-package", "per-repository"],
            "Any is what run_checks always did"
        );
        let package = run_checks_over(
            &checks,
            &ctx,
            Tier::Fast,
            None,
            UnitFilter::Only(check::Unit::Package),
        );
        assert_eq!(ran(&package), vec!["per-package"]);
        let repository = run_checks_over(
            &checks,
            &ctx,
            Tier::Fast,
            None,
            UnitFilter::Only(check::Unit::Repository),
        );
        assert_eq!(ran(&repository), vec!["per-repository"]);
    }

    #[test]
    fn the_registered_checks_declare_what_they_judge() {
        let judged: Vec<(&str, check::Unit)> = registered_checks()
            .iter()
            .map(|c| (c.id(), c.unit()))
            .collect();
        assert_eq!(
            judged,
            vec![
                ("reachability", check::Unit::Package),
                ("unused-return-values", check::Unit::Package),
                ("unused-parameters", check::Unit::Package),
                // reads the whole workspace's versions: from a member it only says "run me on the root"
                ("version-consistency", check::Unit::Repository),
                // reads one GitHub repository's settings: one finding per repository, not per member
                ("ci-protection-presence", check::Unit::Repository),
            ]
        );
    }

    struct Fake {
        id: &'static str,
        result: fn() -> anyhow::Result<Vec<Finding>>,
    }

    impl Check for Fake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn scope(&self) -> check::Scope {
            check::Scope::Project
        }
        fn network(&self) -> check::Network {
            check::Network::LocalOnly
        }
        fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
            (self.result)()
        }
    }

    fn fake_finding(summary: &str, control: Option<&str>) -> Finding {
        Finding {
            check_id: "fake".into(),
            severity: finding::Severity::Low,
            confidence: finding::Confidence::High,
            project: "p".into(),
            location: Some(finding::Location {
                file: "src/x.rs".into(),
                line: Some(1),
            }),
            subject: Some("present".into()),
            summary: summary.into(),
            detail: "d".into(),
            positive_control: control.map(str::to_string),
            member: None,
        }
    }

    /// Runs `fake` over a project whose allowlist has one entry that matches nothing.
    fn run_fake_with_an_unmatched_entry(fake: Fake) -> RunResult {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            "[[allow]]\ncheck = \"fake\"\nfile = \"src/x.rs\"\nsymbol = \"gone\"\nreason = \"r\"\n",
        )
        .unwrap();
        let ctx = CheckContext {
            project_root: tmp.path().to_path_buf(),
            portfolio_root: tmp.path().to_path_buf(),
        };
        run_checks_over(&[Box::new(fake)], &ctx, Tier::Fast, None, UnitFilter::Any)
    }

    #[test]
    fn a_check_that_completes_has_its_unmatched_entry_reported() {
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || Ok(vec![fake_finding("a plain finding", None)]),
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let stale: Vec<_> = r
            .findings
            .iter()
            .filter(|f| f.check_id == "allowlist")
            .collect();
        assert_eq!(stale.len(), 1, "{:?}", r.findings);
    }

    #[test]
    fn a_check_that_returned_an_invalid_finding_is_not_judged() {
        // Review finding: nothing exercised `all_valid`. An absence claim with no positive control
        // is rejected; the check did not complete cleanly, so its entries must not be called stale.
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || Ok(vec![fake_finding("zero callers found", None)]),
        });
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        assert!(
            r.findings.iter().all(|f| f.check_id != "allowlist"),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn a_finding_reported_under_another_id_still_reaches_the_report() {
        // Library users' checks have always been free to report a finding under another id; the in-process module
        // attributes a finding to the rule result that follows it, not to its check_id (external modules are matched by id).
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "other",
            result: || Ok(vec![fake_finding("a plain finding", None)]),
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(
            r.findings.iter().any(|f| f.check_id == "fake"),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn a_check_that_errors_is_not_judged() {
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || anyhow::bail!("boom"),
        });
        assert_eq!(r.errors.len(), 1);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }

    #[test]
    fn unknown_check_id_is_an_error_and_runs_nothing() {
        // A bogus id short-circuits before any check's (potentially expensive, real-build) `run` is ever called --
        // verified by using a path that would fail if `run` were invoked -- and is an ERROR: "no findings" and "nothing
        // ran" are different answers (it used to be an empty, clean result).
        let ctx = CheckContext::new("/does/not/exist");
        let result = run_checks(&ctx, Tier::Fast, Some("no-such-check"));
        assert!(result.findings.is_empty());
        assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
        assert!(result.errors[0].contains("no check named \"no-such-check\""));
        assert!(
            result.errors[0].contains("reachability"),
            "lists the real ids"
        );
    }
}
