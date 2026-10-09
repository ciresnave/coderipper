//! The coverage report (multi-language design section 6): which of the catalog's rules CodeRipper can check for a language, and
//! what this run found for them.
//!
//! Coverage is **computed**, never asserted: it joins the rule catalog with what a module claims in its hello and whether the
//! claim carries a proof (a conformance fixture, see [`crate::conformance`]). A claim with no proof is counted as a gap, not as
//! coverage. A language with no module has every rule not covered.
//!
//! A gap is a statement about CodeRipper, not about the code it checked: it means "not yet implemented", it is never a
//! finding, and it never changes a run's exit code (the owner's ruling, 2026-10-08).

use crate::catalog::{Catalog, Rule};
use crate::module::Hello;
use serde_json::json;

/// The most gaps a JSON coverage line lists before it says it is cut (the counts are never cut).
pub const GAP_LIST_LIMIT: usize = 50;

/// How one rule fared in this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunOutcome {
    /// It ran to completion and found nothing.
    Clean,
    /// It ran to completion and reported this many findings.
    Findings(usize),
    /// It did not apply to the project, as found out at run time. A gap, never clean.
    Skipped,
    /// It gave no verdict (an error). The audit is incomplete.
    CouldNotRun,
}

impl RunOutcome {
    /// Two outcomes for the same rule (two members of a workspace): the one that says more about the audit wins. A rule that
    /// could not run anywhere is incomplete; findings add up; a rule that ran somewhere is clean there even if it did not apply
    /// elsewhere.
    pub fn combine(self, other: RunOutcome) -> RunOutcome {
        use RunOutcome::{Clean, CouldNotRun, Findings, Skipped};
        match (self, other) {
            (CouldNotRun, _) | (_, CouldNotRun) => CouldNotRun,
            (Findings(a), Findings(b)) => Findings(a + b),
            (Findings(n), _) | (_, Findings(n)) => Findings(n),
            (Clean, _) | (_, Clean) => Clean,
            (Skipped, Skipped) => Skipped,
        }
    }
}

/// One rule and how it fared in this run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RuleRun {
    /// The rule's id.
    pub rule: String,
    /// How it fared.
    pub outcome: RunOutcome,
}

/// Folds `from` into `into`: one entry per rule, outcomes combined (see [`RunOutcome::combine`]).
pub fn merge_runs(into: &mut Vec<RuleRun>, from: Vec<RuleRun>) {
    for run in from {
        match into.iter_mut().find(|r| r.rule == run.rule) {
            Some(existing) => existing.outcome = existing.outcome.combine(run.outcome),
            None => into.push(run),
        }
    }
}

impl RuleRun {
    /// This rule's outcome.
    pub fn new(rule: impl Into<String>, outcome: RunOutcome) -> Self {
        Self {
            rule: rule.into(),
            outcome,
        }
    }
}

/// This run's outcomes counted by kind (a rule with findings counts once, however many findings it has).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunCounts {
    /// Rules that ran and found nothing.
    pub clean: usize,
    /// Rules that ran and found something.
    pub findings: usize,
    /// Rules that did not apply here.
    pub skipped: usize,
    /// Rules that gave no verdict.
    pub could_not_run: usize,
}

impl RunCounts {
    fn of(run: &[RuleRun]) -> Self {
        let mut counts = Self::default();
        for r in run {
            match r.outcome {
                RunOutcome::Clean => counts.clean += 1,
                RunOutcome::Findings(_) => counts.findings += 1,
                RunOutcome::Skipped => counts.skipped += 1,
                RunOutcome::CouldNotRun => counts.could_not_run += 1,
            }
        }
        counts
    }
}

/// What CodeRipper covers for one language.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LanguageCoverage {
    /// The language.
    pub language: String,
    /// The module that covers it, if one is active.
    pub module: Option<String>,
    /// The catalog rules that apply to the language.
    pub rules_total: usize,
    /// Rules the module claims with a proof: earned coverage.
    pub covered: usize,
    /// Rules the module claims without a proof. Counted as a gap, not as coverage.
    pub claimed_unproven: usize,
    /// Rules the module says do not apply to the language.
    pub not_applicable: usize,
    /// Rules nothing covers yet (a rule the module never mentions is one).
    pub not_covered: usize,
    /// The ids of the claimed-but-unproven and the not-covered rules, in catalog order.
    pub gaps: Vec<String>,
    /// How this run fared, when a run is being reported.
    pub run: Option<RunCounts>,
    /// For a language with no module: how many tracked source files were found and not checked.
    pub source_files: Option<usize>,
    /// A sentence about the language's state (`no module for typescript`).
    pub note: Option<String>,
    /// Problems with the module's claims (a rule outside the catalog, or one that does not apply to the language).
    pub problems: Vec<String>,
}

