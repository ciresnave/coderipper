//! [`Composite`]: several modules asked as one, so a run judges one allowlist and one set of rule ids across all of them.

use super::{Event, Hello, Module, ModuleOutput, ModuleSummary, Request};

/// Modules that answer a single request together. Each rule goes to the first part whose hello claims it; the parts' events are
/// joined (in the parts' order) under one closing summary. The composite reports itself as in-process only when every part does
/// (a finding of an in-process module is matched to its rule by position; an external one's, by its `check_id`), so compose
/// built-in modules only.
pub struct Composite<'a> {
    parts: Vec<&'a dyn Module>,
}

impl<'a> Composite<'a> {
    /// A composite of these modules, in priority order.
    pub fn new(parts: Vec<&'a dyn Module>) -> Self {
        Self { parts }
    }
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
        let mut taken: Vec<&str> = Vec::new();
        let mut events = Vec::new();
        let mut unreadable = Vec::new();
        let mut failure = None;
        let mut in_process = true;
        let mut tally = ModuleSummary {
            rules_requested: request.rules.len(),
            ..ModuleSummary::default()
        };
        for part in &self.parts {
            let claimed: Vec<String> = match part.describe() {
                Ok(hello) => hello.rules.into_iter().map(|r| r.id).collect(),
                Err(_) => Vec::new(),
            };
            let mine: Vec<String> = request
                .rules
                .iter()
                .filter(|r| claimed.contains(r) && !taken.contains(&r.as_str()))
                .cloned()
                .collect();
            if mine.is_empty() {
                continue;
            }
            taken.extend(
                request
                    .rules
                    .iter()
                    .filter(|r| mine.contains(r))
                    .map(String::as_str),
            );
            let mut sub = request.clone();
            sub.rules = mine;
            let output = part.check(&sub);
            for event in output.events {
                match event {
                    Event::Summary(s) => {
                        tally.rules_ran += s.rules_ran;
                        tally.rules_skipped += s.rules_skipped;
                        tally.rules_errored += s.rules_errored;
                        tally.findings += s.findings;
                        tally.incomplete |= s.incomplete;
                    }
                    other => events.push(other),
                }
            }
            in_process &= output.in_process;
            unreadable.extend(output.unreadable);
            failure = failure.or(output.failure);
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
