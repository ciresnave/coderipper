//! [`Finding`], the one shape every check reports in, with its [`Severity`] and [`Confidence`].

use serde::{Deserialize, Serialize};

/// How bad a finding is, independent of how sure we are it's real.
///
/// `#[non_exhaustive]`: a variant may be added in a minor release, so an outside `match` needs a wildcard arm.
///
/// ```compile_fail,E0004
/// use coderipper::finding::Severity;
/// fn rank(s: Severity) -> u8 {
///     match s {
///         Severity::Info => 0,
///         Severity::Low => 1,
///         Severity::Medium => 2,
///         Severity::High => 3,
///         Severity::Critical => 4,
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    /// Worth knowing, nothing to fix (a stale allowlist entry, a repository too young to judge).
    Info,
    /// A defect with a small cost.
    Low,
    /// A defect that will cost someone time.
    Medium,
    /// A defect that is likely to cause a failure.
    High,
    /// A defect that is a failure.
    Critical,
}

/// How sure the check is that a finding is real, independent of how bad it would be.
///
/// Kept separate from [`Severity`] on purpose: a symbol-reference miss is a plausible defect at
/// `High` severity but only `Medium` confidence (name collisions, trait dispatch can hide a real
/// caller); a version mismatch is `High` severity *and* `High` confidence, no ambiguity once read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Confidence {
    /// A plausible reading; expect false positives.
    Low,
    /// Probably real; something the check cannot see could change the verdict.
    Medium,
    /// Read directly from the project, no ambiguity.
    High,
}

/// Where in a project a finding points, when it points anywhere specific.
///
/// Omitted (`None`) for project-level findings, e.g. "no branch protection" has no single line.
///
/// Build one with [`Location::new`]: the struct is `#[non_exhaustive]`, so a struct literal is rejected outside this
/// crate and a field can be added later without breaking anyone.
///
/// ```compile_fail,E0639
/// use coderipper::finding::Location;
/// let _ = Location { file: "src/lib.rs".to_string(), line: None };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Location {
    /// The file, relative to the project, with forward slashes. A check about something that is not a file may name its
    /// own place (the CI-protection check uses `github:branch-protection`).
    pub file: String,
    /// The line, when the finding is about one line.
    pub line: Option<u32>,
}

impl Location {
    /// A location in `file` (relative to the project, forward slashes), at `line` when it is about one line.
    pub fn new(file: impl Into<String>, line: Option<u32>) -> Self {
        Self {
            file: file.into(),
            line,
        }
    }
}

/// One thing a check found, in the shape every check emits into.
///
/// This shared shape is what lets the host triage findings from different checks into one ranked
/// list, instead of concatenating each check's own report format.
///
/// Build one with [`Finding::new`] and its chained setters: the struct is `#[non_exhaustive]`, so a struct literal is
/// rejected outside this crate and a field can be added later without breaking anyone.
///
/// ```compile_fail,E0639
/// use coderipper::finding::{Confidence, Finding, Severity};
/// let _ = Finding {
///     check_id: "c".to_string(),
///     severity: Severity::Low,
///     confidence: Confidence::Low,
///     project: "p".to_string(),
///     location: None,
///     subject: None,
///     summary: "s".to_string(),
///     detail: "d".to_string(),
///     member: None,
///     positive_control: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Finding {
    /// The id of the check that reported it ([`Check::id`](crate::check::Check::id)).
    pub check_id: String,
    /// How bad it is.
    pub severity: Severity,
    /// How sure the check is that it is real.
    pub confidence: Confidence,
    /// The project it is about, by directory name.
    pub project: String,
    /// Where it points, when it points anywhere specific.
    pub location: Option<Location>,
    /// The symbol the finding is about (a function name, ...), when it is about one. Together with
    /// `check_id` and `location.file` it is the finding's fingerprint, which is what an allowlist
    /// entry (`.coderipper.toml`) names. `None` for findings that aren't about a single symbol.
    #[serde(default)]
    pub subject: Option<String>,
    /// One line: what is wrong.
    pub summary: String,
    /// Why it matters and how to resolve it.
    pub detail: String,
    /// The cargo PACKAGE this finding came from, set by a `--workspace` run (`project` is the member's directory name,
    /// which two members can share). `None` outside a workspace run, and for findings about the whole repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// Required whenever the finding's core claim is an absence ("zero callers", "no protection
    /// enforced", "no matching version"). A finding making that kind of claim with this unset is a
    /// bug in the check that produced it, not evidence — see `Finding::validate`.
    pub positive_control: Option<String>,
}

/// Findings whose language signals an absence claim ("zero", "no", "none", "missing", "unreachable")
/// but carry no positive control are a defect in the check, not a real finding — reject them at
/// construction rather than let a broken check's output reach a report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FindingError {
    /// A finding says something is absent but gives no way to tell that from a broken query.
    #[error(
        "finding from check '{check_id}' claims an absence (\"{summary}\") but has no positive_control: {ABSENCE_CLAIM_HELP}"
    )]
    AbsenceClaimMissingControl {
        /// The check that emitted the finding.
        check_id: String,
        /// The finding's summary, which made the claim.
        summary: String,
    },
}