fn applies_to(rule: &Rule, language: &str) -> bool {
    rule.applicability()
        .languages
        .iter()
        .any(|l| l == "any" || l == language)
}

/// Coverage of `language` by the module that said `hello`, joined with the catalog and (optionally) this run's outcomes.
///
/// A claim counts as covered when it carries a `proof`, **and the proof string is trusted as given**: this function does not
/// check it. That is right for the built-in modules, whose proofs this repository's own tests tie to real conformance fixtures
/// (`tests/conformance.rs`). An external module's proofs must first be checked against its `conformance.lock` (design 6.2: the lock's
/// checksum must equal the executable's); no external module can reach this function until that check exists.
pub fn for_module(
    catalog: &Catalog,
    hello: &Hello,
    language: &str,
    run: Option<&[RuleRun]>,
) -> LanguageCoverage {
    let mut coverage = empty(catalog, language);
    coverage.module = Some(hello.module.clone());
    coverage.run = run.map(RunCounts::of);
    let applicable: Vec<&Rule> = catalog
        .rules()
        .iter()
        .filter(|r| applies_to(r, language))
        .collect();
    let mut claimed_rules: Vec<&str> = Vec::new();
    for claim in &hello.rules {
        if let Some(canonical) = catalog.canonical_id(&claim.id) {
            if claimed_rules.contains(&canonical) {
                coverage.problems.push(format!(
                    "the module claims \"{canonical}\" twice (by its id and an alias, or repeated); the first claim counts"
                ));
            }
            claimed_rules.push(canonical);
        }
        match catalog.get(&claim.id) {
            None => coverage.problems.push(format!(
                "the module claims \"{}\", which is not in the catalog",
                claim.id
            )),
            Some(rule) if !applies_to(rule, language) => coverage.problems.push(format!(
                "the module claims \"{}\", which does not apply to {language}",
                claim.id
            )),
            Some(_) => {}
        }
    }
    for rule in applicable {
        let claim = hello
            .rules
            .iter()
            .find(|c| catalog.canonical_id(&c.id) == Some(rule.id()));
        match claim {
            Some(c) if c.status == "not-applicable" => coverage.not_applicable += 1,
            Some(c) if c.proof.is_some() => coverage.covered += 1,
            Some(_) => {
                coverage.claimed_unproven += 1;
                coverage.gaps.push(rule.id().to_string());
            }
            None => {
                coverage.not_covered += 1;
                coverage.gaps.push(rule.id().to_string());
            }
        }
    }
    coverage
}

/// Coverage of a language that no active module handles: every rule is not covered, and the note says why.
pub fn for_missing_module(
    catalog: &Catalog,
    language: &str,
    source_files: usize,
) -> LanguageCoverage {
    let mut coverage = empty(catalog, language);
    coverage.not_covered = coverage.rules_total;
    coverage.gaps = catalog
        .rules()
        .iter()
        .filter(|r| applies_to(r, language))
        .map(|r| r.id().to_string())
        .collect();
    coverage.source_files = Some(source_files);
    coverage.note = Some(format!("no module for {language}"));
    coverage
}

fn empty(catalog: &Catalog, language: &str) -> LanguageCoverage {
    LanguageCoverage {
        language: language.to_string(),
        module: None,
        rules_total: catalog
            .rules()
            .iter()
            .filter(|r| applies_to(r, language))
            .count(),
        covered: 0,
        claimed_unproven: 0,
        not_applicable: 0,
        not_covered: 0,
        gaps: Vec::new(),
        run: None,
        source_files: None,
        note: None,
        problems: Vec::new(),
    }
}

impl LanguageCoverage {
    /// Covered rules as a percentage of the applicable ones (0 when none apply).
    pub fn percent_covered(&self) -> f64 {
        if self.rules_total == 0 {
            0.0
        } else {
            self.covered as f64 * 100.0 / self.rules_total as f64
        }
    }

    /// The `coderipper-coverage` JSON object, with at most [`GAP_LIST_LIMIT`] gaps listed.
    pub fn to_json(&self) -> serde_json::Value {
        self.to_json_with_gap_limit(GAP_LIST_LIMIT)
    }

