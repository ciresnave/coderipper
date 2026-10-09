//! DOC-005: decision records have a valid status and consistent supersession links.
//!
//! **What this judges:** a decision record is a tracked `.md` file in a directory named `adr`, `adrs` or `decisions` (other than
//! a `README.md`, `index.md` or `template*`). Each record must state a status (YAML front matter `status:`, a `Status:` line, or
//! the line under a `## Status` heading) whose first word is `proposed`, `accepted`, `rejected`, `deprecated` or `superseded`.
//! A record that says it is superseded by another must point at a record that exists (by markdown link, else by number) and that
//! says it supersedes the first; a record that says it supersedes another must find that one marked superseded.
//!
//! **What it cannot see:** the third clause of the rule, that records refer to the rules that enforce them, is not judged. Records
//! outside those directories, other status vocabularies, and links written in prose without a number are not understood; a
//! repository with no such records is `not applicable`.

use super::{dir_of, file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = "DOC-005";

const STATUSES: &[&str] = &[
    "proposed",
    "accepted",
    "rejected",
    "deprecated",
    "superseded",
];

/// A decision record: its path, text, number (from the file name) and stated status.
struct Record {
    path: String,
    text: String,
    number: Option<u64>,
    /// The status as written, and the line it is on.
    status: Option<(String, usize)>,
}

impl Record {
    /// The status word, lower-cased and bare (`Superseded by ADR-3` is `superseded`).
    fn word(&self) -> Option<String> {
        let (value, _) = self.status.as_ref()?;
        Some(
            value
                .split_whitespace()
                .next()?
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase(),
        )
    }

    fn line(&self) -> Option<u32> {
        self.status
            .as_ref()
            .and_then(|(_, line)| u32::try_from(*line).ok())
    }
}

fn is_record(path: &str) -> bool {
    let name = file_name(path).to_lowercase();
    let dir = dir_of(path).to_lowercase();
    let last = dir.rsplit('/').next().unwrap_or(&dir);
    name.ends_with(".md")
        && matches!(last, "adr" | "adrs" | "decisions")
        && name != "readme.md"
        && name != "index.md"
        && !name.contains("template")
}

/// The record's number: the digits that begin its file name, after an optional `adr-`.
fn number_of(path: &str) -> Option<u64> {
    let name = file_name(path).to_lowercase();
    let name = name.strip_prefix("adr-").unwrap_or(&name);
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Every run of digits in `text`, as a number.
fn numbers_in(text: &str) -> Vec<u64> {
    let mut found = Vec::new();
    // a date (2024-03-01) in the same line is not a record number
    for token in text.split_whitespace().filter(|t| !is_date(t)) {
        let mut current = String::new();
        for c in token.chars().chain(std::iter::once(' ')) {
            if c.is_ascii_digit() {
                current.push(c);
            } else if !current.is_empty() {
                if let Ok(n) = current.parse() {
                    found.push(n);
                }
                current.clear();
            }
        }
    }
    found
}

/// Whether a word is a `yyyy-mm-dd`-shaped date, ignoring punctuation around it.
fn is_date(token: &str) -> bool {
    let core = token.trim_matches(|c: char| !c.is_ascii_digit());
    let parts: Vec<&str> = core.split('-').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Strips the decoration around a status value (`**Rejected**`, `[Accepted]`).
fn clean_value(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(['*', '_', '[', '`', '('])
        .trim_end_matches(['*', '_', ']', '`'])
        .trim()
        .to_string()
}

/// What follows a `status:` label at the start of `line` (any case), sliced from the original text: lower-casing can change a
/// string's length, so the label is compared as ASCII and the rest is never measured against a lower-cased copy.
fn after_status_label(line: &str) -> Option<&str> {
    let label = line.get(..7)?;
    label.eq_ignore_ascii_case("status:").then(|| &line[7..])
}

fn status_of(text: &str) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut start = 0;
    if lines.first().is_some_and(|l| l.trim() == "---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---") {
            for (i, line) in lines[1..=end].iter().enumerate() {
                if let Some(value) = after_status_label(line) {
                    return Some((clean_value(value), i + 2));
                }
            }
            start = end + 2;
        }
    }
    for (i, line) in lines.iter().enumerate().skip(start) {
        let bare = line.trim().trim_start_matches(['*', '-', '>', ' ']);
        if let Some(value) = after_status_label(bare) {
            return Some((clean_value(value), i + 1));
        }
        if bare.starts_with('#') {
            if let Some(value) = after_status_label(bare.trim_start_matches('#').trim_start()) {
                return Some((clean_value(value), i + 1));
            }
        }
        if bare.starts_with('#')
            && bare
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case("status")
        {
            let (j, next) = lines
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, l)| !l.trim().is_empty())?;
            return Some((clean_value(next), j + 1));
        }
    }
    None
}

