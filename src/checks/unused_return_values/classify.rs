//! Turns rustc's diagnostics plus the rewriter's tag table into per-function usage counts.
//!
//! Two signals, joined by the `CR:<id>` reason string:
//! - `unused_must_use` -> a call whose value was discarded ("ignored"). The tag is a child note.
//! - `deprecated`      -> any use at all ("total"). The tag is the text after the last `: CR:`.

use super::rewriter::{FnTag, UseRange};
use crate::cargo_json::Diagnostic;
use std::collections::{BTreeMap, HashSet};

/// The reason string of the per-run sentinel, which is not a real function.
pub const SENTINEL_TAG: &str = "sentinel";

#[derive(Debug, PartialEq, Eq)]
enum Tag {
    Fn(u32),
    Sentinel,
}

fn parse_tag(text: &str) -> Option<Tag> {
    let rest = text.strip_prefix("CR:")?;
    if rest == SENTINEL_TAG {
        Some(Tag::Sentinel)
    } else {
        rest.parse().ok().map(Tag::Fn)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Usage<'a> {
    pub tag: &'a FnTag,
    /// Call sites whose returned value is discarded.
    pub ignored: u32,
    /// Every use (calls, method calls, function-pointer uses), `let _ =` included.
    pub total: u32,
}

impl Usage<'_> {
    pub fn all_ignored(&self) -> bool {
        self.ignored == self.total
    }
}

#[derive(Debug)]
pub struct Classification<'a> {
    /// True only if BOTH signals were seen for the injected sentinel in this same build.
    pub sentinel_ok: bool,
    /// Functions with at least one ignored call, ordered by (file, line).
    pub usages: Vec<Usage<'a>>,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Kind {
    Ignored,
    Used,
}