const ABSENCE_CLAIM_HELP: &str =
    "an absence claim without a positive control is indistinguishable from a broken query; set positive_control or don't emit the finding";

const ABSENCE_WORDS: &[&str] = &["zero", "no ", "none", "missing", "unreachable", "0 "];

impl Finding {
    /// A finding with the six parts every finding has; the optional parts start empty and are set with the chained
    /// setters ([`Finding::location`], [`Finding::subject`], [`Finding::positive_control`], [`Finding::member`]).
    ///
    /// ```
    /// use coderipper::finding::{Confidence, Finding, Location, Severity};
    ///
    /// let finding = Finding::new(
    ///     "my-check", Severity::Low, Confidence::High, "my-project", "an unused helper", "nothing calls it",
    /// )
    /// .location(Location::new("src/util.rs", Some(12)))
    /// .subject("util::helper");
    /// assert_eq!(finding.subject.as_deref(), Some("util::helper"));
    /// ```
    pub fn new(
        check_id: impl Into<String>,
        severity: Severity,
        confidence: Confidence,
        project: impl Into<String>,
        summary: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            check_id: check_id.into(),
            severity,
            confidence,
            project: project.into(),
            location: None,
            subject: None,
            summary: summary.into(),
            detail: detail.into(),
            member: None,
            positive_control: None,
        }
    }

    /// Where the finding points.
    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// The symbol the finding is about: with `check_id` and the location's file it is what an allowlist entry names.
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// What proves the claim is not a broken query (required when the finding claims an absence).
    pub fn positive_control(mut self, control: impl Into<String>) -> Self {
        self.positive_control = Some(control.into());
        self
    }

    /// The cargo package a workspace run found it in.
    pub fn member(mut self, member: impl Into<String>) -> Self {
        self.member = Some(member.into());
        self
    }

    /// Rejects an absence-claiming finding that has no positive control. Call this before a check
    /// hands a finding to the host, not just at the end of a report — a caller that only validates
    /// the final report can't tell which check produced a bad finding.
    pub fn validate(self) -> Result<Self, FindingError> {
        let claims_absence = ABSENCE_WORDS
            .iter()
            .any(|w| self.summary.to_lowercase().contains(w));
        if claims_absence && self.positive_control.is_none() {
            return Err(FindingError::AbsenceClaimMissingControl {
                check_id: self.check_id,
                summary: self.summary,
            });
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(summary: &str, positive_control: Option<&str>) -> Finding {
        Finding {
            check_id: "reachability".into(),
            severity: Severity::High,
            confidence: Confidence::Medium,
            project: "lightbulb".into(),
            location: Some(Location {
                file: "src/model_fuel/policies.rs".into(),
                line: Some(626),
            }),
            subject: Some("splice_prefix".into()),
            summary: summary.into(),
            detail: "detail text".into(),
            positive_control: positive_control.map(str::to_string),
            member: None,
        }
    }

    #[test]
    fn a_finding_round_trips_through_json() {
        let f = sample("zero callers found for splice_prefix", Some("ctl"));
        let json = serde_json::to_string(&f).expect("serialize");
        let back: Finding = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.check_id, f.check_id);
        assert_eq!(back.summary, f.summary);
    }

    #[test]
    fn a_finding_written_before_subject_existed_still_deserializes() {
        // Old JSON reports have no `subject`; the field must default to None, not fail.
        let json = r#"{"check_id":"reachability","severity":"high","confidence":"medium","project":"p","location":null,"summary":"s","detail":"d","positive_control":null}"#;
        let f: Finding = serde_json::from_str(json).expect("deserialize");
        assert_eq!(f.subject, None);
    }

    #[test]
    fn an_absence_claim_without_a_positive_control_is_rejected() {
        let f = sample("zero callers found for splice_prefix", None);
        let err = f.validate().unwrap_err();
        assert!(matches!(
            err,
            FindingError::AbsenceClaimMissingControl { .. }
        ));
    }

    #[test]
    fn an_absence_claim_with_a_positive_control_is_accepted() {
        let f = sample(
            "zero callers found for splice_prefix",
            Some("record found for a known-used sibling symbol"),
        );
        assert!(f.validate().is_ok());
    }

    #[test]
    fn a_non_absence_claim_needs_no_control() {
        let f = sample("version mismatch: crate a=0.3.0, crate b=0.2.9", None);
        assert!(f.validate().is_ok());
    }

    #[test]
    fn severity_and_confidence_order_independently() {
        // Sabotage-style check that the two enums don't get conflated: a High-severity,
        // Low-confidence finding and a Low-severity, High-confidence finding must both exist and
        // compare correctly on their own axis.
        assert!(Severity::High > Severity::Low);
        assert!(Confidence::High > Confidence::Low);
    }
}
