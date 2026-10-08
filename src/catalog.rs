//! The rule catalog: one record per rule, as data (multi-language design §4).
//!
//! A [`Rule`] says what a rule claims, why, and how it behaves (scope, network, unit, default severity, how contested
//! it is). Records live in `rules/<domain>.toml` at the crate root, one `[[rule]]` table each, and are compiled in;
//! [`Catalog::builtin`] is the catalog of this release. They are written in CodeRipper's own words: a rule's
//! `kb_refs` cite the owner's knowledge base by ID only, never its text.
//!
//! A record file is parsed strictly: an unknown field, a missing field, a blank statement or a repeated ID is an error
//! that names the file, never a rule that quietly loads wrong. The five built-in checks keep their kebab-case IDs
//! (`reachability`, ...), so findings and allowlist entries do not change; a record may also answer to `aliases`, which
//! [`Catalog::canonical_id`] resolves.
//!
//! The format is TOML rather than the design's YAML: the crate already reads `.coderipper.toml`, and a TOML catalog
//! needs no second parser (the YAML crates are either deprecated or young).

use crate::check::{Network, Scope, Unit};
use crate::finding::{Confidence, Severity};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The built-in record files: `(file name, contents)`, one per domain.
const BUILTIN_FILES: &[(&str, &str)] = &[
    ("err.toml", include_str!("../rules/err.toml")),
    ("mod.toml", include_str!("../rules/mod.toml")),
    ("rdb.toml", include_str!("../rules/rdb.toml")),
    ("wsp.toml", include_str!("../rules/wsp.toml")),
];

/// Why a set of record files did not load. Every variant names the file (and the rule, once it is known).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// The file is not valid TOML, or a record has an unknown or missing field or a value of the wrong kind.
    #[error("{file}: {message}")]
    #[non_exhaustive]
    Parse {
        /// The record file.
        file: String,
        /// What the parser said.
        message: String,
    },
    /// A record parsed but says something that cannot be true (a blank statement, an ID that is not an ID).
    #[error("{file}: rule \"{id}\": {message}")]
    #[non_exhaustive]
    Invalid {
        /// The record file.
        file: String,
        /// The rule's `id` as written.
        id: String,
        /// What is wrong.
        message: String,
    },
    /// Two records, or a record's alias and another record, claim the same name.
    #[error("\"{name}\" is claimed twice: by {first} and by {second}")]
    #[non_exhaustive]
    Duplicate {
        /// The contested ID or alias.
        name: String,
        /// `file: rule` of the first claimant.
        first: String,
        /// `file: rule` of the second claimant.
        second: String,
    },
}

/// How strong the evidence is that a rule is worth following (the knowledge base's scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum EvidenceStrength {
    /// Empirical: measured outcomes.
    E,
    /// Consensus: widely agreed practice.
    C,
    /// Folklore: common belief without much support.
    F,
}

/// The knowledge base's verification tiers (V0 metadata ... V5): metadata about a rule, not a run tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum EvidenceTier {
    /// Deterministic, from files and metadata.
    V0,
    /// See the design's vocabulary section.
    V1,
    /// See the design's vocabulary section.
    V2,
    /// See the design's vocabulary section.
    V3,
    /// See the design's vocabulary section.
    V4,
    /// See the design's vocabulary section.
    V5,
}

/// A rule's default gating level (a project profile can raise or lower it; `--deny` still decides the exit code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Lifecycle {
    /// Newly written; expect noise.
    Experimental,
    /// Reported, not gating.
    Advisory,
    /// Gating.
    Blocking,
}

/// What a rule applies to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Applicability {
    /// Languages it applies to; `any` means all.
    pub languages: Vec<String>,
    /// Which code it judges (`first_party`, ...).
    pub scopes: Vec<String>,
}

/// One rule: what it claims and how it behaves. Read it through the accessors.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    id: String,
    title: String,
    domain: String,
    statement: String,
    rationale: String,
    evidence_tiers: Vec<EvidenceTier>,
    evidence_strength: EvidenceStrength,
    contested: bool,
    scope: Scope,
    network: Network,
    unit: Unit,
    executes_project_code: bool,
    applicability: Applicability,
    default_severity: Severity,
    confidence_class: Confidence,
    #[serde(default)]
    aliases: Vec<String>,
    kb_refs: Vec<String>,
    remediation: String,
    #[serde(default)]
    related: Vec<String>,
    lifecycle: Lifecycle,
}

