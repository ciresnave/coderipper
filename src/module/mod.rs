// SPDX-License-Identifier: MIT OR Apache-2.0
//! Modules: the unit the host asks to check rules.
//!
//! A [`Module`] answers `describe` (what it covers) and `check` (run exactly these rules). The five built-in checks live
//! behind [`RustModule`], in process; any other module is a separate executable behind [`ExternalModule`], speaking
//! newline-delimited JSON (the protocol of the design, `docs/superpowers/specs/2026-10-07-multi-language-design.md` §5).
//!
//! Whatever a module prints, [`reconcile`] is what the host believes: **a rule that gave no verdict is an error, never a
//! clean run**, and a module cannot leave a hole by being silent, crashing, hanging or lying about a count.

mod composite;
mod delegate;
mod external;
mod neutral;
mod protocol;
mod rust;

use std::collections::{HashMap, HashSet};

pub use composite::Composite;
pub use delegate::{delegated_rule_ids, DelegatedModule};
pub use external::ExternalModule;
pub use neutral::{rule_ids as neutral_rule_ids, NeutralModule};
pub use protocol::{
    Capabilities, ClaimStatus, ErrorKind, Event, Hello, Limits, ModuleSummary, Request,
    RequestTier, RequestUnit, RuleClaim, RuleRan, RuleResult, HELLO_REASON, PROTOCOL,
    REQUEST_REASON, TOOL_UNAVAILABLE,
};
pub use rust::RustModule;

use crate::finding::Finding;

/// Something that checks rules.
///
/// `Send + Sync` so a host can run modules on a thread pool or a service; a method added later will have a default body.
pub trait Module: Send + Sync {
    /// What the module covers.
    fn describe(&self) -> anyhow::Result<Hello>;

    /// Checks exactly the rules in `request` and returns what the module said, unjudged: pass it to [`reconcile`].
    fn check(&self, request: &Request) -> ModuleOutput;
}

/// Why a module run ended badly, as the host saw it from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModuleFailure {
    /// The closed-set reason ([`ErrorKind::Timeout`], [`ErrorKind::ModuleCrashed`], [`ErrorKind::LimitExceeded`], ...).
    pub kind: ErrorKind,
    /// What happened, with the tail of the module's stderr when it has one.
    pub detail: String,
}

impl ModuleFailure {
    /// A failure of this kind.
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

/// What a module run produced, before the host has judged it.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ModuleOutput {
    /// The events, in the order the module sent them.
    pub events: Vec<Event>,
    /// Lines that could not be read as events (not JSON, no `reason`, malformed): each is a protocol violation.
    pub unreadable: Vec<String>,
    /// Set when the run ended badly: a timeout, a crash, a non-zero exit, output past the limits.
    pub failure: Option<ModuleFailure>,
    /// True for a module that runs in the host's own process (it cannot interleave rules), so a finding belongs to the
    /// rule result that follows it whatever its `check_id` says: a library user's check has always been free to report a
    /// finding under another id, and the allowlist keys on the finding's own `check_id`. An external module is matched by
    /// `check_id`, as the protocol says.
    pub in_process: bool,
}

impl ModuleOutput {
    /// An output with these events and nothing wrong.
    pub fn from_events(events: Vec<Event>) -> Self {
        Self {
            events,
            ..Self::default()
        }
    }

    /// An output with no events at all and this failure.
    pub fn failed(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            failure: Some(ModuleFailure::new(kind, detail)),
            ..Self::default()
        }
    }
}

/// What the host concluded about one requested rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuleStatus {
    /// The rule ran to completion; `findings` are its verdict (an empty list is a clean rule).
    Ran,
    /// The rule did not apply to this project, as the module found out at run time. A gap, never `clean`.
    Skipped {
        /// The module's explanation.
        detail: String,
    },
    /// The rule gave no verdict. The audit is incomplete.
    Error {
        /// Why.
        kind: ErrorKind,
        /// The details.
        detail: String,
    },
    /// (Declared last: a variant added before `Error` would change its discriminant.) The rule was not run because the tool it delegates to is not available (see [`TOOL_UNAVAILABLE`]). A coverage gap: it
    /// fails nothing, and it is never `clean`.
    Unavailable {
        /// What is missing and what to do about it.
        detail: String,
    },
}

