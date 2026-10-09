//! The wire shapes of the module protocol (design §5): the request the host sends, the hello a module answers `describe`
//! with, and the events a module prints while it checks.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::check::{CheckContext, Tier, Unit};
use crate::finding::Finding;

/// The protocol version this host speaks, as `"major.minor"`.
pub const PROTOCOL: &str = "1.0";

/// `reason` of the request line.
pub const REQUEST_REASON: &str = "coderipper-check-request";
/// `reason` of the hello object.
pub const HELLO_REASON: &str = "coderipper-module-hello";
const FINDING_REASON: &str = "coderipper-finding";
const RULE_RESULT_REASON: &str = "coderipper-rule-result";
const PROGRESS_REASON: &str = "coderipper-progress";
const SUMMARY_REASON: &str = "coderipper-module-summary";

/// Why a rule gave no verdict: a closed set (design §5.5), so a report can group on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorKind {
    /// A tool the rule needs is not installed.
    ToolMissing,
    /// The tool has no build for this platform.
    ToolUnavailableForPlatform,
    /// The runtime the tool needs (node, python, ...) is missing.
    RuntimeMissing,
    /// A downloaded tool did not match its recorded checksum.
    ChecksumMismatch,
    /// The user has not consented to installing the tool.
    ConsentNotGiven,
    /// The rule needs the network and the run did not grant it.
    NetworkNotPermitted,
    /// The tool ran and failed.
    ToolFailed,
    /// The tool's output could not be read.
    ToolOutputUnreadable,
    /// The module ran past its wall-clock limit.
    Timeout,
    /// The module produced more output or findings than the limits allow.
    LimitExceeded,
    /// The rule's configuration is invalid.
    ConfigInvalid,
    /// The module refused to run on untrusted code.
    UntrustedRefused,
    /// The module broke the protocol (a missing, duplicate or unrequested result, a wrong count, an unreadable line).
    ProtocolMismatch,
    /// The module process died before it committed a result for the rule.
    ModuleCrashed,
    /// The module finished without a verdict for the rule.
    NoVerdict,
    /// Anything else: a bug in the module or the host.
    Internal,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        f.write_str(&text)
    }
}

/// The resource limits the host enforces on a module run (design §5.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Limits {
    /// Wall-clock seconds for the whole run.
    pub wall_secs: u64,
    /// Seconds of silence tolerated from a module that reports progress (not enforced yet: nothing reports progress).
    pub idle_secs: u64,
    /// Findings accepted before the run is stopped.
    pub max_findings: usize,
    /// Bytes of stdout accepted before the run is stopped.
    pub max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall_secs: 600,
            idle_secs: 120,
            max_findings: 5000,
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// Limits with the given wall-clock seconds and output cap, and the defaults for the rest.
    pub fn new(wall_secs: u64, max_output_bytes: usize) -> Self {
        Self {
            wall_secs,
            max_output_bytes,
            ..Self::default()
        }
    }
}

/// What the host asks of a module: check exactly these rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Request {
    /// Always [`REQUEST_REASON`].
    pub reason: String,
    /// The protocol version the host chose.
    pub protocol: String,
    /// Identifies this run in logs; a module treats it as opaque.
    pub run_id: String,
    /// `"package"` or `"repository"`: what one run judges (see [`Unit`]).
    pub unit: String,
    /// The user's own directory. Read-only to the module.
    pub project_root: PathBuf,
    /// Where checks that read sibling projects look for them.
    pub portfolio_root: PathBuf,
    /// `"fast"` or `"sweep"`.
    pub run_tier: String,
    /// The rules to check. The module checks exactly these.
    pub rules: Vec<String>,
    /// Whether the run permits network access.
    pub network: bool,
    /// The limits the host enforces.
    pub limits: Limits,
}

impl Request {
    /// A request to check `rules` in the project of `ctx`.
    pub fn new(
        ctx: &CheckContext,
        unit: Option<Unit>,
        tier: Tier,
        rules: Vec<String>,
        limits: Limits,
    ) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let run_id = format!(
            "{:x}-{:x}-{:x}",
            std::process::id(),
            nanos,
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        Self {
            reason: REQUEST_REASON.into(),
            protocol: PROTOCOL.into(),
            run_id,
            unit: match unit {
                Some(Unit::Repository) => "repository",
                _ => "package",
            }
            .into(),
            project_root: ctx.project_root.clone(),
            portfolio_root: ctx.portfolio_root.clone(),
            run_tier: match tier {
                Tier::Sweep => "sweep",
                _ => "fast",
            }
            .into(),
            rules,
            network: tier == Tier::Sweep,
            limits,
        }
    }
}