impl Rule {
    /// The stable ID: a kebab-case name for the five original checks, the knowledge base's `XXX-nnn` for new rules.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// A short name, in CodeRipper's words.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// The three-letter domain (`WSP`, `RDB`, ...).
    pub fn domain(&self) -> &str {
        &self.domain
    }
    /// What the rule claims, paraphrased.
    pub fn statement(&self) -> &str {
        &self.statement
    }
    /// Why it matters.
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
    /// How strong the evidence for the rule is.
    pub fn evidence_strength(&self) -> EvidenceStrength {
        self.evidence_strength
    }
    /// The verification tiers the rule's evidence sits at (metadata, not a run tier).
    pub fn evidence_tiers(&self) -> &[EvidenceTier] {
        &self.evidence_tiers
    }
    /// Whether reasonable people disagree about the rule.
    pub fn contested(&self) -> bool {
        self.contested
    }
    /// Which repositories checking it reads.
    pub fn scope(&self) -> Scope {
        self.scope
    }
    /// Whether checking it leaves the machine; this decides the run tier.
    pub fn network(&self) -> Network {
        self.network
    }
    /// Whether one run judges a package or the whole repository.
    pub fn unit(&self) -> Unit {
        self.unit
    }
    /// Whether checking it runs the project's code (build scripts, config, plugins, tests).
    pub fn executes_project_code(&self) -> bool {
        self.executes_project_code
    }
    /// The languages and code the rule applies to.
    pub fn applicability(&self) -> &Applicability {
        &self.applicability
    }
    /// The severity a finding has unless a project overrides it.
    pub fn default_severity(&self) -> Severity {
        self.default_severity
    }
    /// How sure a finding of this rule usually is.
    pub fn confidence_class(&self) -> Confidence {
        self.confidence_class
    }
    /// Other names this rule answers to; [`Catalog::canonical_id`] resolves them.
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    /// Knowledge-base rule IDs that overlap this one (cited by ID only).
    pub fn kb_refs(&self) -> &[String] {
        &self.kb_refs
    }
    /// What to do about a finding.
    pub fn remediation(&self) -> &str {
        &self.remediation
    }
    /// IDs of related rules.
    pub fn related(&self) -> &[String] {
        &self.related
    }
    /// The default gating level.
    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }
}

/// One record file as written: a list of `[[rule]]` tables.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    #[serde(default)]
    rule: Vec<Rule>,
}

/// A loaded set of rules, in file order then record order.
#[derive(Debug, Clone)]
pub struct Catalog {
    rules: Vec<Rule>,
    /// Every ID and alias, to the index of the rule it names.
    by_name: HashMap<String, usize>,
}

impl Catalog {
    /// The catalog of this release: the records under `rules/`, compiled in.
    ///
    /// # Panics
    /// If a built-in record file is invalid. That is a defect in this crate that its own tests catch, not a runtime condition.
    pub fn builtin() -> &'static Catalog {
        static BUILTIN: OnceLock<Catalog> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            Catalog::parse(BUILTIN_FILES)
                .unwrap_or_else(|e| panic!("the built-in rule catalog is invalid: {e}"))
        })
    }

    /// Loads record files given as `(file name, contents)`; the name appears in every error.
    pub fn parse(files: &[(&str, &str)]) -> Result<Catalog, CatalogError> {
        let mut rules = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        let mut origin: Vec<String> = Vec::new();
        let mut file_of: Vec<String> = Vec::new();
        for (file, text) in files {
            let parsed: RecordFile = toml::from_str(text).map_err(|e| CatalogError::Parse {
                file: (*file).to_string(),
                message: e.to_string(),
            })?;
            for rule in parsed.rule {
                validate(file, &rule)?;
                let index = rules.len();
                let here = format!("{file}: {}", rule.id);
                origin.push(here.clone());
                file_of.push((*file).to_string());
                for name in std::iter::once(&rule.id).chain(&rule.aliases) {
                    if let Some(&earlier) = by_name.get(name) {
                        return Err(CatalogError::Duplicate {
                            name: name.clone(),
                            first: origin[earlier].clone(),
                            second: here,
                        });
                    }
                    by_name.insert(name.clone(), index);
                }
                rules.push(rule);
            }
        }
        for (rule, file) in rules.iter().zip(file_of) {
            for related in &rule.related {
                let problem = if *related == rule.id {
                    Some("lists itself as related")
                } else if !by_name.contains_key(related) {
                    Some("names a related rule that is not in the catalog")
                } else {
                    None
                };
                if let Some(message) = problem {
                    return Err(CatalogError::Invalid {
                        file: file.clone(),
                        id: rule.id.clone(),
                        message: format!("\"{related}\" {message}"),
                    });
                }
            }
        }
        Ok(Catalog { rules, by_name })
    }

    /// Every rule, in file order then record order.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The rule with this ID or alias.
    pub fn get(&self, id_or_alias: &str) -> Option<&Rule> {
        self.by_name.get(id_or_alias).map(|&i| &self.rules[i])
    }

    /// The canonical ID of the rule this ID or alias names. Findings carry the canonical ID.
    pub fn canonical_id(&self, id_or_alias: &str) -> Option<&str> {
        self.get(id_or_alias).map(Rule::id)
    }
}