/// One requested rule and what came of it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RuleOutcome {
    /// The rule's id.
    pub rule: String,
    /// Its findings. For an [`RuleStatus::Error`] rule these are partial: shown, never used to claim the rule clean.
    pub findings: Vec<Finding>,
    /// How it ended.
    pub status: RuleStatus,
    /// What the rule said that is not a finding (a count it set aside): a rule that ran may carry some in its `detail`, one per
    /// line. Notes, never findings: they cannot fail a run.
    pub notes: Vec<String>,
}

/// The host's judgement of a [`ModuleOutput`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reconciled {
    /// One outcome per requested rule, in request order.
    pub rules: Vec<RuleOutcome>,
    /// Problems with the run itself (a missing summary, an unreadable line, a duplicate result, a crash), each an error.
    pub run_errors: Vec<String>,
    /// Whether the run did not finish.
    pub incomplete: bool,
}

/// Judges what a module said against what was asked, by the rules of design §5.5:
///
/// - exactly one rule result per requested rule; a missing one is `no_verdict` (or the run's failure), a duplicate or an
///   unrequested one is dropped and recorded as `protocol_mismatch`;
/// - a rule's `findings` count must equal the finding lines received for it, or the rule is `protocol_mismatch`;
/// - a skip is only valid as `not_applicable_here`, and an external module that skips every requested rule is `no_verdict`
///   (an in-process module is the host's own rules: skipping the one rule asked for is an answer, "not applicable");
/// - a missing module summary, a failure, or an unreadable line makes the run an error.
pub fn reconcile(request: &Request, output: ModuleOutput) -> Reconciled {
    let requested: Vec<&str> = request.rules.iter().map(String::as_str).collect();
    let requested_set: HashSet<&str> = requested.iter().copied().collect();
    let mut run_errors = Vec::new();
    let mut pending: HashMap<String, Vec<Finding>> = HashMap::new();
    let mut committed: HashMap<String, (RuleResult, Vec<Finding>)> = HashMap::new();
    let mut summary_seen = false;
    let mut batch: Vec<Finding> = Vec::new();

    for line in &output.unreadable {
        run_errors.push(format!(
            "protocol_mismatch: unreadable line from the module: {line}"
        ));
    }
    for event in output.events {
        if summary_seen {
            run_errors.push(
                "protocol_mismatch: the module sent an event after its summary (dropped)".into(),
            );
            continue;
        }
        match event {
            Event::Finding(finding) if output.in_process => batch.push(*finding),
            Event::Finding(finding) => {
                let id = finding.check_id.clone();
                if !requested_set.contains(id.as_str()) {
                    run_errors.push(format!(
                        "protocol_mismatch: a finding for \"{id}\", which was not requested (dropped)"
                    ));
                } else if committed.contains_key(&id) {
                    run_errors.push(format!(
                        "protocol_mismatch: a finding for \"{id}\" after its result (dropped)"
                    ));
                } else {
                    pending.entry(id).or_default().push(*finding);
                }
            }
            Event::RuleResult(result) => {
                if !requested_set.contains(result.rule.as_str()) {
                    run_errors.push(format!(
                        "protocol_mismatch: a result for \"{}\", which was not requested (dropped)",
                        result.rule
                    ));
                } else if committed.contains_key(&result.rule) {
                    run_errors.push(format!(
                        "protocol_mismatch: a second result for \"{}\" (dropped)",
                        result.rule
                    ));
                } else {
                    let findings = if output.in_process {
                        std::mem::take(&mut batch)
                    } else {
                        pending.remove(&result.rule).unwrap_or_default()
                    };
                    committed.insert(result.rule.clone(), (result, findings));
                }
            }
            Event::Summary(_) => summary_seen = true,
            Event::Progress(..) => {}
        }
    }

    let mut incomplete = false;
    if let Some(failure) = &output.failure {
        incomplete = true;
        run_errors.push(format!("{}: {}", failure.kind, failure.detail));
    }
    if !summary_seen {
        incomplete = true;
        if output.failure.is_none() {
            run_errors.push(
                "protocol_mismatch: the module ended without a coderipper-module-summary".into(),
            );
        }
    }

    let mut rules: Vec<RuleOutcome> = requested
        .iter()
        .map(|&rule| match committed.remove(rule) {
            Some((result, findings)) => judge(result, findings, output.in_process),
            None => {
                let (kind, detail) = output.failure.as_ref().map_or_else(
                    || {
                        (
                            ErrorKind::NoVerdict,
                            "the module finished without a result for this rule".to_string(),
                        )
                    },
                    |f| (f.kind, f.detail.clone()),
                );
                RuleOutcome {
                    rule: rule.to_string(),
                    // partial findings are shown, flagged by the error status; they never make the rule clean
                    findings: pending.remove(rule).unwrap_or_default(),
                    status: RuleStatus::Error { kind, detail },
                    notes: Vec::new(),
                }
            }
        })
        .collect();

    if !rules.is_empty()
        && !output.in_process
        && rules
            .iter()
            .all(|r| matches!(r.status, RuleStatus::Skipped { .. }))
    {
        for outcome in &mut rules {
            outcome.status = RuleStatus::Error {
                kind: ErrorKind::NoVerdict,
                detail: "the module skipped every requested rule".into(),
            };
        }
    }

    Reconciled {
        rules,
        run_errors,
        incomplete,
    }
}