pub fn classify<'a>(
    diagnostics: &[Diagnostic],
    tags: &'a [FnTag],
    use_ranges: &[UseRange],
) -> Classification<'a> {
    let by_id: BTreeMap<u32, &FnTag> = tags.iter().map(|t| (t.id, t)).collect();
    let mut seen: HashSet<(Kind, u32, String, u32, u32)> = HashSet::new();
    let mut ignored: BTreeMap<u32, u32> = BTreeMap::new();
    let mut total: BTreeMap<u32, u32> = BTreeMap::new();
    let (mut sentinel_ignored, mut sentinel_used) = (false, false);

    for d in diagnostics {
        let (kind, tag) = match d.code.as_deref() {
            Some("unused_must_use") => (Kind::Ignored, d.notes.iter().find_map(|n| parse_tag(n))),
            Some("deprecated") => (
                Kind::Used,
                d.message
                    .rsplit_once(": ")
                    .and_then(|(_, tail)| parse_tag(tail)),
            ),
            _ => continue,
        };
        let (Some(tag), Some(span)) = (tag, d.primary_span()) else {
            continue;
        };

        let id = match tag {
            Tag::Sentinel => {
                match kind {
                    Kind::Ignored => sentinel_ignored = true,
                    Kind::Used => sentinel_used = true,
                }
                continue;
            }
            Tag::Fn(id) => id,
        };
        let Some(def) = by_id.get(&id) else { continue };

        // `--all-targets` compiles shared code more than once; one site is one site.
        if !seen.insert((kind, id, span.file.clone(), span.line, span.column)) {
            continue;
        }
        // rustc never reports `deprecated` for a function's use of ITSELF, but does report
        // `unused_must_use` for it. Drop both so a recursive call is not an ignored caller.
        if span.file == def.file && (def.line..=def.last_line).contains(&span.line) {
            continue;
        }
        // An import is not a call site, but rustc reports `deprecated` at every imported name.
        if kind == Kind::Used
            && use_ranges
                .iter()
                .any(|u| u.file == span.file && (u.first_line..=u.last_line).contains(&span.line))
        {
            continue;
        }

        match kind {
            Kind::Ignored => *ignored.entry(id).or_default() += 1,
            Kind::Used => *total.entry(id).or_default() += 1,
        }
    }

    let mut usages: Vec<Usage> = ignored
        .into_iter()
        .map(|(id, ignored)| Usage {
            tag: by_id[&id],
            ignored,
            // A macro can expand one `deprecated` hit into several ignored sites; never report
            // more discarded calls than uses.
            total: total.get(&id).copied().unwrap_or(0).max(ignored),
        })
        .collect();
    usages.sort_by(|a, b| (&a.tag.file, a.tag.line).cmp(&(&b.tag.file, b.tag.line)));

    Classification {
        sentinel_ok: sentinel_ignored && sentinel_used,
        usages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo_json::Span;

    fn tag(id: u32, line: u32, last_line: u32) -> FnTag {
        FnTag {
            id,
            file: "src/lib.rs".into(),
            name: format!("f{id}"),
            line,
            last_line,
        }
    }

    fn ignored(id: &str, line: u32, col: u32) -> Diagnostic {
        Diagnostic {
            code: Some("unused_must_use".into()),
            level: "warning".into(),
            message: "unused return value of `f` that must be used".into(),
            notes: vec![format!("CR:{id}"), "use `let _ = ...`".into()],
            spans: vec![Span {
                file: "src/lib.rs".into(),
                line,
                column: col,
                is_primary: true,
            }],
        }
    }

    fn used(id: &str, line: u32, col: u32) -> Diagnostic {
        Diagnostic {
            code: Some("deprecated".into()),
            level: "warning".into(),
            message: format!("use of deprecated function `f`: CR:{id}"),
            notes: vec![],
            spans: vec![Span {
                file: "src/lib.rs".into(),
                line,
                column: col,
                is_primary: true,
            }],
        }
    }

    #[test]
    fn all_ignored_versus_mixed_comes_from_comparing_ignored_to_total() {
        let tags = [tag(0, 1, 1), tag(1, 2, 2)];
        let diags = [
            // f0: used twice, both discarded
            used("0", 10, 5),
            ignored("0", 10, 5),
            used("0", 11, 5),
            ignored("0", 11, 5),
            // f1: used twice, one discarded
            used("1", 12, 5),
            ignored("1", 12, 5),
            used("1", 13, 13),
        ];
        let c = classify(&diags, &tags, &[]);
        let got: Vec<_> = c
            .usages
            .iter()
            .map(|u| (u.tag.id, u.ignored, u.total, u.all_ignored()))
            .collect();
        assert_eq!(got, vec![(0, 2, 2, true), (1, 1, 2, false)]);
    }

    #[test]
    fn a_function_never_discarded_is_not_reported_at_all() {
        let tags = [tag(0, 1, 1)];
        let c = classify(&[used("0", 10, 5)], &tags, &[]);
        assert!(c.usages.is_empty());
    }

    #[test]
    fn duplicate_reports_from_multiple_targets_count_once() {
        let tags = [tag(0, 1, 1)];
        let diags = [
            used("0", 10, 5),
            ignored("0", 10, 5),
            used("0", 10, 5),
            ignored("0", 10, 5),
        ];
        let c = classify(&diags, &tags, &[]);
        assert_eq!((c.usages[0].ignored, c.usages[0].total), (1, 1));
    }

    #[test]
    fn a_recursive_call_inside_the_functions_own_body_is_neither_ignored_nor_used() {
        // Review Focus: rustc skips `deprecated` for self-use but not `unused_must_use`, so a
        // function that only discards its own recursive result would otherwise look "ignored 1 of 0".
        let tags = [tag(0, 3, 9)];
        let diags = [ignored("0", 6, 9), used("0", 20, 5)];
        let c = classify(&diags, &tags, &[]);
        assert!(
            c.usages.is_empty(),
            "the only real use (line 20) is not discarded"
        );
    }

    #[test]
    fn an_import_is_not_a_call_site() {
        // Review Focus: `use a::f;` makes rustc report `deprecated` at the imported name, which
        // would turn "ignored 1 of 1" into "ignored 1 of 2" -- every cross-module function "mixed".
        let tags = [tag(0, 1, 1)];
        let imports = [UseRange {
            file: "src/lib.rs".into(),
            first_line: 7,
            last_line: 9,
        }];
        let diags = [used("0", 8, 5), used("0", 12, 5), ignored("0", 12, 5)];
        let c = classify(&diags, &tags, &imports);
        assert_eq!((c.usages[0].ignored, c.usages[0].total), (1, 1));
        assert!(c.usages[0].all_ignored());
    }

    #[test]
    fn the_sentinel_needs_both_signals() {
        let tags: [FnTag; 0] = [];
        assert!(!classify(&[ignored("sentinel", 1, 1)], &tags, &[]).sentinel_ok);
        assert!(!classify(&[used("sentinel", 1, 1)], &tags, &[]).sentinel_ok);
        let both = [ignored("sentinel", 1, 1), used("sentinel", 1, 1)];
        assert!(classify(&both, &tags, &[]).sentinel_ok);
    }

    #[test]
    fn a_tag_that_is_not_ours_or_not_in_the_table_is_ignored() {
        let tags = [tag(0, 1, 1)];
        let mut other = ignored("0", 10, 5);
        other.notes = vec!["some other must_use reason".into()];
        let diags = [other, ignored("99", 11, 5), used("99", 11, 5)];
        assert!(classify(&diags, &tags, &[]).usages.is_empty());
    }
}
