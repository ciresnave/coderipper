// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`Composite`]: several modules asked as one, so a run judges one allowlist and one set of rule ids across all of them.

use std::collections::HashSet;

use super::{
    ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleRan, RuleResult,
};

/// Modules that answer a single request together. Each rule goes to **every** part whose hello claims it (a rule that applies to
/// several languages is claimed by the module of each); the parts' events are joined (in the parts' order) under one closing
/// summary. A rule claimed by one part is passed through exactly as that part said it. A rule claimed by several gets ONE
/// result, merged by [`merge`]: its findings are all the parts' findings, it ran when any part ran it, and a part that errored,
/// timed out or sent no result for it is an error on the rule that no other part's `ran` can hide. The composite reports itself
/// as in-process only when every part does (a finding of an in-process module is matched to its rule by position; an external
/// one's, by its `check_id`), so compose built-in modules only.
pub struct Composite<'a> {
    parts: Vec<&'a dyn Module>,
}

impl<'a> Composite<'a> {
    /// A composite of these modules, in priority order.
    pub fn new(parts: Vec<&'a dyn Module>) -> Self {
        Self { parts }
    }
}

/// One rule's answer from one part: the part's name, its result, and the finding events that came before the result.
struct Answer {
    module: String,
    result: RuleResult,
    findings: Vec<Event>,
    in_process: bool,
}

