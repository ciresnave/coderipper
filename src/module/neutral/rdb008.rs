// SPDX-License-Identifier: MIT OR Apache-2.0
//! RDB-008 (first part): a `TODO` or `FIXME` in a source comment names an owner or a ticket.
//!
//! **What this judges:** every tracked source file (by extension, outside the directories `conflict-marker` skips) is read line by
//! line, and the text of its comments is searched for the words `TODO` and `FIXME` (upper case, whole word, the first word of the comment). One is **owned** when
//! it is followed by a parenthesised or bracketed name (`TODO(eric)`, `TODO [eric]`), by `@name`, or when its comment line holds an issue reference (`#123`,
//! `PROJ-123`) or a link (`http://`, `https://`). An unowned one is a **Low** finding; a file gets one finding, at its first, with the
//! count. The rule's statement also asks for an expiry date; a date is not required here, because an issue is the expiry.
//!
//! **Comments by family:** `//` and `/* .. */` for C-like languages, Rust, Go, Java, JavaScript, TypeScript and friends; `#` for Python,
//! Ruby, shell, TOML, YAML and friends; `--` for SQL, Lua and Haskell. A `//` after a `:` is a URL, not a comment. A comment marker
//! inside a single-line string literal (`"`, and `'` or a backtick where the family has them) is not a comment; a string that spans lines
//! is not tracked, so a marker inside one is read as a comment (an `[[allow]]` entry on the file waives it).
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

/// The comment text of `line`, given whether a block comment is open at its start; and whether one is open at its end. A marker inside a
/// string literal on the same line is not a comment.
fn comment_of(line: &str, family: Family, in_block: bool) -> (Option<&str>, bool) {
    if family == Family::C && in_block {
        return match line.find("*/") {
            Some(i) => (Some(&line[..i]), false),
            None => (Some(line), true),
        };
    }
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' && family != Family::Dash {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied();
        match family {
            Family::Hash => {
                if c == b'#' {
                    return (Some(&line[i + 1..]), false);
                }
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                }
            }
            Family::Dash => {
                if c == b'-' && next == Some(b'-') {
                    return (Some(&line[i + 2..]), false);
                }
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                }
            }
            Family::C => {
                if c == b'/' && next == Some(b'/') && (i == 0 || bytes[i - 1] != b':') {
                    return (Some(&line[i + 2..]), false);
                }
                if c == b'/' && next == Some(b'*') {
                    let rest = &line[i + 2..];
                    return match rest.find("*/") {
                        Some(j) => (Some(&rest[..j]), false),
                        None => (Some(rest), true),
                    };
                }
                if c == b'"' || c == b'`' {
                    quote = Some(c);
                } else if c == b'\'' {
                    // A char literal ('x' or '\n') is skipped whole; a lone `'` (a Rust lifetime) opens nothing.
                    if next == Some(b'\\') {
                        if let Some(j) = line.get(i + 3..).and_then(|r| r.find('\'')) {
                            i += 3 + j;
                        }
                    } else if bytes.get(i + 2) == Some(&b'\'') {
                        i += 2;
                    }
                }
            }
        }
        i += 1;
    }
    (None, false)
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

/// Whether the comment `text` opens with a `TODO`/`FIXME` that names neither an owner nor a ticket. The marker must be the comment's first
/// word, so prose that mentions the word (or a file called `TODO.md`) is not a to-do.
fn unowned(text: &str) -> bool {
    let body = text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '/' | '!'));
    let Some(after) = ["TODO", "FIXME"].iter().find_map(|w| body.strip_prefix(w)) else {
        return false;
    };
    let mut rest = after.chars();
    let (next, then) = (rest.next(), rest.next());
    if !word_edge(next) || (next == Some('.') && then.is_some_and(char::is_alphanumeric)) {
        return false;
    }
    let named = after
        .strip_prefix('(')
        .and_then(|r| r.split_once(')'))
        .is_some_and(|(name, _)| !name.trim().is_empty())
        || after.trim_start().starts_with('@')
        || after
            .trim_start()
            .strip_prefix('[')
            .and_then(|r| r.split_once(']'))
            .is_some_and(|(name, _)| !name.trim().is_empty());
    !(named || has_reference(text))
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

/// Fewest consecutive code-shaped line comments that count as commented-out code.
const MIN_RUN: usize = 3;

/// Whether comment text `t` (one whole-line comment) looks like a line of code rather than prose: it ends in `;`, `{` or `}`, or opens
/// with a statement keyword and does not read as a sentence.
fn code_shaped(t: &str, family: Family) -> bool {
    let t = t.trim();
    if t.is_empty() || t.starts_with(['/', '!', '*']) {
        return false;
    }
    if t.ends_with(';') || t.ends_with('{') || t == "}" || t.ends_with("});") {
        return true;
    }
    let keywords: &[&str] = match family {
        Family::C => &[
            "let ", "fn ", "pub fn ", "use ", "const ", "return ", "if (", "for (", "while (",
        ],
        Family::Hash => &["def ", "import ", "from ", "return ", "self.", "print("],
        Family::Dash => &[],
    };
    keywords.iter().any(|k| t.starts_with(k))
        && !t.ends_with(['.', ':', '?'])
        && (t.contains('(') || t.contains('=') || t.contains("::"))
}

