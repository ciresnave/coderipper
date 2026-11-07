// SPDX-License-Identifier: MIT OR Apache-2.0
//! What the Rust source checks of this module share: which tracked files are production source, which items are test code, and
//! whether a comment sits beside a line.

use super::conflict_marker::SKIPPED;
use super::efx003::is_dev_path;
use super::RepoView;
use syn::Attribute;

/// Larger files are not read (generated code).
pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;

/// The tracked `.rs` files that are production source: outside the directories `conflict-marker` skips, development tooling and
/// test directories, `build.rs` and `tests.rs`-style files.
pub(super) fn production_files(view: &RepoView) -> Vec<String> {
    view.paths()
        .iter()
        .filter(|p| p.ends_with(".rs"))
        .filter(|p| {
            let segments: Vec<&str> = p.split('/').collect();
            !segments[..segments.len() - 1]
                .iter()
                .any(|s| SKIPPED.contains(s))
                && !is_dev_path(p)
        })
        .cloned()
        .collect()
}

/// The 1-based line a span starts on.
pub(super) fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

/// Whether the attributes mark test code: `#[test]`, `#[tokio::test]`, or a `#[cfg(..)]` that names `test`.
pub(super) fn is_test_item(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "test" || s.ident == "bench")
            || (a.path().is_ident("cfg")
                && a.meta.require_list().is_ok_and(|l| {
                    l.tokens
                        .to_string()
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|t| t == "test")
                }))
    })
}

/// The text of the comment on `line`, from its `//` or `/*` on: not one preceded by `:` (a URL), and not one inside a string.
pub(super) fn comment_text(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str
                && i + 1 < bytes.len()
                && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*')
                && (i == 0 || bytes[i - 1] != b':') =>
            {
                return Some(&line[i..]);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn has_comment(line: &str) -> bool {
    comment_text(line).is_some()
}

/// Whether a comment sits on the previous line (when it is only a comment) or on any of the lines `first..=last` (1-based).
pub(super) fn comment_beside(lines: &[&str], first: u32, last: u32) -> bool {
    let first = first.max(1) as usize;
    let last = (last as usize).max(first);
    if first >= 2 {
        let prev = lines.get(first - 2).map_or("", |l| l.trim_start());
        if prev.starts_with("//") || prev.ends_with("*/") {
            return true;
        }
    }
    (first..=last).any(|n| lines.get(n - 1).is_some_and(|l| has_comment(l)))
}

/// The last line of a statement that starts at `first`: the first line at or after it that holds a `;` (at most 8 lines on).
pub(super) fn statement_end(lines: &[&str], first: u32) -> u32 {
    let from = first.max(1) as usize;
    (from..from + 8)
        .find(|n| lines.get(n - 1).is_some_and(|l| l.contains(';')))
        .map_or(first, |n| u32::try_from(n).unwrap_or(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_found_on_the_line_or_the_one_before_and_a_url_is_not_one() {
        let src = [
            "// why",
            "let _ = f();",
            "let _ = g(); // why",
            "let u = \"http://x\";",
            "let _ = h();",
        ];
        assert!(comment_beside(&src, 2, 2));
        assert!(comment_beside(&src, 3, 3));
        assert!(!comment_beside(&src, 4, 4));
        assert!(!comment_beside(&src, 5, 5));
    }

    #[test]
    fn the_comment_text_starts_at_the_marker_and_ignores_strings() {
        assert_eq!(comment_text("x(); // why"), Some("// why"));
        assert_eq!(comment_text("let u = \"a//b\"; /* c */"), Some("/* c */"));
        assert_eq!(comment_text("let u = \"http://x\";"), None);
    }

    #[test]
    fn a_statement_ends_at_its_semicolon() {
        let src = ["let _ = f(", "    a,", ");", "x"];
        assert_eq!(statement_end(&src, 1), 3);
    }
}
