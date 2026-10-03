use serde::{Deserialize, Serialize};

/// How bad a finding is, independent of how sure we are it's real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// How sure the check is that a finding is real, independent of how bad it would be.
///
/// Kept separate from [`Severity`] on purpose: a symbol-reference miss is a plausible defect at
/// `High` severity but only `Medium` confidence (name collisions, trait dispatch can hide a real
/// caller); a version mismatch is `High` severity *and* `High` confidence, no ambiguity once read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Where in a project a finding points, when it points anywhere specific.
///
/// Omitted (`None`) for project-level findings, e.g. "no branch protection" has no single line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: Option<u32>,
}

/// One thing a check found, in the shape every check emits into.
///
/// This shared shape is what lets the host triage findings from different checks into one ranked
/// list, instead of concatenating each check's own report format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub check_id: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub project: String,
    pub location: Option<Location>,
    /// The symbol the finding is about (a function name, ...), when it is about one. Together with
    /// `check_id` and `location.file` it is the finding's fingerprint, which is what an allowlist
    /// entry (`.coderipper.toml`) names. `None` for findings that aren't about a single symbol.
    #[serde(default)]
    pub subject: Option<String>,
    pub summary: String,
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
pub enum FindingError {
    #[error(
        "finding from check '{check_id}' claims an absence (\"{summary}\") but has no positive_control: {ABSENCE_CLAIM_HELP}"
    )]
    AbsenceClaimMissingControl { check_id: String, summary: String },
}

const ABSENCE_CLAIM_HELP: &str =
    "an absence claim without a positive control is indistinguishable from a broken query; set positive_control or don't emit the finding";

const ABSENCE_WORDS: &[&str] = &["zero", "no ", "none", "missing", "unreachable", "0 "];

impl Finding {
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
