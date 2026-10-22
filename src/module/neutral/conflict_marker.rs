// SPDX-License-Identifier: MIT OR Apache-2.0
//! conflict-marker: no tracked file holds an unresolved merge conflict.
//!
//! **What this judges:** every tracked text file (outside the directories below) is read line by line. A line that starts with
//! seven `<` and a space (or is those seven alone) opens a conflict region; a line that starts with seven `>` and a space closes
//! it. A region, or a closing line with no opening, is a finding at its first line. A line of seven `=` alone counts only inside
//! a region, because on its own it is a Markdown heading underline.
//!
//! **What it does not read, on purpose, so that a file that quotes the markers is not a defect:** a fenced code block in a
//! Markdown file; any file under a `fixtures`, `testdata`, `conformance`, `snapshots`, `golden` or vendored directory; patch files
//! (`.patch`, `.diff`, `.rej`, `.orig`); and a file with a NUL byte (binary) or over two megabytes.
//!
//! **How it reads:** git is asked once which tracked files have a line that could be a marker (`git grep`, binary files
//! excluded), and only those are read; when git cannot answer, every tracked file is read.
//!
//! **What it cannot see:** a marker that does not start its line (an indented one is not a marker); a conflict in the `|||||||`
//! (diff3) style is read through its `<<<<<<<` and `>>>>>>>` lines.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::CONFLICT_MARKER;

/// Directory names whose contents quote markers or are somebody else's.
pub(super) const SKIPPED: &[&str] = &[
    "fixtures",
    "fixture",
    "testdata",
    "test_data",
    "conformance",
    "snapshots",
    "golden",
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bower_components",
];

const PATCH_EXTENSIONS: &[&str] = &[".patch", ".diff", ".rej", ".orig"];
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s))
        || PATCH_EXTENSIONS.iter().any(|e| path.ends_with(e))
}

fn is_markdown(path: &str) -> bool {
    path.ends_with(".md") || path.ends_with(".markdown") || path.ends_with(".mdx")
}

fn open_marker(line: &str) -> bool {
    line == "<<<<<<<" || line.starts_with("<<<<<<< ")
}

fn close_marker(line: &str) -> bool {
    line == ">>>>>>>" || line.starts_with(">>>>>>> ")
}

/// A code fence opens or closes: up to three spaces, then three backticks or tildes.
fn fence(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    line.len() - trimmed.len() <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
}

