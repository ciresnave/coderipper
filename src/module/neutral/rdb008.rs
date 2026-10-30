// SPDX-License-Identifier: MIT OR Apache-2.0
//! RDB-008 (first part): a `TODO` or `FIXME` in a source comment names an owner or a ticket.
//!
//! **What this judges:** every tracked source file (by extension, outside the directories `conflict-marker` skips) is read line by
//! line, and the text of its comments is searched for the words `TODO` and `FIXME` (upper case, whole word). One is **owned** when
//! it is followed by a parenthesised name (`TODO(eric)`), by `@name`, or when its comment line holds an issue reference (`#123`,
//! `PROJ-123`) or a link (`http://`, `https://`). An unowned one is a **Low** finding; a file gets one finding, at its first, with the
//! count. The rule's statement also asks for an expiry date; a date is not required here, because an issue is the expiry.
//!
//! **Comments by family:** `//` and `/* .. */` for C-like languages, Rust, Go, Java, JavaScript, TypeScript and friends; `#` for Python,
//! Ruby, shell, TOML, YAML and friends; `--` for SQL, Lua and Haskell. A `//` after a `:` is a URL, not a comment. A comment marker
//! inside a string literal is read as a comment (a known false positive, which an `[[allow]]` entry on the file waives).
//!
//! **What it cannot see:** the second half of the rule, commented-out code; an owner written on the line above; a to-do in prose (a
//! Markdown file, a doc page); languages not listed; a marker in a language that nests block comments.

use super::conflict_marker::SKIPPED;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::RDB_008;

const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    C,
    Hash,
    Dash,
}

fn family(path: &str) -> Option<Family> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "java" | "js" | "jsx" | "mjs" | "cjs"
        | "ts" | "tsx" | "go" | "cs" | "kt" | "swift" | "scala" | "php" | "dart" => Family::C,
        "py" | "rb" | "sh" | "bash" | "zsh" | "toml" | "yaml" | "yml" | "pl" | "r" | "ps1" => {
            Family::Hash
        }
        "sql" | "lua" | "hs" => Family::Dash,
        _ => return None,
    })
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s))
}

/// The comment text of `line`, given whether a block comment is open at its start; and whether one is open at its end.
fn comment_of(line: &str, family: Family, in_block: bool) -> (Option<&str>, bool) {
    match family {
        Family::Hash => (line.find('#').map(|i| &line[i + 1..]), false),
        Family::Dash => (line.find("--").map(|i| &line[i + 2..]), false),
        Family::C => {
            if in_block {
                return match line.find("*/") {
                    Some(i) => (Some(&line[..i]), false),
                    None => (Some(line), true),
                };
            }
            let bytes = line.as_bytes();
            let mut i = 0;
            while i + 1 < bytes.len() {
                if bytes[i] == b'/' && bytes[i + 1] == b'/' && (i == 0 || bytes[i - 1] != b':') {
                    return (Some(&line[i + 2..]), false);
                }
                if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    let rest = &line[i + 2..];
                    return match rest.find("*/") {
                        Some(j) => (Some(&rest[..j]), false),
                        None => (Some(rest), true),
                    };
                }
                i += 1;
            }
            (None, false)
        }
    }
}

fn has_reference(text: &str) -> bool {
    if text.contains("http://") || text.contains("https://") {
        return true;
    }
    let b = text.as_bytes();
    for i in 0..b.len() {
        // #123
        if b[i] == b'#' && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
            return true;
        }
        // PROJ-123: two or more capitals, a hyphen, digits
        if b[i] == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
            let caps = b[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_uppercase())
                .count();
            if caps >= 2 {
                return true;
            }
        }
    }
    false
}

fn word_edge(c: Option<char>) -> bool {
    c.is_none_or(|c| !c.is_alphanumeric() && c != '_')
}

