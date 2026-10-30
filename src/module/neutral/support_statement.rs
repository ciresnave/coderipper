// SPDX-License-Identifier: MIT OR Apache-2.0
//! `security-support-statement`: a released project's `SECURITY.md` promises no support period and claims no support for an old line.
//!
//! **What this judges (PM task 2026-10-30):** the repository declares a version that could be published (the same test as
//! `patent-sweep-record`), tracks a `SECURITY.md` (at the root, `.github/` or `docs/`, the three places GitHub reads), and that file has a
//! section whose heading names "supported versions". Inside that section:
//! - **Medium:** a table row marked with a check mark (a check-mark emoji, a ballot box with a check, or `:white_check_mark:` /
//!   `:heavy_check_mark:`) for a version line **older** than the manifest's, or two or more rows marked. GitHub's own template is such a
//!   table (`| 5.1.x | :white_check_mark: |`); a project that fixes only its latest line cannot keep the promise a second marked row makes.
//! - **Medium:** a support period: a number (digits, or one to twelve, eighteen or twenty-four as words) followed by `month(s)` or
//!   `year(s)`, with an optional hyphen (`18 months`, `two years`, `3-year`). A period is a promise the project then has to keep.
//!
//! **What it cannot see:** whether the statement is *true* (a project may genuinely support two lines; it then opts out with an
//! `[[allow]]` entry and a reason, which stays visible); a promise worded without a number ("for the foreseeable future"); a policy kept
//! outside the file; a `SECURITY.md` with no such section or no file at all (not judged: this rule does not ask for one); manifests below
//! the root. A repository that is not releasable (`publish = false`, `"private": true`) is not judged.

use super::patent_sweep::{line_of, releasable};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::SECURITY_SUPPORT_STATEMENT;

/// Where GitHub looks for the file, in the order it prefers.
const PLACES: &[&str] = &["SECURITY.md", ".github/SECURITY.md", "docs/SECURITY.md"];

const MARKS: &[&str] = &[
    "\u{2705}",
    "\u{2714}",
    "\u{2611}",
    ":white_check_mark:",
    ":heavy_check_mark:",
    ":ballot_box_with_check:",
];

const NUMBER_WORDS: &[&str] = &[
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "eighteen",
    "twenty-four",
];

/// The 1-based `(first, last)` lines of the "supported versions" section: from its heading to the next heading of the same or a higher level.
fn section(text: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let level = |l: &str| {
        let t = l.trim_start();
        let n = t.bytes().take_while(|b| *b == b'#').count();
        (n > 0 && t[n..].starts_with(' ')).then_some(n)
    };
    let start = lines
        .iter()
        .position(|l| level(l).is_some() && l.to_ascii_lowercase().contains("supported version"))?;
    let depth = level(lines[start])?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| level(l).is_some_and(|n| n <= depth))
        .map_or(lines.len(), |i| start + 1 + i);
    Some((start + 1, end))
}

/// `(major, minor)` ordering key of a version line: `0.9` is `(0, 9)`, `2` is `(2, 0)`.
fn key(line: &str) -> (u64, u64) {
    let mut parts = line.split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor)
}

/// The version line a table's first cell names (`5.1.x`, `v0.14`, `**2.x**`), if it names one.
fn row_line(cell: &str) -> Option<String> {
    let c = cell
        .trim()
        .trim_matches(['*', '_', '`'])
        .trim_start_matches(['v', 'V']);
    if !c.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let digits: String = c
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    let digits = digits.trim_end_matches('.');
    let normalised = if digits.contains('.') {
        digits.to_string()
    } else {
        format!("{digits}.0")
    };
    line_of(&normalised)
}

/// A support period in `line` (lower-cased): the matched words.
fn period(line: &str) -> Option<String> {
    let words: Vec<&str> = line
        .split(|c: char| {
            c.is_whitespace() || matches!(c, ',' | ';' | '.' | '(' | ')' | '*' | '_' | '`')
        })
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        // "18-month", "3-year": one token, number then unit
        if let Some((n, unit)) = w.split_once('-') {
            if is_number(n) && is_unit(unit) {
                return Some((*w).to_string());
            }
        }
        if is_number(w) && words.get(i + 1).is_some_and(|u| is_unit(u)) {
            return Some(format!("{w} {}", words[i + 1]));
        }
    }
    None
}

fn is_number(w: &str) -> bool {
    (!w.is_empty() && w.bytes().all(|b| b.is_ascii_digit())) || NUMBER_WORDS.contains(&w)
}