/// How many notes one rule's result may carry, and how long one may be: a note is text from a module (possibly a child
/// process) that reaches a terminal and a JSON line.
const MAX_NOTES_PER_RULE: usize = 20;
const MAX_NOTE_CHARS: usize = 500;

/// `line` with control characters and bidi overrides replaced by a space, cut at [`MAX_NOTE_CHARS`] characters.
fn clean_note(line: &str) -> String {
    let flat: String = line
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Before the cut: a cut inside a secret would leave half of it, too short for any shape to recognise.
    let flat = crate::redact::scrub(&flat);
    if flat.chars().count() > MAX_NOTE_CHARS {
        let cut: String = flat.chars().take(MAX_NOTE_CHARS).collect();
        format!("{cut}...")
    } else {
        flat
    }
}

/// One rule's committed result against the findings received for it.
fn judge(result: RuleResult, findings: Vec<Finding>, in_process: bool) -> RuleOutcome {
    let mismatch = |detail: String| RuleOutcome {
        rule: result.rule.clone(),
        findings: Vec::new(),
        status: RuleStatus::Error {
            kind: ErrorKind::ProtocolMismatch,
            detail,
        },
        notes: Vec::new(),
    };
    if result.findings != findings.len() {
        return mismatch(format!(
            "the result says {} finding(s) and {} finding line(s) arrived",
            result.findings,
            findings.len()
        ));
    }
    let status = match result.status {
        RuleRan::Ran => RuleStatus::Ran,
        RuleRan::Skipped
            if in_process && result.reason_code.as_deref() == Some(TOOL_UNAVAILABLE) =>
        {
            if !findings.is_empty() {
                return mismatch("a rule whose tool is unavailable sent findings".into());
            }
            RuleStatus::Unavailable {
                detail: result.detail.clone().unwrap_or_default(),
            }
        }
        RuleRan::Skipped => {
            if result.reason_code.as_deref() != Some("not_applicable_here") {
                return mismatch(
                    "a skip needs reason_code \"not_applicable_here\"; the host decides what is in scope before it asks"
                        .into(),
                );
            }
            if !findings.is_empty() {
                return mismatch("a skipped rule sent findings".into());
            }
            RuleStatus::Skipped {
                detail: result.detail.clone().unwrap_or_default(),
            }
        }
        RuleRan::Error => RuleStatus::Error {
            kind: result.error_kind.unwrap_or(ErrorKind::Internal),
            detail: result.detail.clone().unwrap_or_default(),
        },
    };
    let notes = match (&status, &result.detail) {
        (RuleStatus::Ran, Some(detail)) => detail
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(MAX_NOTES_PER_RULE)
            .map(clean_note)
            .collect(),
        _ => Vec::new(),
    };
    RuleOutcome {
        rule: result.rule,
        findings,
        status,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{CheckContext, Tier};
    use crate::finding::{Confidence, Severity};

    fn request(rules: &[&str]) -> Request {
        Request::new(
            &CheckContext::new("."),
            None,
            Tier::Fast,
            rules.iter().map(|r| r.to_string()).collect(),
            Limits::default(),
        )
    }

    fn finding(rule: &str) -> Event {
        Event::Finding(Box::new(Finding::new(
            rule,
            Severity::Low,
            Confidence::High,
            "p",
            "a finding",
            "d",
        )))
    }

    fn ran(rule: &str, n: usize) -> Event {
        Event::RuleResult(RuleResult::ran(rule, n))
    }

    fn summary() -> Event {
        Event::Summary(ModuleSummary::default())
    }

    fn statuses(r: &Reconciled) -> Vec<(&str, &RuleStatus)> {
        r.rules
            .iter()
            .map(|o| (o.rule.as_str(), &o.status))
            .collect()
    }

    fn kind_of(outcome: &RuleOutcome) -> Option<ErrorKind> {
        match outcome.status {
            RuleStatus::Error { kind, .. } => Some(kind),
            _ => None,
        }
    }

    #[test]
    fn a_complete_run_is_judged_as_the_module_said() {
        let r = reconcile(
            &request(&["a", "b"]),
            ModuleOutput::from_events(vec![finding("a"), ran("a", 1), ran("b", 0), summary()]),
        );
        assert_eq!(r.run_errors, Vec::<String>::new());
        assert!(!r.incomplete);
        assert_eq!(
            statuses(&r),
            vec![("a", &RuleStatus::Ran), ("b", &RuleStatus::Ran)]
        );
        assert_eq!(r.rules[0].findings.len(), 1);
        assert!(r.rules[1].findings.is_empty());
    }

    #[test]
    fn a_requested_rule_with_no_result_is_no_verdict_not_clean() {
        let r = reconcile(
            &request(&["a", "b"]),
            ModuleOutput::from_events(vec![ran("a", 0), summary()]),
        );
        assert_eq!(kind_of(&r.rules[1]), Some(ErrorKind::NoVerdict));
        assert_eq!(kind_of(&r.rules[0]), None);
    }

    #[test]
    fn a_missing_summary_makes_the_run_incomplete_and_an_error() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![ran("a", 0)]),
        );
        assert!(r.incomplete);
        assert_eq!(r.run_errors.len(), 1, "{:?}", r.run_errors);
        // the committed rule keeps its result
        assert_eq!(kind_of(&r.rules[0]), None);
    }

    #[test]
    fn a_failure_turns_every_rule_without_a_result_into_that_failure() {
        let mut output = ModuleOutput::from_events(vec![ran("a", 0)]);
        output.failure = Some(ModuleFailure::new(ErrorKind::Timeout, "past 1s"));
        let r = reconcile(&request(&["a", "b"]), output);
        assert!(r.incomplete);
        assert_eq!(
            kind_of(&r.rules[0]),
            None,
            "a result before the failure is kept"
        );
        assert_eq!(kind_of(&r.rules[1]), Some(ErrorKind::Timeout));
        assert_eq!(r.run_errors, vec!["timeout: past 1s".to_string()]);
    }

    #[test]
    fn partial_findings_of_an_uncommitted_rule_are_kept_but_the_rule_is_an_error() {
        let mut output = ModuleOutput::from_events(vec![finding("a")]);
        output.failure = Some(ModuleFailure::new(ErrorKind::ModuleCrashed, "exit 1"));
        let r = reconcile(&request(&["a"]), output);
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ModuleCrashed));
        assert_eq!(r.rules[0].findings.len(), 1);
    }

    #[test]
    fn a_wrong_finding_count_makes_that_rule_a_protocol_mismatch_and_drops_its_findings() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![finding("a"), ran("a", 2), summary()]),
        );
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ProtocolMismatch));
        assert!(r.rules[0].findings.is_empty());
    }

    #[test]
    fn a_duplicate_or_unrequested_result_is_dropped_and_recorded() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![ran("a", 0), ran("a", 0), ran("zzz", 0), summary()]),
        );
        assert_eq!(kind_of(&r.rules[0]), None);
        assert_eq!(r.run_errors.len(), 2, "{:?}", r.run_errors);
    }

    #[test]
    fn a_finding_for_an_unrequested_rule_or_after_its_result_is_dropped_and_recorded() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![finding("zzz"), ran("a", 0), finding("a"), summary()]),
        );
        assert!(r.rules[0].findings.is_empty());
        assert_eq!(r.run_errors.len(), 2, "{:?}", r.run_errors);
    }

    #[test]
    fn an_unreadable_line_is_a_run_error() {
        let mut output = ModuleOutput::from_events(vec![ran("a", 0), summary()]);
        output.unreadable.push("garbage".into());
        let r = reconcile(&request(&["a"]), output);
        assert_eq!(r.run_errors.len(), 1);
        assert_eq!(kind_of(&r.rules[0]), None);
    }

    fn skip(rule: &str, code: Option<&str>) -> Event {
        let mut result = RuleResult::ran(rule, 0);
        result.status = RuleRan::Skipped;
        result.reason_code = code.map(str::to_string);
        result.detail = Some("no tsconfig.json here".into());
        Event::RuleResult(result)
    }

    #[test]
    fn a_not_applicable_skip_is_a_gap_and_not_an_error() {
        let r = reconcile(
            &request(&["a", "b"]),
            ModuleOutput::from_events(vec![
                skip("a", Some("not_applicable_here")),
                ran("b", 0),
                summary(),
            ]),
        );
        assert!(matches!(r.rules[0].status, RuleStatus::Skipped { .. }));
        assert_eq!(r.run_errors, Vec::<String>::new());
    }

    #[test]
    fn a_skip_with_any_other_reason_is_a_protocol_mismatch() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![skip("a", Some("felt_like_it")), summary()]),
        );
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ProtocolMismatch));
    }

    #[test]
    fn a_module_that_skips_everything_gave_no_verdict() {
        let r = reconcile(
            &request(&["a", "b"]),
            ModuleOutput::from_events(vec![
                skip("a", Some("not_applicable_here")),
                skip("b", Some("not_applicable_here")),
                summary(),
            ]),
        );
        assert!(r
            .rules
            .iter()
            .all(|o| kind_of(o) == Some(ErrorKind::NoVerdict)));
    }

    fn in_process(events: Vec<Event>) -> ModuleOutput {
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }

    #[test]
    fn an_in_process_module_whose_every_rule_does_not_apply_has_not_failed() {
        // `coderipper check DOC-009` on a repository with no docs/ asks for one rule and gets "not applicable": that is an
        // answer (a gap, exit 0), not "failed to run"; only an external module that skips everything is suspect
        let r = reconcile(
            &request(&["a"]),
            in_process(vec![skip("a", Some("not_applicable_here")), summary()]),
        );
        assert!(matches!(r.rules[0].status, RuleStatus::Skipped { .. }));
        assert!(r.run_errors.is_empty() && !r.incomplete);
    }

    #[test]
    fn a_tool_unavailable_rule_of_an_in_process_module_is_a_gap_not_an_error() {
        let r = reconcile(
            &request(&["a", "b"]),
            in_process(vec![
                Event::RuleResult(RuleResult::tool_unavailable(
                    "a",
                    "gitleaks is not installed",
                )),
                ran("b", 0),
                summary(),
            ]),
        );
        assert_eq!(r.run_errors, Vec::<String>::new());
        assert!(
            matches!(&r.rules[0].status, RuleStatus::Unavailable { detail } if detail == "gitleaks is not installed")
        );
        assert_eq!(kind_of(&r.rules[0]), None);
    }

    #[test]
    fn a_module_whose_every_rule_has_no_tool_has_not_failed() {
        // unlike a module that skips everything as "not applicable" (no_verdict): the absence of a tool is declared and reported
        let r = reconcile(
            &request(&["a"]),
            in_process(vec![
                Event::RuleResult(RuleResult::tool_unavailable("a", "no tool")),
                summary(),
            ]),
        );
        assert!(matches!(r.rules[0].status, RuleStatus::Unavailable { .. }));
        assert!(r.run_errors.is_empty() && !r.incomplete);
    }

    #[test]
    fn an_external_module_cannot_report_a_tool_gap() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![
                Event::RuleResult(RuleResult::tool_unavailable("a", "no tool")),
                summary(),
            ]),
        );
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ProtocolMismatch));
    }

    #[test]
    fn a_tool_gap_that_sent_findings_is_a_protocol_mismatch() {
        let mut gap = RuleResult::tool_unavailable("a", "no tool");
        gap.findings = 1;
        let r = reconcile(
            &request(&["a"]),
            in_process(vec![finding("a"), Event::RuleResult(gap), summary()]),
        );
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ProtocolMismatch));
    }

    #[test]
    fn an_event_after_the_summary_is_dropped_and_recorded() {
        let r = reconcile(
            &request(&["a"]),
            ModuleOutput::from_events(vec![ran("a", 0), summary(), finding("a")]),
        );
        assert_eq!(r.run_errors.len(), 1);
        assert!(r.rules[0].findings.is_empty());
    }

    #[test]
    fn an_error_result_carries_its_kind_and_defaults_to_internal() {
        let mut bare = RuleResult::ran("b", 0);
        bare.status = RuleRan::Error;
        let r = reconcile(
            &request(&["a", "b"]),
            ModuleOutput::from_events(vec![
                Event::RuleResult(RuleResult::error("a", ErrorKind::ToolMissing, "no eslint")),
                Event::RuleResult(bare),
                summary(),
            ]),
        );
        assert_eq!(kind_of(&r.rules[0]), Some(ErrorKind::ToolMissing));
        assert_eq!(kind_of(&r.rules[1]), Some(ErrorKind::Internal));
    }

    #[test]
    fn events_round_trip_through_their_json_line_and_unknown_reasons_are_ignored() {
        for event in [
            finding("a"),
            ran("a", 1),
            summary(),
            Event::Progress(1, 2, "x".into()),
        ] {
            let line = event.to_line().unwrap();
            assert_eq!(Event::parse(&line).unwrap(), Some(event), "{line}");
        }
        assert_eq!(
            Event::parse(r#"{"reason":"coderipper-from-the-future"}"#),
            Ok(None)
        );
        assert!(Event::parse("not json").is_err());
        assert!(Event::parse(r#"{"no":"reason"}"#).is_err());
        assert!(
            Event::parse(r#"{"reason":"coderipper-rule-result","rule":"a"}"#).is_err(),
            "a known event missing its required fields is malformed, not ignored"
        );
    }

    #[test]
    fn a_finding_line_with_the_cli_s_extra_keys_still_parses() {
        // the CLI's finding line carries `reason`; a module's may carry `rule`, `language`, `tool` (additive keys)
        let line = r#"{"reason":"coderipper-finding","check_id":"a","severity":"low","confidence":"high","project":"p","location":null,"summary":"s","detail":"d","positive_control":null,"rule":"a","language":"rust","tool":{"name":"x","version":"1"}}"#;
        assert!(matches!(Event::parse(line), Ok(Some(Event::Finding(_)))));
    }

    #[test]
    fn a_hello_for_another_major_or_with_a_rule_twice_is_refused() {
        let ok = Hello::new(
            "m",
            "1",
            vec![],
            vec![],
            Capabilities::default(),
            vec![RuleClaim::native("a")],
        );
        assert_eq!(ok.problem(), None);
        let mut other = ok.clone();
        other.protocol = vec!["2.0".into()];
        assert!(other.problem().is_some());
        let mut twice = ok.clone();
        twice.rules.push(RuleClaim::native("a"));
        assert!(twice.problem().is_some());
    }
}

#[cfg(test)]
mod note_tests {
    use super::*;

    #[test]
    fn a_ran_rules_detail_becomes_clean_capped_notes_and_never_a_finding() {
        let mut result = RuleResult::ran("a", 0);
        let long = "x".repeat(900);
        result.detail = Some(format!(
            "first \u{1b}]0;title\u{7}line\n\n{long}\n\u{202e}reversed"
        ));
        let outcome = judge(result, Vec::new(), true);
        assert_eq!(outcome.status, RuleStatus::Ran);
        assert!(outcome.findings.is_empty());
        assert_eq!(outcome.notes.len(), 3, "{:?}", outcome.notes);
        assert!(outcome
            .notes
            .iter()
            .all(|n| !n.chars().any(|c| c.is_control() || c == '\u{202e}')));
        assert!(outcome.notes[1].chars().count() <= MAX_NOTE_CHARS + 3);
    }

    #[test]
    fn a_rule_may_carry_only_so_many_notes() {
        let mut result = RuleResult::ran("a", 0);
        result.detail = Some(
            (0..100)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        assert_eq!(
            judge(result, Vec::new(), true).notes.len(),
            MAX_NOTES_PER_RULE
        );
    }

    #[test]
    fn a_skipped_or_errored_rules_detail_is_not_read_as_notes() {
        let result = RuleResult::error("a", ErrorKind::Internal, "boom");
        assert!(judge(result, Vec::new(), true).notes.is_empty());
    }

    #[test]
    fn a_note_is_redacted_before_it_is_cut_when_the_policy_is_on() {
        use crate::redact::{with_policy, RedactionPolicy};
        let token = ["ghp_", &"aB3dE5fG7h".repeat(4)].concat();
        let line = format!("{} {token} tail", "x".repeat(MAX_NOTE_CHARS - 10));
        with_policy(RedactionPolicy::Off, || {
            assert!(clean_note(&line).contains("ghp_aB3"));
        });
        with_policy(RedactionPolicy::On, || {
            let note = clean_note(&line);
            assert!(!note.contains("ghp_"), "{note}");
        });
    }
}