/// What a file's conflict regions are: `(first line, last line, closed)`, and the lines of closing markers with no opening.
fn regions(path: &str, text: &str) -> (Vec<(u32, u32, bool)>, Vec<u32>) {
    let markdown = is_markdown(path);
    let (mut found, mut stray) = (Vec::new(), Vec::new());
    let mut open: Option<u32> = None;
    let mut in_fence = false;
    let mut last = 0u32;
    for (i, raw) in text.lines().enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        last = n;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if markdown && fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if open_marker(line) {
            if let Some(start) = open {
                found.push((start, n - 1, false));
            }
            open = Some(n);
        } else if close_marker(line) {
            match open.take() {
                Some(start) => found.push((start, n, true)),
                None => stray.push(n),
            }
        }
    }
    if let Some(start) = open {
        found.push((start, last, false));
    }
    (found, stray)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut findings = Vec::new();
    let mut read = 0usize;
    // Git names the files with a line that could be a marker; only those are read. A file git calls binary is not one of them.
    let candidates = view.paths_matching("^(<{7}|>{7})([ \r]|$)");
    let everything;
    let paths: Vec<&String> = match &candidates {
        Some(found) => found.iter().filter(|p| !skipped(p)).collect(),
        None => {
            everything = view.paths();
            everything.iter().filter(|p| !skipped(p)).collect()
        }
    };
    for path in paths {
        let Some(text) = view.read(path) else {
            continue;
        };
        if text.len() > MAX_BYTES || text.contains('\0') {
            continue;
        }
        read += 1;
        let (found, stray) = regions(path, &text);
        for (first, last, closed) in found {
            let what = if closed {
                format!("an unresolved merge conflict (lines {first}-{last})")
            } else {
                format!("a conflict marker opened at line {first} and never closed")
            };
            findings.push(
                Finding::new(
                    ID,
                    Severity::High,
                    Confidence::High,
                    view.name(),
                    format!("{path} holds {what}"),
                    "A merge was committed with its conflict markers still in the file, so the file is not what either side wrote. \
                     Resolve it: keep the intended lines and delete the `<<<<<<<`, `=======` and `>>>>>>>` lines. \
                     If the file quotes the markers on purpose, put them in a fenced code block.",
                )
                .location(Location::new(path.as_str(), Some(first)))
                .subject(path.as_str())
                .positive_control(format!(
                    "{read} tracked text file(s) were read line by line; the opening marker was read at {path}:{first}"
                )),
            );
        }
        for line in stray {
            findings.push(
                Finding::new(
                    ID,
                    Severity::High,
                    Confidence::High,
                    view.name(),
                    format!("{path} has a conflict closing marker at line {line} with no opening one"),
                    "Half of a conflict was resolved and the other half left behind. Delete the stray `>>>>>>>` line, and check what was lost with it.",
                )
                .location(Location::new(path.as_str(), Some(line)))
                .subject(path.as_str())
                .positive_control(format!(
                    "{read} tracked text file(s) were read line by line; the closing marker was read at {path}:{line}"
                )),
            );
        }
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seven of one character, so no line of this file is itself a marker.
    fn seven(c: char) -> String {
        c.to_string().repeat(7)
    }

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            Verdict::Noted(..) => unreachable!("this rule reports no notes"),
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    fn conflict() -> String {
        format!(
            "keep\n{} HEAD\nours\n{}\ntheirs\n{} branch\nafter\n",
            seven('<'),
            seven('='),
            seven('>')
        )
    }

    fn lines(f: &[Finding]) -> Vec<(String, Option<u32>)> {
        f.iter()
            .map(|f| {
                let l = f.location.as_ref().unwrap();
                (l.file.clone(), l.line)
            })
            .collect()
    }

    #[test]
    fn a_closed_region_is_one_finding_at_its_first_line() {
        let found = run(&[("docs/DEVICE-TESTS.md", &conflict())]);
        assert_eq!(
            lines(&found),
            vec![("docs/DEVICE-TESTS.md".into(), Some(2))]
        );
        assert!(found[0].summary.contains("lines 2-6"), "{:?}", found[0]);
    }

    #[test]
    fn crlf_files_are_read_the_same() {
        let found = run(&[("a.rs", &conflict().replace('\n', "\r\n"))]);
        assert_eq!(lines(&found), vec![("a.rs".into(), Some(2))]);
    }

    #[test]
    fn an_opening_marker_never_closed_is_a_finding() {
        let text = format!("x\n{} HEAD\nours\n", seven('<'));
        let found = run(&[("a.txt", &text)]);
        assert_eq!(lines(&found), vec![("a.txt".into(), Some(2))]);
        assert!(found[0].summary.contains("never closed"));
    }

    #[test]
    fn a_stray_closing_marker_is_a_finding() {
        let text = format!("x\n{} branch\n", seven('>'));
        let found = run(&[("a.txt", &text)]);
        assert_eq!(lines(&found), vec![("a.txt".into(), Some(2))]);
        assert!(found[0].summary.contains("no opening"));
    }

    #[test]
    fn a_heading_underline_is_not_a_marker() {
        let text = format!("Title\n{}\n\nbody\n", seven('='));
        assert!(run(&[("README.md", &text)]).is_empty());
    }

    #[test]
    fn a_marker_must_start_its_line_and_have_seven_characters() {
        let text = format!(
            "  {} HEAD\n{}x\n{}\n{} b\n{}\n",
            seven('<'),
            seven('<'),
            "<".repeat(6),
            ">".repeat(6),
            seven('=')
        );
        assert!(run(&[("a.txt", &text)]).is_empty());
    }

    #[test]
    fn a_markdown_page_that_quotes_the_markers_in_a_fence_is_clean_and_outside_one_it_is_not() {
        let quoted = format!("How to read one:\n\n```text\n{}\n```\n", conflict());
        assert!(run(&[("docs/merge.md", &quoted)]).is_empty());
        let tilde = format!("~~~\n{}~~~\n", conflict());
        assert!(run(&[("docs/merge.md", &tilde)]).is_empty());
        // after the fence closes, the page is read again
        let after = format!("```\nx\n```\n{}", conflict());
        assert_eq!(run(&[("docs/merge.md", &after)]).len(), 1);
        // a fence means nothing outside Markdown
        assert_eq!(
            run(&[("a.txt", &format!("```\n{}```\n", conflict()))]).len(),
            1
        );
    }

    #[test]
    fn fixtures_vendored_code_patches_and_binaries_are_not_read() {
        let c = conflict();
        for path in [
            "tests/fixtures/a.txt",
            "testdata/a.txt",
            "conformance/x/defective/a.txt",
            "vendor/lib/a.txt",
            "node_modules/p/a.txt",
            "change.patch",
            "a.rej",
        ] {
            assert!(run(&[(path, &c)]).is_empty(), "{path}");
        }
        let binary = format!("\0{c}");
        assert!(run(&[("blob.bin", &binary)]).is_empty());
        // positive control: the same text in an ordinary path is found
        assert_eq!(run(&[("src/a.txt", &c)]).len(), 1);
    }

    #[test]
    fn two_regions_in_two_files_are_three_findings() {
        let twice = format!("{}{}", conflict(), conflict());
        let found = run(&[
            ("a.txt", &twice),
            ("b.txt", &conflict()),
            ("c.txt", "fine\n"),
        ]);
        assert_eq!(
            lines(&found),
            vec![
                ("a.txt".into(), Some(2)),
                ("a.txt".into(), Some(9)),
                ("b.txt".into(), Some(2))
            ]
        );
    }
}