fn is_unit(w: &str) -> bool {
    matches!(w, "month" | "months" | "year" | "years")
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let Some((manifest, version)) = releasable(view) else {
        return Verdict::NotApplicable(
            "no root Cargo.toml or package.json declares a version that could be published".into(),
        );
    };
    let Some(path) = PLACES.iter().find(|p| view.has(p)) else {
        return Verdict::NotApplicable(
            "the repository tracks no SECURITY.md (at the root, .github/ or docs/); this rule judges one that exists".into(),
        );
    };
    let Some(text) = view.read(path) else {
        return Verdict::Findings(Vec::new());
    };
    let Some((first, end)) = section(&text) else {
        return Verdict::NotApplicable(format!(
            "{path} has no heading that names supported versions"
        ));
    };
    let current = line_of(&version).map(|l| key(&l));
    let project = view.name();
    let advice = "Say that the latest release line receives security fixes, that an older line gets a backport only when it is small, and that no \
                  support period is promised. A table of marked lines or a stated period is a promise the project then has to keep. A project \
                  that really supports more than one line, or a period, opts out with an [[allow]] entry and a reason.";
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut marked: Vec<(usize, Option<String>)> = Vec::new();
    // `first` is the heading's 1-based line and `end` the 1-based last line of the section
    for n in first + 1..=end {
        let l = lines[n - 1];
        if l.trim_start().starts_with('|') && MARKS.iter().any(|m| l.contains(m)) {
            let cell = l
                .trim()
                .trim_start_matches('|')
                .split('|')
                .next()
                .unwrap_or("");
            marked.push((n, row_line(cell)));
        }
        if let Some(p) = period(&l.to_ascii_lowercase()) {
            found.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::Medium,
                    project,
                    format!("{path} promises a support period (\"{p}\") in its supported-versions section"),
                    advice.to_string(),
                )
                .location(Location::new(*path, Some(n as u32)))
                .subject(format!("{path}:period"))
                .positive_control(format!(
                    "{path} was read whole; its supported-versions section is lines {first} to {}",
                    end.max(first)
                )),
            );
        }
    }
    let older = marked.iter().find(|(_, line)| {
        matches!((line.as_deref().map(key), current), (Some(row), Some(cur)) if row < cur)
    });
    let at = if marked.len() >= 2 {
        marked.get(1)
    } else {
        older
    };
    if let Some((n, _)) = at {
        found.push(
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::Medium,
                project,
                format!(
                    "{path} marks {} as supported in a table, and {manifest} is at version {version}: only the latest line is fixed",
                    if marked.len() >= 2 { "more than one line" } else { "an older line" }
                ),
                advice.to_string(),
            )
            .location(Location::new(*path, Some(*n as u32)))
            .subject(format!("{path}:table"))
            .positive_control(format!(
                "{path} was read whole; {} marked row(s) in lines {first} to {}",
                marked.len(),
                end.max(first)
            )),
        );
    }
    Verdict::Findings(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARGO: &str = "[package]\nname = \"a\"\nversion = \"0.14.1\"\n";
    const TEXT: &str = "# Security policy\n\n## Supported versions\n\nThe latest release line receives security fixes.\n\nAn older line receives a backport when the backport is small. When it is not, we do not attempt it. The advisory then says that line is not fixed and recommends upgrading.\n\nNo support period is promised.\n";

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    #[test]
    fn the_statement_the_portfolio_uses_is_clean() {
        assert!(run(&[("Cargo.toml", CARGO), ("SECURITY.md", TEXT)]).is_empty());
        // the same file in `.github/` and `docs/`
        assert!(run(&[("Cargo.toml", CARGO), (".github/SECURITY.md", TEXT)]).is_empty());
    }

    #[test]
    fn a_check_mark_for_an_older_line_or_for_two_lines_is_found() {
        let older = "## Supported versions\n\n| Version | Supported |\n| ------- | --------- |\n| 0.13.x  | :white_check_mark: |\n| 0.12.x  | :x: |\n";
        let f = run(&[("Cargo.toml", CARGO), ("SECURITY.md", older)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(5));
        let two = older.replace(":x:", "\u{2705}").replace("0.12.x", "0.14.x");
        let f = run(&[("Cargo.toml", CARGO), ("SECURITY.md", &two)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("more than one line"), "{f:?}");
        // positive control for the quiet case: only the current line marked
        let current =
            "## Supported versions\n\n| 0.14.x | :white_check_mark: |\n| 0.13.x | :x: |\n";
        assert!(run(&[("Cargo.toml", CARGO), ("SECURITY.md", current)]).is_empty());
    }

    #[test]
    fn a_stated_period_is_found_by_digits_words_and_hyphens_but_not_by_no_support_period_is_promised(
    ) {
        for promise in [
            "Each line is supported for 18 months.",
            "We support a line for two years.",
            "A 3-year window.",
            "Security fixes for twelve months after release.",
        ] {
            let text = format!("## Supported versions\n\n{promise}\n");
            let f = run(&[("Cargo.toml", CARGO), ("SECURITY.md", &text)]);
            assert_eq!(f.len(), 1, "{promise}: {f:?}");
        }
        // a number that is not a period, and the sentence the portfolio uses
        let quiet = "## Supported versions\n\nVersion 2 of the protocol. No support period is promised. Reports are answered within 3 days.\n";
        assert!(run(&[("Cargo.toml", CARGO), ("SECURITY.md", quiet)]).is_empty());
    }

    #[test]
    fn only_the_supported_versions_section_is_read() {
        let text = "# Security policy\n\nReleases ship every 6 months.\n\n## Supported versions\n\nThe latest line.\n\n## Reporting\n\nWe answer in 2 years.\n";
        assert!(run(&[("Cargo.toml", CARGO), ("SECURITY.md", text)]).is_empty());
    }

    #[test]
    fn a_project_that_is_not_released_or_has_no_file_or_no_section_is_not_judged() {
        let private = "[package]\nname = \"a\"\nversion = \"0.1.0\"\npublish = false\n";
        let bad = "## Supported versions\n\nFor 5 years.\n";
        assert!(matches!(
            judge(&RepoView::from_files(&[
                ("Cargo.toml", private),
                ("SECURITY.md", bad)
            ])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[("Cargo.toml", CARGO)])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[
                ("Cargo.toml", CARGO),
                ("SECURITY.md", "# Security\n\nReport it.\n")
            ])),
            Verdict::NotApplicable(_)
        ));
    }
}