/// What a module can do, as it declares it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Capabilities {
    /// Whether the module runs the analysed project's own code (a build script, a lint config).
    #[serde(default)]
    pub executes_project_code: bool,
    /// Whether some rule needs the network.
    #[serde(default)]
    pub needs_network: bool,
    /// Whether the module needs a private writable checkout.
    #[serde(default)]
    pub needs_checkout: bool,
    /// Whether the module sends `coderipper-progress` events.
    #[serde(default)]
    pub emits_progress: bool,
}

impl Capabilities {
    /// Capabilities with these four answers.
    pub fn new(
        executes_project_code: bool,
        needs_network: bool,
        needs_checkout: bool,
        emits_progress: bool,
    ) -> Self {
        Self {
            executes_project_code,
            needs_network,
            needs_checkout,
            emits_progress,
        }
    }
}

/// One rule a module claims, with how it covers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RuleClaim {
    /// The rule's id.
    pub id: String,
    /// `"implemented-native"`, `"delegated"` or `"not-applicable"`.
    pub status: String,
    /// The tool a delegated rule runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Where the conformance fixture is (earns the claim; see design §6.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<String>,
    /// Why a rule is not applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The languages the claim covers. Empty means every language the rule applies to; a module whose check reads only some
    /// languages' files says which, so the coverage report shows the rest as gaps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
}

impl RuleClaim {
    /// A rule the module implements itself.
    pub fn native(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "implemented-native".into(),
            tool: None,
            proof: None,
            reason: None,
            languages: Vec::new(),
        }
    }

    /// Limits the claim to these languages (see [`RuleClaim::languages`]).
    pub fn only_languages(mut self, languages: &[&str]) -> Self {
        self.languages = languages.iter().map(|l| (*l).to_string()).collect();
        self
    }

    /// Whether the claim covers `language` (an empty list covers every language).
    pub fn covers_language(&self, language: &str) -> bool {
        self.languages.is_empty() || self.languages.iter().any(|l| l == language)
    }
}

/// The hello a module prints for `describe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Hello {
    /// Always [`HELLO_REASON`].
    pub reason: String,
    /// The module's name.
    pub module: String,
    /// The module's own version.
    pub module_version: String,
    /// The protocol majors it speaks, as `"major.minor"` strings.
    pub protocol: Vec<String>,
    /// The languages it checks.
    pub languages: Vec<String>,
    /// Manifest file names that mean the language is present.
    #[serde(default)]
    pub detect: Vec<String>,
    /// What it can do.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// The tools it uses, as the module describes them (not interpreted yet).
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,
    /// The rules it claims.
    #[serde(default)]
    pub rules: Vec<RuleClaim>,
}

impl Hello {
    /// A hello for a module that speaks this host's protocol.
    pub fn new(
        module: impl Into<String>,
        module_version: impl Into<String>,
        languages: Vec<String>,
        detect: Vec<String>,
        capabilities: Capabilities,
        rules: Vec<RuleClaim>,
    ) -> Self {
        Self {
            reason: HELLO_REASON.into(),
            module: module.into(),
            module_version: module_version.into(),
            protocol: vec![PROTOCOL.into()],
            languages,
            detect,
            capabilities,
            tools: Vec::new(),
            rules,
        }
    }

    /// Whether the hello is well formed enough to trust: the right `reason`, a protocol the host speaks, no rule twice.
    pub fn problem(&self) -> Option<String> {
        if self.reason != HELLO_REASON {
            return Some(format!(
                "the hello's reason is \"{}\", expected \"{HELLO_REASON}\"",
                self.reason
            ));
        }
        let major = |v: &str| v.split('.').next().map(str::to_string);
        if !self.protocol.iter().any(|p| major(p) == major(PROTOCOL)) {
            return Some(format!(
                "the module speaks protocol {:?}; this host speaks {PROTOCOL}",
                self.protocol
            ));
        }
        let mut seen = std::collections::HashSet::new();
        self.rules
            .iter()
            .find(|r| !seen.insert(r.id.as_str()))
            .map(|r| format!("the hello claims the rule \"{}\" twice", r.id))
    }
}

/// How a rule's run ended, as the module reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RuleRan {
    /// The rule ran to completion; its findings (possibly none) are the verdict.
    Ran,
    /// The rule did not apply here (only with `reason_code: not_applicable_here`).
    Skipped,
    /// The rule could not give a verdict.
    Error,
}