/// Whether `text` holds a `TODO`/`FIXME` and none of them names an owner or a ticket.
fn unowned(text: &str) -> bool {
    let mut found = false;
    for word in ["TODO", "FIXME"] {
        let mut from = 0;
        while let Some(i) = text[from..].find(word) {
            let at = from + i;
            let after = &text[at + word.len()..];
            from = at + word.len();
            if !(word_edge(text[..at].chars().next_back()) && word_edge(after.chars().next())) {
                continue;
            }
            found = true;
            let named = after
                .strip_prefix('(')
                .and_then(|r| r.split_once(')'))
                .is_some_and(|(name, _)| !name.trim().is_empty())
                || after.trim_start().starts_with('@');
            if named || has_reference(text) {
                return false;
            }
        }
    }
    found
}

/// The 1-based lines of a file's unowned to-dos.
fn unowned_lines(path: &str, text: &str) -> Vec<u32> {
    let Some(family) = family(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_block = false;
    for (i, line) in text.lines().enumerate() {
        let (comment, open) = comment_of(line, family, in_block);
        in_block = open;
        if comment.is_some_and(unowned) {
            out.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
        }
    }
    out
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let candidates = view.paths_matching("(TODO|FIXME)");
    let everything;
    let paths: Vec<&String> = match &candidates {
        Some(found) => found
            .iter()
            .filter(|p| !skipped(p) && family(p).is_some())
            .collect(),
        None => {
            everything = view.paths();
            everything
                .iter()
                .filter(|p| !skipped(p) && family(p).is_some())
                .collect()
        }
    };
    let mut findings = Vec::new();
    let mut read = 0usize;
    for path in paths {
        let Some(text) = view.read(path) else {
            continue;
        };
        if text.len() > MAX_BYTES || text.contains('\0') {
            continue;
        }
        read += 1;
        let lines = unowned_lines(path, &text);
        let Some(first) = lines.first().copied() else {
            continue;
        };
        findings.push(
            Finding::new(
                ID,
                Severity::Low,
                Confidence::Medium,
                view.name(),
                format!(
                    "{path} has {} TODO/FIXME comment(s) with no owner or ticket, the first at line {first}",
                    lines.len()
                ),
                "Name who will do it and by when: `TODO(name): ...`, `TODO #123: ...`, or a link to the issue. Or do it, or delete it.",
            )
            .location(Location::new(path.as_str(), Some(first)))
            .subject(path.as_str())
            .positive_control(format!(
                "{read} source file(s) with a TODO or FIXME were read; the first unowned one is at {path}:{first}"
            )),
        );
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_todo_is_found_and_an_owned_one_is_not() {
        let src = "// TODO: later\n// TODO(eric): later\n// FIXME #12 broken\n// TODO @eric\nlet x = 1; // FIXME\n/* TODO see https://x.test/1 */\n";
        assert_eq!(unowned_lines("a.rs", src), vec![1, 5]);
    }

    #[test]
    fn only_comments_are_read() {
        let src =
            "let s = \"TODO\";\nlet url = \"http://x\"; // fine\nTODOS();\n// TODOS\n// MYTODO\n";
        assert!(unowned_lines("a.rs", src).is_empty());
    }

    #[test]
    fn block_hash_and_dash_comments() {
        assert_eq!(
            unowned_lines("a.js", "/*\n * TODO: x\n */\nx();\n"),
            vec![2]
        );
        assert_eq!(
            unowned_lines("a.py", "x = 1  # TODO later\n# TODO(a) ok\n"),
            vec![1]
        );
        assert_eq!(unowned_lines("a.sql", "-- FIXME\n"), vec![1]);
        assert!(unowned_lines("a.md", "TODO: prose\n").is_empty());
    }

    #[test]
    fn ticket_keys_count_as_owners() {
        assert!(unowned_lines("a.rs", "// TODO PROJ-42 later\n").is_empty());
        assert_eq!(unowned_lines("a.rs", "// TODO x-42 later\n"), vec![1]);
    }

    #[test]
    fn one_finding_per_file_and_fixtures_are_skipped() {
        let v = RepoView::from_files(&[
            ("src/a.rs", "// TODO\n// TODO\n"),
            ("fixtures/b.rs", "// TODO\n"),
            ("src/c.rs", "// TODO(x)\n"),
        ]);
        let Verdict::Findings(f) = judge(&v) else {
            panic!("expected findings")
        };
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("2 TODO"), "{:?}", f[0].summary);
    }
}