/// The record paths a piece of text refers to, by markdown link relative to `from_dir`, else by the numbers in it.
fn referenced<'a>(text: &str, from_dir: &str, records: &'a [Record]) -> Vec<&'a Record> {
    let mut by_link = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = rest[..end].split('#').next().unwrap_or("");
        if target.ends_with(".md") {
            let resolved = resolve(from_dir, target);
            if let Some(record) = records.iter().find(|r| r.path == resolved) {
                by_link.push(record);
            }
        }
    }
    if !by_link.is_empty() {
        return by_link;
    }
    let wanted = numbers_in(text);
    records
        .iter()
        .filter(|r| r.number.is_some_and(|n| wanted.contains(&n)))
        .collect()
}

/// `dir/target` with `.` and `..` segments applied.
fn resolve(dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn finding(view: &RepoView, record: &Record, summary: String, detail: &str) -> Finding {
    Finding::new(
        ID,
        Severity::Low,
        Confidence::High,
        view.name(),
        summary,
        detail,
    )
    .location(Location::new(record.path.as_str(), record.line()))
    .subject(file_name(&record.path))
    // the summary can quote the record's own words (a status of "None" is an absence word), so every finding says what was read
    .positive_control(format!(
        "{} is a tracked decision record and was read",
        record.path
    ))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let records: Vec<Record> = view
        .paths()
        .iter()
        .filter(|p| is_record(p))
        .filter_map(|p| {
            let text = view.read(p)?;
            Some(Record {
                number: number_of(p),
                status: status_of(&text),
                path: p.clone(),
                text,
            })
        })
        .collect();
    if records.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no decision records (markdown files in a directory named adr, adrs or decisions)".into(),
        );
    }
    let mut findings = Vec::new();
    let stating = records.iter().filter(|r| r.status.is_some()).count();
    for record in &records {
        let from_dir = dir_of(&record.path);
        match (&record.status, record.word()) {
            (None, _) => findings.push(
                finding(
                    view,
                    record,
                    format!("{} states no status", record.path),
                    "A decision record says whether it is proposed, accepted, rejected, deprecated or superseded. Add a `Status:` line.",
                )
                .positive_control(format!(
                    "{} decision records were read; the status reader found a status line in {stating} of them",
                    records.len()
                )),
            ),
            (Some(_), None) => findings.push(finding(
                view,
                record,
                format!("{} has an empty status", record.path),
                "The status label is there but its value is not. Write one of the standard statuses after it.",
            )),
            (Some((value, _)), Some(word)) if !STATUSES.contains(&word.as_str()) => {
                findings.push(finding(
                    view,
                    record,
                    format!("{} has the status `{value}`, which is not one of {}", record.path, STATUSES.join(", ")),
                    "Use one of the standard statuses so the decision log can be read mechanically.",
                ));
            }
            (Some((value, _)), Some(word)) if word == "superseded" => {
                let replacement = referenced(value, from_dir, &records)
                    .into_iter()
                    .find(|r| r.path != record.path);
                match replacement {
                    None => findings.push(finding(
                        view,
                        record,
                        format!("{} is superseded by a record that is not in the log", record.path),
                        "The status points at a replacement that cannot be found among the decision records. Fix the link or the number.",
                    )),
                    Some(target) if !supersedes(target, record, &records) => findings.push(finding(
                        view,
                        record,
                        format!(
                            "{} is superseded by {}, which does not say it supersedes it",
                            record.path, target.path
                        ),
                        "Supersession links point both ways. Add `Supersedes ...` to the replacement.",
                    )),
                    Some(_) => {}
                }
            }
            _ => {}
        }
    }
    // a record that says it supersedes another, whose target is still current
    let mut reported: Vec<&str> = Vec::new();
    for newer in &records {
        for line in newer
            .text
            .lines()
            .filter(|l| l.to_lowercase().contains("supersedes"))
        {
            for older in referenced(line, dir_of(&newer.path), &records) {
                if older.path == newer.path
                    || older.word().as_deref() == Some("superseded")
                    || reported.contains(&older.path.as_str())
                {
                    continue;
                }
                reported.push(&older.path);
                findings.push(finding(
                    view,
                    older,
                    format!(
                        "{} is superseded by {} but is still marked {}",
                        older.path,
                        newer.path,
                        older.word().unwrap_or_else(|| "without a status".into())
                    ),
                    "Mark the older record `Superseded by ...`, so nobody follows a decision that was replaced.",
                ));
            }
        }
    }
    Verdict::Findings(findings)
}

