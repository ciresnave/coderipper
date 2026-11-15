// SPDX-License-Identifier: MIT OR Apache-2.0
//! PRF-006: a performance exception names the benchmark that justified it.
//!
//! **The convention (PM ruling, 2026-11-15; a repository adopts it by writing the marker):** a deviation from normal structure made for
//! speed carries a comment that **begins** with `PERF-EXCEPTION:` and gives a benchmark reference on that line or in the comment text of
//! the next two lines:
//!
//! ```text
//! // PERF-EXCEPTION: get_unchecked, hot loop, see benches/decode.rs
//! // PERF-EXCEPTION: hand-unrolled, https://example.org/ci/bench/1234
//! ```
//!
//! **What this judges:** every tracked source file that contains the marker text (by extension, the comment families of `RDB-008`:
//! `//` and `/* */`, `#`, `--`; outside the directories `conflict-marker` skips) is read line by line. A comment whose first word is
//! `PERF-EXCEPTION:` is a **marker**. A marker is **linked** when its own comment text or the comment text of the next two lines holds an
//! `http://` or `https://` link, a path with a `bench`, `benches`, `benchmark` or `benchmarks` directory (`benches/decode.rs`), or a
//! `bench::` name. A marker with none is one finding per file at the first such marker, with the count: **Info** severity, **Low**
//! confidence. A repository that has no marker is silent ("not applicable"): the rule judges that a stated exception has evidence, never
//! that code is slow, and never that a site which is slow lacks the marker.
//!
//! **What it cannot see:** a performance deviation that carries no marker; whether the link resolves or the benchmark shows what the
//! comment says (the quality of the linkage needs a human); a reference on the line above the marker; a marker inside a string. Opt-in:
//! it runs when named (`coderipper check PRF-006`).

use super::rdb008::{comment_of, family, skipped, MAX_BYTES};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::PRF_006;

/// The marker text a comment must begin with.
pub(super) const MARKER: &str = "PERF-EXCEPTION:";

/// The body of a comment, without the characters that decorate it (`*`, `/`, `!`) or its leading space.
fn body(comment: &str) -> &str {
    comment.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '/' | '!'))
}

/// Whether comment text holds a benchmark reference.
fn has_benchmark(text: &str) -> bool {
    if text.contains("http://") || text.contains("https://") || text.contains("bench::") {
        return true;
    }
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '`' | '"' | '\''))
        .any(|word| {
            word.split('/')
                .rev()
                .skip(1)
                .any(|dir| matches!(dir, "bench" | "benches" | "benchmark" | "benchmarks"))
        })
}

/// The 1-based lines of a file's markers that name no benchmark, and how many markers there are.
fn unlinked(path: &str, text: &str) -> (Vec<u32>, usize) {
    let Some(family) = family(path) else {
        return (Vec::new(), 0);
    };
    let mut comments: Vec<Option<&str>> = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let (comment, open) = comment_of(line, family, in_block);
        in_block = open;
        comments.push(comment);
    }
    let mut bad = Vec::new();
    let mut markers = 0;
    for (i, comment) in comments.iter().enumerate() {
        let Some(c) = comment else { continue };
        let Some(rest) = body(c).strip_prefix(MARKER) else {
            continue;
        };
        markers += 1;
        // The reference may be on the marker's line or in the next two lines' comments, but not in the next marker's own comment.
        let linked = has_benchmark(rest)
            || comments
                .iter()
                .skip(i + 1)
                .take(2)
                .flatten()
                .take_while(|next| !body(next).starts_with(MARKER))
                .any(|next| has_benchmark(next));
        if !linked {
            bad.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
        }
    }
    (bad, markers)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let candidates = view.paths_matching("PERF-EXCEPTION");
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
    let (mut read, mut markers) = (0usize, 0usize);
    for path in paths {
        let Some(text) = view.read(path) else {
            continue;
        };
        if text.len() > MAX_BYTES || text.contains('\0') || !text.contains(MARKER) {
            continue;
        }
        read += 1;
        let (bad, found) = unlinked(path, &text);
        markers += found;
        if let Some(first) = bad.first().copied() {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Info,
                    Confidence::Low,
                    view.name(),
                    format!(
                        "{path} has {} `{MARKER}` marker(s) with no benchmark reference, the first at line {first}",
                        bad.len()
                    ),
                    "Name the benchmark that justified the exception on the marker's line or the next two: a link, a `benches/` path or a `bench::` name. If there is no benchmark, the exception is a guess: measure it or remove it.",
                )
                .location(Location::new(path.as_str(), Some(first)))
                .subject(path.as_str())
                .positive_control(format!(
                    "{read} file(s) carrying the marker were read, {markers} marker(s) found; a marker with a benchmark reference is not reported"
                )),
            );
        }
    }
    if markers == 0 {
        return Verdict::NotApplicable(format!(
            "no source comment begins with `{MARKER}`: the convention is adopted by writing the marker"
        ));
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Verdict {
        judge(&RepoView::from_files(files))
    }

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match run(files) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_marker_with_a_benchmark_on_its_line_or_the_next_two_is_clean() {
        let src = "// PERF-EXCEPTION: get_unchecked, see benches/decode.rs\nfn a() {}\n\n// PERF-EXCEPTION: unrolled\n// measured in\n// https://example.org/bench/1\nfn b() {}\n\n// PERF-EXCEPTION: inlined, bench::decode_fast\nfn c() {}\n";
        assert!(found(&[("src/lib.rs", src)]).is_empty());
    }

    #[test]
    fn a_marker_without_a_reference_is_reported_once_per_file_with_the_count() {
        let src = "// PERF-EXCEPTION: faster this way\nfn a() {}\n// PERF-EXCEPTION: also faster\nfn b() {}\n// PERF-EXCEPTION: ok, see benches/x.rs\nfn c() {}\n";
        let f = found(&[("src/lib.rs", src)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(1));
        assert!(
            f[0].summary.contains("2 `PERF-EXCEPTION:` marker"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn the_reference_must_be_within_two_lines_and_in_a_comment() {
        let far = "// PERF-EXCEPTION: x\nfn a() {}\nfn b() {}\n// see benches/x.rs\n";
        assert_eq!(found(&[("src/lib.rs", far)]).len(), 1);
        let code = "// PERF-EXCEPTION: x\nlet p = \"benches/x.rs\";\n";
        assert_eq!(found(&[("src/lib.rs", code)]).len(), 1);
    }

    #[test]
    fn only_a_comment_that_begins_with_the_marker_counts() {
        let src = "// we do not write PERF-EXCEPTION: here\nlet s = \"PERF-EXCEPTION: no\";\n";
        assert!(matches!(
            run(&[("src/lib.rs", src)]),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn block_hash_and_dash_comments_are_read() {
        assert_eq!(
            found(&[("a.rs", "/* PERF-EXCEPTION: x */\n")]).len(),
            1,
            "block"
        );
        assert_eq!(found(&[("a.py", "# PERF-EXCEPTION: x\n")]).len(), 1, "hash");
        assert!(found(&[("a.sql", "-- PERF-EXCEPTION: x, benchmarks/q.sql\n")]).is_empty());
        assert_eq!(
            found(&[("a.rs", "/**\n * PERF-EXCEPTION: x\n */\n")]).len(),
            1
        );
    }

    #[test]
    fn a_repository_with_no_marker_is_not_applicable() {
        assert!(matches!(
            run(&[("src/lib.rs", "fn a() {}\n")]),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn every_finding_validates() {
        for f in found(&[("src/lib.rs", "// PERF-EXCEPTION: x\n")]) {
            f.validate().unwrap();
        }
    }
}