/// Prefixes each line of `text` with `[module] `, so a merged note or error says which part produced it.
fn tagged(module: &str, text: &str) -> String {
    text.lines()
        .map(|l| format!("[{module}] {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The one result and findings of a rule that several parts answered.
///
/// - any part in `Error` (or one that is not in process, which cannot be merged by position) makes the rule an error, whose detail
///   names each erroring part; the other parts' findings are withheld and counted in the detail, never shown as a verdict;
/// - else any part that ran makes the rule `Ran`: the findings are all the ran parts' findings, summed, and each part's notes are
///   kept under its name;
/// - else every part skipped it: the first skip stands, with every part's reason named.
fn merge(rule: &str, answers: Vec<Answer>) -> (RuleResult, Vec<Event>) {
    let errors: Vec<&Answer> = answers
        .iter()
        .filter(|a| a.result.status == RuleRan::Error || !a.in_process)
        .collect();
    if !errors.is_empty() {
        let kind = errors
            .iter()
            .find_map(|a| a.result.error_kind)
            .unwrap_or(ErrorKind::Internal);
        let mut detail: Vec<String> = errors
            .iter()
            .map(|a| {
                let why = if a.in_process {
                    a.result
                        .detail
                        .clone()
                        .unwrap_or_else(|| "gave no verdict".into())
                } else {
                    "an external module's result cannot be merged with another part's".into()
                };
                tagged(&a.module, &why)
            })
            .collect();
        let withheld: usize = answers
            .iter()
            .filter(|a| a.result.status == RuleRan::Ran)
            .map(|a| a.findings.len())
            .sum();
        if withheld > 0 {
            detail.push(format!(
                "{withheld} finding(s) of the parts that ran are withheld because a part of this rule errored"
            ));
        }
        return (RuleResult::error(rule, kind, detail.join("\n")), Vec::new());
    }
    if answers.iter().any(|a| a.result.status == RuleRan::Ran) {
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        for a in answers
            .into_iter()
            .filter(|a| a.result.status == RuleRan::Ran)
        {
            if let Some(d) = &a.result.detail {
                notes.push(tagged(&a.module, d));
            }
            findings.extend(a.findings);
        }
        let mut result = RuleResult::ran(rule, findings.len());
        if !notes.is_empty() {
            result.detail = Some(notes.join("\n"));
        }
        return (result, findings);
    }
    let mut skipped = answers[0].result.clone();
    skipped.detail = Some(
        answers
            .iter()
            .map(|a| {
                tagged(
                    &a.module,
                    a.result.detail.as_deref().unwrap_or("not applicable"),
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    (skipped, Vec::new())
}

impl Module for Composite<'_> {
    fn describe(&self) -> anyhow::Result<Hello> {
        let mut hellos = Vec::new();
        for part in &self.parts {
            hellos.push(part.describe()?);
        }
        let mut joined = hellos
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("a composite of no modules"))?;
        for hello in &hellos[1..] {
            joined.module = format!("{}+{}", joined.module, hello.module);
            for language in &hello.languages {
                if !joined.languages.contains(language) {
                    joined.languages.push(language.clone());
                }
            }
            joined.detect.extend(hello.detect.iter().cloned());
            let caps = &hello.capabilities;
            joined.capabilities.executes_project_code |= caps.executes_project_code;
            joined.capabilities.needs_network |= caps.needs_network;
            joined.capabilities.needs_checkout |= caps.needs_checkout;
            joined.capabilities.emits_progress |= caps.emits_progress;
            joined.rules.extend(hello.rules.iter().cloned());
        }
        Ok(joined)
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        // what each part claims of the request
        let claims: Vec<(String, HashSet<String>)> = self
            .parts
            .iter()
            .map(|p| match p.describe() {
                Ok(h) => (h.module, h.rules.into_iter().map(|r| r.id).collect()),
                Err(_) => (String::new(), HashSet::new()),
            })
            .collect();
        let claimers = |rule: &str| claims.iter().filter(|(_, ids)| ids.contains(rule)).count();

        let mut events = Vec::new();
        let mut unreadable = Vec::new();
        let mut failure = None;
        let mut in_process = true;
        let mut incomplete = false;
        // each part's answers in the order it gave them; a rule claimed by several parts is merged at its first answer
        let mut answered: Vec<Vec<Answer>> = Vec::new();
        let mut passthrough: Vec<Vec<Event>> = Vec::new();
        for (part, (name, ids)) in self.parts.iter().zip(&claims) {
            let mine: Vec<String> = request
                .rules
                .iter()
                .filter(|r| ids.contains(r.as_str()))
                .cloned()
                .collect();
            let (mut answers, mut other) = (Vec::new(), Vec::new());
            if !mine.is_empty() {
                let mut sub = request.clone();
                sub.rules = mine.clone();
                let output = part.check(&sub);
                let mut batch: Vec<Event> = Vec::new();
                for event in output.events {
                    match event {
                        Event::Finding(_) => batch.push(event),
                        Event::RuleResult(result) => answers.push(Answer {
                            module: name.clone(),
                            result,
                            findings: std::mem::take(&mut batch),
                            in_process: output.in_process,
                        }),
                        Event::Summary(s) => incomplete |= s.incomplete,
                        progress => other.push(progress),
                    }
                }
                other.extend(batch);
                // a part that was asked for a rule and sent no result for it: an error on a shared rule, never silence
                for rule in &mine {
                    if claimers(rule) > 1 && !answers.iter().any(|a| &a.result.rule == rule) {
                        let (kind, detail) = output.failure.as_ref().map_or_else(
                            || {
                                (
                                    ErrorKind::NoVerdict,
                                    "the module finished without a result for this rule"
                                        .to_string(),
                                )
                            },
                            |f| (f.kind, f.detail.clone()),
                        );
                        answers.push(Answer {
                            module: name.clone(),
                            result: RuleResult::error(rule, kind, detail),
                            findings: Vec::new(),
                            in_process: true,
                        });
                    }
                }
                in_process &= output.in_process;
                unreadable.extend(output.unreadable);
                failure = failure.or(output.failure);
            }
            answered.push(answers);
            passthrough.push(other);
        }

        let mut tally = ModuleSummary {
            rules_requested: request.rules.len(),
            incomplete,
            ..ModuleSummary::default()
        };
        let mut merged_done: HashSet<String> = HashSet::new();
        for (i, other) in passthrough.into_iter().enumerate() {
            events.extend(other);
            for answer in std::mem::take(&mut answered[i]) {
                let rule = answer.result.rule.clone();
                let (result, findings) = if claimers(&rule) > 1 {
                    if !merged_done.insert(rule.clone()) {
                        continue;
                    }
                    let all: Vec<Answer> = std::iter::once(answer)
                        .chain(answered[i + 1..].iter_mut().flat_map(|later| {
                            let (mine, rest): (Vec<Answer>, Vec<Answer>) = std::mem::take(later)
                                .into_iter()
                                .partition(|a| a.result.rule == rule);
                            *later = rest;
                            mine
                        }))
                        .collect();
                    merge(&rule, all)
                } else {
                    (answer.result, answer.findings)
                };
                match result.status {
                    RuleRan::Ran => tally.rules_ran += 1,
                    RuleRan::Skipped => tally.rules_skipped += 1,
                    RuleRan::Error => tally.rules_errored += 1,
                }
                tally.findings += findings.len();
                events.extend(findings);
                events.push(Event::RuleResult(result));
            }
        }
        // a requested rule that no part claims gets no result: the host reports it as having given no verdict
        events.push(Event::Summary(tally));
        ModuleOutput {
            events,
            unreadable,
            failure,
            in_process,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Confidence, Finding, Severity};
    use crate::module::{Capabilities, Hello, RuleClaim};

    /// What a fake part says about the one rule `R-1`.
    #[derive(Clone)]
    enum Say {
        Findings(usize, Option<&'static str>),
        Skip,
        Error(&'static str),
        Nothing,
    }

    struct Part {
        name: &'static str,
        says: Say,
    }

    impl Module for Part {
        fn describe(&self) -> anyhow::Result<Hello> {
            Ok(Hello::new(
                self.name,
                "0",
                vec!["any".into()],
                vec![],
                Capabilities::new(false, false, false, false),
                vec![RuleClaim::native("R-1")],
            ))
        }

        fn check(&self, _request: &Request) -> ModuleOutput {
            let mut events = Vec::new();
            match &self.says {
                Say::Findings(n, note) => {
                    for _ in 0..*n {
                        events.push(Event::Finding(Box::new(Finding::new(
                            "R-1",
                            Severity::Low,
                            Confidence::High,
                            "p",
                            "a finding",
                            "fix it",
                        ))));
                    }
                    let notes: Vec<String> = note.iter().map(|n| (*n).to_string()).collect();
                    events.push(Event::RuleResult(RuleResult::ran_noted("R-1", *n, &notes)));
                }
                Say::Skip => events.push(Event::RuleResult(RuleResult::not_applicable(
                    "R-1",
                    "no files of mine",
                ))),
                Say::Error(why) => events.push(Event::RuleResult(RuleResult::error(
                    "R-1",
                    ErrorKind::Internal,
                    *why,
                ))),
                Say::Nothing => {}
            }
            events.push(Event::Summary(ModuleSummary::default()));
            ModuleOutput {
                in_process: true,
                ..ModuleOutput::from_events(events)
            }
        }
    }

    fn run(parts: &[(&'static str, Say)]) -> (Vec<Event>, ModuleSummary) {
        let built: Vec<Part> = parts
            .iter()
            .map(|(name, says)| Part {
                name,
                says: says.clone(),
            })
            .collect();
        let dyns: Vec<&dyn Module> = built.iter().map(|p| p as &dyn Module).collect();
        let ctx = crate::check::CheckContext::new(std::path::Path::new("."));
        let request = Request::new(
            &ctx,
            None,
            crate::check::Tier::Fast,
            vec!["R-1".into()],
            crate::module::Limits::default(),
        );
        let out = Composite::new(dyns).check(&request);
        let mut summary = None;
        let events: Vec<Event> = out
            .events
            .into_iter()
            .filter_map(|e| match e {
                Event::Summary(s) => {
                    summary = Some(s);
                    None
                }
                other => Some(other),
            })
            .collect();
        (events, summary.expect("one summary"))
    }

    fn results(events: &[Event]) -> Vec<&RuleResult> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::RuleResult(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_shared_rule_ran_when_any_part_ran_it() {
        // the skipping part must not make the rule skipped; the ran part's finding survives
        let (events, summary) = run(&[("rust", Say::Skip), ("typescript", Say::Findings(1, None))]);
        let r = results(&events);
        assert_eq!(r.len(), 1, "one merged result");
        assert_eq!(r[0].status, RuleRan::Ran);
        assert_eq!(r[0].findings, 1);
        assert_eq!(
            (summary.rules_ran, summary.rules_skipped, summary.findings),
            (1, 0, 1)
        );
    }

    #[test]
    fn a_shared_rule_sums_findings_and_names_the_part_of_each_note() {
        let (events, summary) = run(&[
            ("rust", Say::Findings(2, Some("read 3 files"))),
            ("typescript", Say::Findings(1, Some("read 4 files"))),
        ]);
        let r = results(&events);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].findings, 3);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Finding(_)))
                .count(),
            3
        );
        let detail = r[0].detail.as_deref().expect("notes kept");
        assert!(detail.contains("[rust] read 3 files"), "{detail}");
        assert!(detail.contains("[typescript] read 4 files"), "{detail}");
        assert_eq!(summary.findings, 3);
    }

    #[test]
    fn an_erroring_part_is_an_error_on_the_rule_whatever_the_others_say() {
        for erroring in [
            Say::Error("grammar failed to load"),
            Say::Nothing, // a part that sent no result at all
        ] {
            let (events, summary) =
                run(&[("rust", Say::Findings(1, None)), ("typescript", erroring)]);
            let r = results(&events);
            assert_eq!(r.len(), 1);
            assert_eq!(
                r[0].status,
                RuleRan::Error,
                "ran=true of the other part must not hide it"
            );
            let detail = r[0].detail.as_deref().unwrap();
            assert!(detail.contains("[typescript]"), "names the part: {detail}");
            assert!(
                detail.contains("1 finding(s)"),
                "withheld findings are counted: {detail}"
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, Event::Finding(_)))
                    .count(),
                0
            );
            assert_eq!((summary.rules_errored, summary.rules_ran), (1, 0));
        }
    }

    #[test]
    fn a_rule_every_part_skipped_stays_skipped_and_names_each_reason() {
        let (events, _) = run(&[("rust", Say::Skip), ("typescript", Say::Skip)]);
        let r = results(&events);
        assert_eq!(r[0].status, RuleRan::Skipped);
        let detail = r[0].detail.as_deref().unwrap();
        assert!(
            detail.contains("[rust]") && detail.contains("[typescript]"),
            "{detail}"
        );
    }

    #[test]
    fn a_rule_claimed_by_one_part_is_passed_through_unchanged() {
        let (events, summary) = run(&[("rust", Say::Findings(2, Some("n")))]);
        let r = results(&events);
        assert_eq!(
            r[0].detail.as_deref(),
            Some("n"),
            "no module tag on a single claim"
        );
        assert_eq!((r[0].findings, summary.findings), (2, 2));
    }
}