/// Whether `newer` has a `supersedes` line that refers to `older` (by link, else by number).
fn supersedes(newer: &Record, older: &Record, records: &[Record]) -> bool {
    newer
        .text
        .lines()
        .filter(|l| l.to_lowercase().contains("supersedes"))
        .any(|line| {
            referenced(line, dir_of(&newer.path), records)
                .iter()
                .any(|r| r.path == older.path)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    fn files_of(found: &[Finding]) -> Vec<String> {
        let mut v: Vec<String> = found
            .iter()
            .map(|f| f.location.as_ref().unwrap().file.clone())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn consistent_records_are_clean() {
        let found = run(&[
            (
                "docs/adr/0001-use-x.md",
                "# 1. Use X\n\n## Status\n\nSuperseded by [2](0002-use-y.md)\n",
            ),
            (
                "docs/adr/0002-use-y.md",
                "# 2. Use Y\n\nStatus: Accepted\n\nSupersedes [1](0001-use-x.md)\n",
            ),
            ("docs/adr/README.md", "index"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_record_with_no_status_is_reported() {
        let found = run(&[
            ("docs/adr/0001-a.md", "# A\n\nWe decided.\n"),
            ("docs/adr/0002-b.md", "# B\n\nStatus: accepted\n"),
        ]);
        assert_eq!(files_of(&found), vec!["docs/adr/0001-a.md"]);
        assert!(found[0].clone().validate().is_ok());
    }

    #[test]
    fn a_status_outside_the_vocabulary_is_reported() {
        let found = run(&[("adr/0001-a.md", "# A\n\nStatus: Maybe\n")]);
        assert_eq!(files_of(&found), vec!["adr/0001-a.md"]);
        assert!(found[0].summary.contains("maybe") || found[0].summary.contains("Maybe"));
    }

    #[test]
    fn the_status_forms_front_matter_line_and_heading_are_all_read() {
        let found = run(&[
            (
                "docs/decisions/0001-a.md",
                "---\nstatus: accepted\n---\n# A\n",
            ),
            ("docs/decisions/0002-b.md", "# B\n\n* Status: Proposed\n"),
            (
                "docs/decisions/0003-c.md",
                "# C\n\n## Status\n\n**Rejected**\n",
            ),
            ("docs/decisions/template.md", "# T\n"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_supersession_link_to_a_record_that_does_not_exist_is_reported() {
        let found = run(&[(
            "docs/adr/0001-a.md",
            "# A\n\nStatus: Superseded by ADR-0009\n",
        )]);
        assert_eq!(files_of(&found), vec!["docs/adr/0001-a.md"]);
    }

    #[test]
    fn a_supersession_that_is_not_reciprocated_is_reported_on_the_old_record() {
        let found = run(&[
            (
                "docs/adr/0001-a.md",
                "# A\n\nStatus: Superseded by ADR-0002\n",
            ),
            ("docs/adr/0002-b.md", "# B\n\nStatus: Accepted\n"),
        ]);
        assert_eq!(files_of(&found), vec!["docs/adr/0001-a.md"]);
    }

    #[test]
    fn a_record_that_supersedes_another_which_is_still_current_is_reported_on_the_old_record() {
        let found = run(&[
            ("docs/adr/0001-a.md", "# A\n\nStatus: Accepted\n"),
            (
                "docs/adr/0002-b.md",
                "# B\n\nStatus: Accepted\n\nSupersedes ADR-0001\n",
            ),
        ]);
        assert_eq!(files_of(&found), vec!["docs/adr/0001-a.md"]);
    }

    #[test]
    fn a_status_that_quotes_an_absence_word_is_a_finding_not_a_rule_error() {
        // the summary quotes the record's own words; "None" must not make the finding invalid
        let view = RepoView::from_files(&[("docs/adr/0001-a.md", "# A\n\nStatus: None\n")]);
        let Verdict::Findings(found) = judge(&view) else {
            panic!("applicable")
        };
        assert_eq!(found.len(), 1);
        assert!(found[0].clone().validate().is_ok(), "{:?}", found[0]);
    }

    #[test]
    fn an_empty_status_value_is_reported() {
        let found = run(&[
            ("docs/adr/0001-a.md", "# A\n\n**Status:**\n"),
            ("docs/adr/0002-b.md", "# B\n\nStatus:\n"),
        ]);
        assert_eq!(
            files_of(&found),
            vec!["docs/adr/0001-a.md", "docs/adr/0002-b.md"]
        );
    }

    #[test]
    fn a_heading_that_carries_the_status_is_read() {
        let found = run(&[("docs/adr/0001-a.md", "# A\n\n## Status: Accepted\n")]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_date_beside_the_replacement_is_not_taken_for_a_record_number() {
        let found = run(&[
            ("docs/adr/0001-a.md", "Status: Accepted\n"),
            (
                "docs/adr/0002-b.md",
                "Status: Superseded by ADR-0005 (2024-03-01)\n",
            ),
            ("docs/adr/0003-c.md", "Status: Accepted\n"),
            (
                "docs/adr/0005-e.md",
                "Status: Accepted\n\nSupersedes ADR-0002 (2024-03-01)\n",
            ),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn template_files_and_upper_case_directories_are_understood() {
        let found = run(&[
            ("docs/adr/0000-template.md", "# Title\n"),
            ("docs/adr/adr-template.md", "# Title\n"),
            ("docs/ADR/0001-a.md", "Status: Accepted\n"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn non_ascii_text_around_the_status_does_not_confuse_or_panic_the_reader() {
        // 'İ' lower-cases to a longer string; slicing by a lower-cased length would cut it in the wrong place or panic
        let found = run(&[
            ("docs/adr/0001-a.md", "# İ\n\nStatus: İ accepted\n"),
            ("docs/adr/0002-b.md", "# B\n\nSTATUS: Accepted — für alle\n"),
        ]);
        assert_eq!(files_of(&found), vec!["docs/adr/0001-a.md"], "{found:?}");
    }

    #[test]
    fn a_repository_with_no_decision_records_is_not_applicable() {
        let view = RepoView::from_files(&[("README.md", "x"), ("docs/guide.md", "y")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
    }

    #[test]
    fn markdown_outside_a_decision_directory_is_not_a_record() {
        let found = run(&[
            ("docs/adr/0001-a.md", "Status: accepted\n"),
            ("docs/notes/0002-b.md", "no status here\n"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }
}
