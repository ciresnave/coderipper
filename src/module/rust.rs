//! [`RustModule`]: the built-in checks behind the module interface.

use super::{
    Capabilities, ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleClaim,
    RuleResult,
};
use crate::check::{Check, CheckContext, Network};

/// The built-in rules whose claim is earned: each has a seeded-defect fixture and a clean twin under `conformance/`, run by this
/// repository's own tests (`tests/conformance.rs` fails if this list and the fixtures disagree). The host trusts a built-in
/// module's `proof` because that CI proved it; there is no per-build lock file for code that is compiled into the binary.
const PROVEN: &[&str] = &[
    "reachability",
    "unused-parameters",
    "unused-return-values",
    "version-consistency",
];

/// The compiled-in checks, answering the module protocol in process. A request names rules by check id; each named check
/// runs through [`Check::run`] and comes back as its raw findings followed by one rule result, exactly the events an
/// external module would print. The host, not this type, validates findings and applies the allowlist.
pub struct RustModule<'a> {
    checks: &'a [Box<dyn Check>],
}

impl<'a> RustModule<'a> {
    /// A module over these checks (see [`crate::registered_checks`] for the built-in five).
    pub fn new(checks: &'a [Box<dyn Check>]) -> Self {
        Self { checks }
    }
}

impl Module for RustModule<'_> {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "rust",
            env!("CARGO_PKG_VERSION"),
            vec!["rust".into()],
            vec!["Cargo.toml".into()],
            Capabilities::new(
                // the reachability, return-value and parameter checks build the project
                true,
                self.checks
                    .iter()
                    .any(|c| c.network() == Network::NetworkRequired),
                true,
                false,
            ),
            self.checks
                .iter()
                .map(|c| {
                    let mut claim = RuleClaim::native(c.id());
                    if PROVEN.contains(&c.id()) {
                        claim.proof = Some(format!("conformance/{}", c.id()));
                    }
                    claim
                })
                .collect(),
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let ctx = CheckContext::new(&request.project_root).portfolio_root(&request.portfolio_root);
        let mut events = Vec::new();
        let (mut ran, mut errored, mut findings) = (0, 0, 0);
        for rule in &request.rules {
            let Some(check) = self.checks.iter().find(|c| c.id() == rule) else {
                events.push(Event::RuleResult(RuleResult::error(
                    rule,
                    ErrorKind::Internal,
                    "the Rust module has no such rule",
                )));
                errored += 1;
                continue;
            };
            match check.run(&ctx) {
                Ok(raw) => {
                    ran += 1;
                    findings += raw.len();
                    let count = raw.len();
                    events.extend(raw.into_iter().map(|f| Event::Finding(Box::new(f))));
                    events.push(Event::RuleResult(RuleResult::ran(rule, count)));
                }
                Err(e) => {
                    errored += 1;
                    events.push(Event::RuleResult(RuleResult::error(
                        rule,
                        ErrorKind::Internal,
                        e.to_string(),
                    )));
                }
            }
        }
        let summary = ModuleSummary {
            rules_requested: request.rules.len(),
            rules_ran: ran,
            rules_errored: errored,
            findings,
            ..ModuleSummary::default()
        };
        events.push(Event::Summary(summary));
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }
}