    /// The `coderipper-coverage` JSON object, with at most `limit` gaps listed (the counts are never cut).
    pub fn to_json_with_gap_limit(&self, limit: usize) -> serde_json::Value {
        let mut value = json!({
            "reason": "coderipper-coverage",
            "language": self.language,
            "module": self.module,
            "rules_total": self.rules_total,
            "covered": self.covered,
            "claimed_unproven": self.claimed_unproven,
            "not_applicable": self.not_applicable,
            "not_covered": self.not_covered,
            "gaps": self.gaps.iter().take(limit).collect::<Vec<_>>(),
            "gaps_truncated": self.gaps.len() > limit,
        });
        let object = value.as_object_mut().expect("an object");
        if let Some(run) = self.run {
            object.insert(
                "run".into(),
                json!({
                    "clean": run.clean,
                    "findings": run.findings,
                    "skipped": run.skipped,
                    "could_not_run": run.could_not_run,
                }),
            );
        }
        if let Some(n) = self.source_files {
            object.insert("source_files".into(), json!(n));
        }
        if let Some(note) = &self.note {
            object.insert("note".into(), json!(note));
        }
        if !self.problems.is_empty() {
            object.insert("problems".into(), json!(self.problems));
        }
        value
    }

    /// One human line. A gap is "not yet implemented": CodeRipper's own incomplete coverage, not a statement about the code.
    pub fn human_line(&self) -> String {
        let mut line = format!(
            "coverage: {:<10} {} of {} covered ({:.1}%)",
            self.language,
            self.covered,
            self.rules_total,
            self.percent_covered()
        );
        match (&self.note, self.source_files) {
            (Some(note), Some(n)) => {
                let files = if n == 1 { "file" } else { "files" };
                line.push_str(&format!(", {note}: {n} source {files} not checked"));
            }
            (Some(note), None) => line.push_str(&format!(", {note}")),
            _ => {}
        }
        if self.module.is_some() || self.note.is_none() {
            line.push_str(&format!(", {} not yet implemented", self.not_covered));
        }
        if self.claimed_unproven > 0 {
            line.push_str(&format!(
                ", {} claimed but not yet proven",
                self.claimed_unproven
            ));
        }
        if let Some(run) = self.run {
            line.push_str(&format!(
                "   this run: {} clean, {} with findings, {} not applicable here, {} could not run",
                run.clean, run.findings, run.skipped, run.could_not_run
            ));
        }
        line
    }
}

/// A language the project contains that no active module checks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UncheckedLanguage {
    /// The language (`typescript`, `python`).
    pub language: String,
    /// How many of the project's tracked files are its source files.
    pub source_files: usize,
}

/// What marks a language as present (a manifest file name) and which extensions are its source. The list is the design's
/// (section 10.2) and grows with the modules.
const LANGUAGES: &[(&str, &[&str], &[&str])] = &[
    (
        "python",
        &["pyproject.toml", "setup.cfg", "setup.py"],
        &["py"],
    ),
    (
        "typescript",
        &["package.json", "tsconfig.json"],
        &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
    ),
];

fn is_manifest(file_name: &str, manifests: &[&str], language: &str) -> bool {
    manifests.contains(&file_name)
        || (language == "python"
            && file_name.starts_with("requirements")
            && file_name.ends_with(".txt"))
}

/// The languages `project_root` contains that CodeRipper has no module for: a manifest *and* at least one source file, among
/// the files git tracks there. A directory that is not a git repository yields nothing: the list comes from the index, never from
/// walking the disk (which would see other checkouts, ignored directories and `node_modules`). So a project outside git, or files
/// not yet added to it, are not seen here; the design's wording is "among the files git tracks".
pub fn unchecked_languages(project_root: &std::path::Path) -> Vec<UncheckedLanguage> {
    let Ok(output) = crate::conformance::git_command(project_root)
        .args(["ls-files", "-z"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let files: Vec<&str> = listing.split('\0').filter(|f| !f.is_empty()).collect();
    let mut found = Vec::new();
    for (language, manifests, extensions) in LANGUAGES {
        let file_name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
        let has_manifest = files
            .iter()
            .any(|f| is_manifest(&file_name(f), manifests, language));
        let source_files = files
            .iter()
            .filter(|f| {
                std::path::Path::new(f)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| extensions.contains(&e))
            })
            .count();
        if has_manifest && source_files > 0 {
            found.push(UncheckedLanguage {
                language: (*language).to_string(),
                source_files,
            });
        }
    }
    found
}