/// The 1-based first lines of runs of `MIN_RUN` or more consecutive whole-line comments that are all code-shaped. Only C-like and
/// Python/Ruby files are read; commented-out configuration (TOML, YAML) and shell are legitimate documentation and are not judged.
fn commented_out_runs(path: &str, text: &str) -> Vec<u32> {
    let Some(family) = family(path) else {
        return Vec::new();
    };
    let marker = match family {
        Family::C => "//",
        Family::Hash if path.ends_with(".py") || path.ends_with(".rb") => "#",
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    let (mut start, mut len) = (0usize, 0usize);
    for (i, line) in text.lines().chain(std::iter::once("")).enumerate() {
        let body = line.trim_start().strip_prefix(marker);
        if body.is_some_and(|b| code_shaped(b, family)) {
            if len == 0 {
                start = i + 1;
            }
            len += 1;
        } else {
            if len >= MIN_RUN {
                out.push(u32::try_from(start).unwrap_or(u32::MAX));
            }
            len = 0;
        }
    }
    out
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    // Commented-out code can be in any source file, so every file of a known family is read (not only those that name a to-do).
    let candidates: Option<Vec<String>> = None;
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
        let runs = commented_out_runs(path, &text);
        if let Some(first) = runs.first().copied() {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Low,
                    view.name(),
                    format!(
                        "{path} has {} run(s) of {MIN_RUN}+ commented-out code lines, the first at line {first}",
                        runs.len()
                    ),
                    "Delete the commented-out code; version history keeps it. If it is an example, move it into a doc comment.",
                )
                .location(Location::new(path.as_str(), Some(first)))
                .subject(path.as_str())
                .positive_control(format!(
                    "{read} source file(s) were read; the first commented-out run is at {path}:{first}"
                )),
            );
        }
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

    /// Sample sources spell the markers with zeros and a 1, so that this file holds no to-do of its own.
    fn lines(path: &str, text: &str) -> Vec<u32> {
        unowned_lines(
            path,
            &text.replace("T0D0", "TODO").replace("F1XME", "FIXME"),
        )
    }

    #[test]
    fn a_run_of_commented_out_code_is_found_and_prose_is_not() {
        let code = "// let x = 1;\n// foo(x);\n// bar();\nfn live() {}\n";
        assert_eq!(commented_out_runs("a.rs", code), vec![1]);
        assert!(commented_out_runs("a.rs", "// let x = 1;\n// foo(x);\n").is_empty());
        assert!(commented_out_runs(
            "a.rs",
            "// This is prose.\n// More prose here.\n// Even more.\n"
        )
        .is_empty());
        assert!(commented_out_runs("a.rs", "/// let x = 1;\n/// foo(x);\n/// bar();\n").is_empty());
        assert_eq!(
            commented_out_runs("a.py", "# self.x = f(a)\n# print(self.x)\n# return g(a);\n"),
            vec![1]
        );
        assert!(commented_out_runs("a.toml", "# a = 1;\n# b = 2;\n# c = 3;\n").is_empty());
    }

    #[test]
    fn a_bare_todo_is_found_and_an_owned_one_is_not() {
        let src = "// T0D0: later
// T0D0(eric): later
// F1XME #12 broken
// T0D0 @eric
let x = 1; // F1XME
/* T0D0 see https://x.test/1 */
";
        assert_eq!(lines("a.rs", src), vec![1, 5]);
    }

    #[test]
    fn only_a_comment_that_opens_with_the_marker_is_read() {
        let src = "let s = \"T0D0\";
let url = \"http://x\"; // fine
T0D0S();
// T0D0S
// MYT0D0
// reports a T0D0.md
// T0D0.md is a file
// the word T0D0 in prose
";
        assert!(lines("a.rs", src).is_empty());
    }

    #[test]
    fn a_marker_inside_a_string_literal_is_not_a_comment() {
        let src = "let a = \"// T0D0 x\";
let b = \"a \\\" // T0D0\";
let c = '\"'; // T0D0
let d: &'a str = \"x\"; // T0D0
let e = `# T0D0`;
";
        assert_eq!(lines("a.rs", src), vec![3, 4]);
        assert_eq!(
            lines(
                "a.py",
                "s = \"# T0D0\"
t = 'a' # T0D0
"
            ),
            vec![2]
        );
        assert!(lines(
            "a.sql",
            "select '-- F1XME';
"
        )
        .is_empty());
    }

    #[test]
    fn a_bracketed_name_is_an_owner() {
        let src = "// T0D0 [eric] later
// T0D0[eric]
// T0D0 [] later
// T0D0 [x
";
        assert_eq!(lines("a.rs", src), vec![3, 4]);
    }

    #[test]
    fn block_hash_and_dash_comments() {
        assert_eq!(
            lines(
                "a.js",
                "/*
 * T0D0: x
 */
x();
"
            ),
            vec![2]
        );
        assert_eq!(
            lines(
                "a.py",
                "x = 1  # T0D0 later
# T0D0(a) ok
"
            ),
            vec![1]
        );
        assert_eq!(
            lines(
                "a.sql",
                "-- F1XME
"
            ),
            vec![1]
        );
        assert!(lines(
            "a.md",
            "T0D0: prose
"
        )
        .is_empty());
    }

    #[test]
    fn ticket_keys_count_as_owners() {
        assert!(lines(
            "a.rs",
            "// T0D0 PROJ-42 later
"
        )
        .is_empty());
        assert_eq!(
            lines(
                "a.rs",
                "// T0D0 x-42 later
"
            ),
            vec![1]
        );
    }

    #[test]
    fn one_finding_per_file_and_fixtures_are_skipped() {
        let (two, one, owned) = (
            "// T0D0
// T0D0
"
            .replace("T0D0", "TODO"),
            "// T0D0
"
            .replace("T0D0", "TODO"),
            "// T0D0(x)
"
            .replace("T0D0", "TODO"),
        );
        let v = RepoView::from_files(&[
            ("src/a.rs", &two),
            ("fixtures/b.rs", &one),
            ("src/c.rs", &owned),
        ]);
        let Verdict::Findings(f) = judge(&v) else {
            panic!("expected findings")
        };
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("2 TODO"), "{:?}", f[0].summary);
    }
}