/// An ID is ASCII letters, digits and hyphens, starting with a letter or digit: a kebab-case name or `XXX-nnn`.
fn is_id(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A knowledge-base ID is three capital letters, a hyphen and three digits.
fn is_kb_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 7
        && bytes[..3].iter().all(u8::is_ascii_uppercase)
        && bytes[3] == b'-'
        && bytes[4..].iter().all(u8::is_ascii_digit)
}

fn validate(file: &str, rule: &Rule) -> Result<(), CatalogError> {
    let invalid = |message: String| CatalogError::Invalid {
        file: file.to_string(),
        id: rule.id.clone(),
        message,
    };
    if !is_id(&rule.id) {
        return Err(invalid(
            "the id must be letters, digits and hyphens (a kebab-case name or XXX-nnn)".into(),
        ));
    }
    for (field, text) in [
        ("title", &rule.title),
        ("domain", &rule.domain),
        ("statement", &rule.statement),
        ("rationale", &rule.rationale),
        ("remediation", &rule.remediation),
    ] {
        if text.trim().is_empty() {
            return Err(invalid(format!("{field} is blank")));
        }
    }
    if rule.evidence_tiers.is_empty() {
        return Err(invalid("evidence_tiers is empty".into()));
    }
    if rule.applicability.languages.is_empty() || rule.applicability.scopes.is_empty() {
        return Err(invalid(
            "applicability names no languages or no scopes".into(),
        ));
    }
    let lists: [(&str, Vec<&String>); 5] = [
        (
            "applicability.languages",
            rule.applicability.languages.iter().collect(),
        ),
        (
            "applicability.scopes",
            rule.applicability.scopes.iter().collect(),
        ),
        ("aliases", rule.aliases.iter().collect()),
        ("kb_refs", rule.kb_refs.iter().collect()),
        ("related", rule.related.iter().collect()),
    ];
    for (field, items) in lists {
        if items.iter().any(|i| i.trim().is_empty()) {
            return Err(invalid(format!("{field} has a blank entry")));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(dup) = items.iter().find(|i| !seen.insert(**i)) {
            return Err(invalid(format!("{field} lists \"{dup}\" twice")));
        }
    }
    if rule.aliases.contains(&rule.id) {
        return Err(invalid("an alias repeats the rule's own id".into()));
    }
    if let Some(related) = rule.related.iter().find(|r| !is_id(r)) {
        return Err(invalid(format!("related entry \"{related}\" is not an id")));
    }
    if let Some(alias) = rule.aliases.iter().find(|a| !is_id(a)) {
        return Err(invalid(format!("alias \"{alias}\" is not an id")));
    }
    if let Some(kb) = rule.kb_refs.iter().find(|k| !is_kb_id(k)) {
        return Err(invalid(format!(
            "kb_refs entry \"{kb}\" is not a knowledge-base id (XXX-nnn)"
        )));
    }
    Ok(())
}