/// The one result a module commits per requested rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RuleResult {
    /// The rule this is the result of.
    pub rule: String,
    /// How the rule's run ended.
    pub status: RuleRan,
    /// How many `coderipper-finding` lines the module sent for this rule, which the host checks.
    pub findings: usize,
    /// Why a rule was skipped (`not_applicable_here`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    /// Human-readable detail, required for a skip and an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Why an errored rule gave no verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ErrorKind>,
}

impl RuleResult {
    /// A rule that ran and sent `findings` finding lines.
    pub fn ran(rule: impl Into<String>, findings: usize) -> Self {
        Self {
            rule: rule.into(),
            status: RuleRan::Ran,
            findings,
            reason_code: None,
            detail: None,
            error_kind: None,
        }
    }

    /// A rule not run because its tool is not available (see [`TOOL_UNAVAILABLE`]); `detail` says what to do about it.
    pub fn tool_unavailable(rule: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            status: RuleRan::Skipped,
            findings: 0,
            reason_code: Some(TOOL_UNAVAILABLE.into()),
            detail: Some(detail.into()),
            error_kind: None,
        }
    }

    /// A rule that could not give a verdict.
    pub fn error(rule: impl Into<String>, kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            status: RuleRan::Error,
            findings: 0,
            reason_code: None,
            detail: Some(detail.into()),
            error_kind: Some(kind),
        }
    }
}

/// The `reason_code` of a rule the host's own module could not run because the tool it delegates to is not available (not in the
/// lock, no build for this platform, or not installed and not allowed to be). A coverage gap, never a failure: the run goes on and
/// the rule is reported as not run. Only an in-process module may use it; from an external module it is a `protocol_mismatch`.
pub const TOOL_UNAVAILABLE: &str = "tool_unavailable";

/// The module's closing tally. The host does not trust it; its absence is what matters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModuleSummary {
    /// Rules the module was asked for.
    #[serde(default)]
    pub rules_requested: usize,
    /// Rules that ran.
    #[serde(default)]
    pub rules_ran: usize,
    /// Rules skipped.
    #[serde(default)]
    pub rules_skipped: usize,
    /// Rules that errored.
    #[serde(default)]
    pub rules_errored: usize,
    /// Findings sent.
    #[serde(default)]
    pub findings: usize,
    /// Whether the module says it did not finish.
    #[serde(default)]
    pub incomplete: bool,
}

/// One line a module prints while it checks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A finding (the shape `--message-format json` prints).
    Finding(Box<Finding>),
    /// A rule's committed result.
    RuleResult(RuleResult),
    /// Display-only progress: `(done, total, what)`.
    Progress(u64, u64, String),
    /// The closing tally.
    Summary(ModuleSummary),
}

impl Event {
    /// The JSON line for this event.
    pub fn to_line(&self) -> serde_json::Result<String> {
        let (reason, mut value) = match self {
            Event::Finding(f) => (FINDING_REASON, serde_json::to_value(f)?),
            Event::RuleResult(r) => (RULE_RESULT_REASON, serde_json::to_value(r)?),
            Event::Progress(done, total, what) => (
                PROGRESS_REASON,
                serde_json::json!({"done": done, "total": total, "what": what}),
            ),
            Event::Summary(s) => (SUMMARY_REASON, serde_json::to_value(s)?),
        };
        if let Some(object) = value.as_object_mut() {
            object.insert("reason".into(), reason.into());
        }
        serde_json::to_string(&value)
    }

    /// Reads one stdout line of a module. `Ok(None)` is a line with a `reason` this host does not know, which the
    /// protocol says to ignore; `Err` is a line that is not JSON, has no `reason`, or is a known event that is malformed.
    pub fn parse(line: &str) -> Result<Option<Event>, String> {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("not JSON ({e})"))?;
        let reason = value
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "no \"reason\" key".to_string())?
            .to_string();
        let malformed = |e: serde_json::Error| format!("a malformed {reason} line ({e})");
        Ok(Some(match reason.as_str() {
            FINDING_REASON => {
                Event::Finding(Box::new(serde_json::from_value(value).map_err(malformed)?))
            }
            RULE_RESULT_REASON => {
                Event::RuleResult(serde_json::from_value(value).map_err(malformed)?)
            }
            SUMMARY_REASON => Event::Summary(serde_json::from_value(value).map_err(malformed)?),
            PROGRESS_REASON => {
                let number = |key: &str| value.get(key).and_then(serde_json::Value::as_u64);
                Event::Progress(
                    number("done").unwrap_or(0),
                    number("total").unwrap_or(0),
                    value
                        .get("what")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                )
            }
            _ => return Ok(None),
        }))
    }
}
