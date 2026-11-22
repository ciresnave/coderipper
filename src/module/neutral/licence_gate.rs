// SPDX-License-Identifier: MIT OR Apache-2.0
//! The opt-in per-file licence gate of `SUP-016` (`[sup016] gate = true` in the repository's own `.coderipper.toml`): the contract of the
//! portfolio's `spdx_gate.py`, so a repository can retire the script.
//!
//! **What it judges, High and High because a repository that opts in wants a red run:**
//! - every tracked file of a stamped extension declares `SPDX-License-Identifier:` in its first `header_window` lines, and the
//!   identifier equals the configured licence (the order of `OR` operands does not matter);
//! - the population is not empty: fewer stamped files than `min_files` means the query is broken, not the tree, and nothing else is judged;
//! - the extension census: a tracked *source* extension that is neither stamped nor declined by name in `not_stamped` is a blind spot, and a
//!   declined extension the tree no longer has is stale;
//! - a `holdout` path (a file whose licence is someone else's decision) that matches no tracked stamped file is stale;
//! - the copyright survey: a tracked file containing the word "copyright" that is not a licence file, a changelog, a notice or a holdout.
//!
//! **What it cannot see:** the licence text; whether a holdout's reason is true. It never writes. Differences from the script, kept on
//! purpose: a file git takes for binary is skipped by the copyright survey (`git grep -I`), and a header terminator is read for `*/`, `-->`
//! and `*)`, as the script does.

use super::RepoView;
use crate::config::Sup016Settings;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeSet;

const ID: &str = crate::rule_id::LICENCE;
const MARKER: &str = "SPDX-License-Identifier:";
const DEFAULT_MIN_FILES: usize = 20;
const DEFAULT_WINDOW: usize = 10;

/// Extensions that are source in the sense of the census (the script's own list).
const SOURCE_EXTENSIONS: &[&str] = &[
    ".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".go", ".java", ".c", ".h", ".cc", ".cpp", ".hpp", ".cs", ".swift", ".kt",
    ".scala", ".php", ".rb", ".pl", ".sh", ".bash", ".ps1", ".sql", ".lua", ".zig", ".dart", ".cu", ".cuh", ".cl", ".comp", ".vert",
    ".frag", ".geom", ".glsl", ".wgsl", ".hlsl", ".metal", ".slang",
];

/// File-name fragments whose files may contain the word "copyright" (a licence text, a changelog, a notice).
const COPYRIGHT_EXPECTED: &[&str] = &["LICENSE", "LICENCE", "COPYING", "CHANGELOG", "NOTICE", ".coderipper.toml"];

/// Canonical form for comparison only: `Apache-2.0 OR MIT` and `MIT OR Apache-2.0` are the same grant.
fn normalise(identifier: &str) -> String {
    let text = identifier.trim();
    for joiner in [" OR ", " or "] {
        if text.contains(joiner) {
            let mut parts: Vec<&str> = text.split(joiner).map(str::trim).collect();
            parts.sort_unstable();
            return parts.join(" OR ");
        }
    }
    text.to_string()
}

/// The identifier a file declares in its first `window` lines, or `None`.
fn declared(text: &str, window: usize) -> Option<String> {
    for line in text.lines().take(window) {
        if let Some((_, rest)) = line.split_once(MARKER) {
            let mut value = rest.trim();
            for terminator in ["*/", "-->", "*)"] {
                if let Some(v) = value.strip_suffix(terminator) {
                    value = v.trim();
                }
            }
            return if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
    }
    None
}

/// The extension of a tracked path, lower-cased, with its dot; empty for a dotfile or a name with no extension.
fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => name[i..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn detail_list(paths: &[String]) -> String {
    let shown: Vec<&str> = paths.iter().take(8).map(String::as_str).collect();
    let more = paths.len().saturating_sub(shown.len());
    if more == 0 {
        shown.join(", ")
    } else {
        format!("{}, and {more} more", shown.join(", "))
    }
}

fn finding(project: &str, summary: String, detail: String, subject: &str, file: Option<&str>) -> Finding {
    let mut f = Finding::new(ID, Severity::High, Confidence::High, project, summary, detail).subject(subject);
    if let Some(file) = file {
        f = f.location(Location::new(file, None));
    }
    f
}

/// The gate's findings for one repository. Empty means every check passed.
pub(super) fn gate(view: &RepoView, settings: &Sup016Settings, project: &str) -> Vec<Finding> {
    let Some(licence) = settings.licence.as_deref().filter(|l| !l.trim().is_empty()) else {
        return vec![finding(
            project,
            "[sup016] gate = true but `licence` is unset".into(),
            "Set `licence = \"<SPDX expression>\"` and `extensions = [...]` under [sup016]; a gate that cannot say what it requires must not pass.".into(),
            "gate configuration",
            Some(".coderipper.toml"),
        )];
    };
    let extensions: BTreeSet<String> = settings.extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    if extensions.is_empty() {
        return vec![finding(
            project,
            "[sup016] gate = true but `extensions` is empty".into(),
            "List the extensions that must carry the header, with their dot, such as `extensions = [\".rs\", \".py\"]`.".into(),
            "gate configuration",
            Some(".coderipper.toml"),
        )];
    }
    let window = settings.header_window.unwrap_or(DEFAULT_WINDOW);
    let floor = settings.min_files.unwrap_or(DEFAULT_MIN_FILES);
    let expected = normalise(licence);

    let tracked = view.paths();
    let stamped: Vec<&String> = tracked.iter().filter(|p| extensions.contains(&extension(p))).collect();
    if stamped.len() < floor {
        return vec![finding(
            project,
            format!("found {} source files of the stamped extensions, expected at least {floor}", stamped.len()),
            "The gate reports full compliance on an empty set, so it fails here instead: the extension list or the tree is broken, not the \
             licence headers. Nothing else was judged."
                .into(),
            "population below the floor",
            None,
        )];
    }

    let mut findings = Vec::new();
    let (mut missing, mut wrong, mut unreadable) = (Vec::new(), Vec::new(), Vec::new());
    let mut held = 0usize;
    for path in &stamped {
        if settings.holdout.contains_key(path.as_str()) {
            held += 1;
            continue;
        }
        match view.read(path) {
            None => unreadable.push((*path).clone()),
            Some(text) => match declared(&text, window) {
                None => missing.push((*path).clone()),
                Some(found) if normalise(&found) != expected => wrong.push(format!("{path} ({found})")),
                Some(_) => {}
            },
        }
    }
    if !missing.is_empty() {
        let first = missing[0].clone();
        findings.push(
            Finding::absence(
                ID,
                Severity::High,
                Confidence::High,
                project,
                format!("{} of {} stamped source files declare no licence in their first {window} lines", missing.len(), stamped.len()),
                format!(
                    "Add `SPDX-License-Identifier: {licence}` as the first line, above any inner docs (a shebang stays on line 1). Files: {}",
                    detail_list(&missing)
                ),
                format!("{} of the stamped files do declare one, so the search finds a header where it exists", stamped.len() - missing.len() - wrong.len() - unreadable.len() - held),
            )
            .location(Location::new(first, Some(1)))
            .subject("gate: missing header"),
        );
    }
    if !wrong.is_empty() {
        findings.push(finding(
            project,
            format!("{} stamped source files declare a licence other than {licence}", wrong.len()),
            format!(
                "A different declaration is not a formatting error: changing it asserts a grant its author may not have made. Either it is right \
                 and the file belongs under [sup016.holdout] with a reason, or it is wrong and that is the owner's decision. Files: {}",
                detail_list(&wrong)
            ),
            "gate: different licence",
            None,
        ));
    }
    if !unreadable.is_empty() {
        findings.push(finding(
            project,
            format!("{} stamped source files could not be read as text", unreadable.len()),
            format!("A file that cannot be read is not a clean file. Files: {}", detail_list(&unreadable)),
            "gate: unreadable",
            None,
        ));
    }
    let stamped_set: BTreeSet<&str> = stamped.iter().map(|p| p.as_str()).collect();
    let stale: Vec<String> = settings
        .holdout
        .keys()
        .filter(|k| !stamped_set.contains(k.as_str()))
        .cloned()
        .collect();
    if !stale.is_empty() {
        findings.push(finding(
            project,
            format!("{} [sup016.holdout] entries name a path that is not a tracked stamped file", stale.len()),
            format!("A holdout that matches nothing protects nothing, and reads like one with nothing to protect until its file is renamed and then stamped. Entries: {}", detail_list(&stale)),
            "gate: stale holdout",
            None,
        ));
    }

    // the census: the population is itself checked
    let present: BTreeSet<String> = tracked.iter().map(|p| extension(p)).filter(|e| !e.is_empty()).collect();
    let declined: BTreeSet<String> = settings.not_stamped.keys().map(|e| e.to_ascii_lowercase()).collect();
    let uncovered: Vec<String> = SOURCE_EXTENSIONS
        .iter()
        .map(|e| (*e).to_string())
        .filter(|e| present.contains(e) && !extensions.contains(e) && !declined.contains(e))
        .collect();
    if !uncovered.is_empty() {
        findings.push(finding(
            project,
            format!("tracked source extensions outside `extensions` and `not_stamped`: {}", uncovered.join(", ")),
            "A source extension outside the stamped list is invisible to this gate, and the pass rate rises as the blind spot grows. Add it to \
             `extensions`, or to [sup016.not_stamped] with the reason it is excluded."
                .into(),
            "gate: uncovered extension",
            None,
        ));
    }
    let stale_ext: Vec<String> = declined.iter().filter(|e| !present.contains(*e)).cloned().collect();
    if !stale_ext.is_empty() {
        findings.push(finding(
            project,
            format!("[sup016.not_stamped] declines extensions that are gone from the tree: {}", stale_ext.join(", ")),
            "Remove the entry: a decision on the record that no longer applies should not stay on it.".into(),
            "gate: stale not_stamped",
            None,
        ));
    }

    // the copyright survey
    let mut tags: Vec<String> = COPYRIGHT_EXPECTED.iter().map(|t| (*t).to_string()).collect();
    tags.extend(settings.copyright_expected.iter().cloned());
    let hits: Vec<String> = match view.paths_matching("[Cc][Oo][Pp][Yy][Rr][Ii][Gg][Hh][Tt]") {
        Some(list) => list,
        None => tracked
            .iter()
            .filter(|p| view.read(p).is_some_and(|t| t.to_ascii_lowercase().contains("copyright")))
            .cloned()
            .collect(),
    };
    let unexpected: Vec<String> = hits
        .into_iter()
        .filter(|p| {
            let base = p.rsplit('/').next().unwrap_or(p);
            !tags.iter().any(|t| base.contains(t.as_str())) && !settings.holdout.contains_key(p.as_str())
        })
        .collect();
    if !unexpected.is_empty() {
        findings.push(finding(
            project,
            format!("{} tracked files carry a copyright notice and are not a licence file or a holdout", unexpected.len()),
            format!(
                "A file carrying somebody else's copyright notice may not be ours to license. Decide, then add it to [sup016.holdout] with the \
                 reason, or its name fragment to `copyright_expected` if the match is incidental. Files: {}",
                detail_list(&unexpected)
            ),
            "gate: copyright notice",
            Some(&unexpected[0]),
        ));
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Sup016Settings {
        Sup016Settings {
            gate: true,
            licence: Some("MIT OR Apache-2.0".into()),
            extensions: vec![".rs".into()],
            min_files: Some(2),
            ..Sup016Settings::default()
        }
    }

    const GOOD: &str = "// SPDX-License-Identifier: MIT OR Apache-2.0\nfn a() {}\n";

    fn run(files: &[(&str, &str)], s: &Sup016Settings) -> Vec<Finding> {
        gate(&RepoView::from_files(files), s, "p")
            .into_iter()
            .map(|f| f.validate().unwrap_or_else(|e| panic!("{e}")))
            .collect()
    }

    fn subjects(found: &[Finding]) -> Vec<&str> {
        found.iter().map(|f| f.subject.as_deref().unwrap_or("")).collect()
    }

    #[test]
    fn a_stamped_tree_passes_and_the_operand_order_does_not_matter() {
        let swapped = "// SPDX-License-Identifier: Apache-2.0 OR MIT\n";
        assert!(run(&[("a.rs", GOOD), ("b.rs", swapped)], &settings()).is_empty());
    }

    #[test]
    fn a_file_with_no_header_a_file_with_another_licence_and_a_header_past_the_window_are_each_reported() {
        let late = format!("{}// SPDX-License-Identifier: MIT OR Apache-2.0\n", "//\n".repeat(10));
        let found = run(
            &[("a.rs", GOOD), ("b.rs", "fn b() {}\n"), ("c.rs", "// SPDX-License-Identifier: GPL-3.0-only\n"), ("d.rs", &late)],
            &settings(),
        );
        assert_eq!(subjects(&found), ["gate: missing header", "gate: different licence"]);
        assert!(found[0].summary.starts_with("2 of 4"), "{}", found[0].summary);
        assert!(found[1].detail.contains("c.rs (GPL-3.0-only)"));
    }

    #[test]
    fn a_comment_terminator_is_not_part_of_the_identifier() {
        let css = "/* SPDX-License-Identifier: MIT OR Apache-2.0 */\n";
        assert!(run(&[("a.rs", css), ("b.rs", GOOD)], &settings()).is_empty());
    }

    #[test]
    fn an_extension_is_matched_without_regard_to_case_and_a_dot_in_a_directory_is_not_one() {
        assert_eq!(extension("UPPER.RS"), ".rs");
        assert_eq!(extension("some.dir/file"), "");
        assert_eq!(extension(".gitignore"), "");
        let found = run(&[("a.rs", GOOD), ("UPPER.RS", "fn x() {}\n")], &settings());
        assert_eq!(subjects(&found), ["gate: missing header"]);
    }

    #[test]
    fn a_tree_below_the_floor_fails_and_judges_nothing_else() {
        let found = run(&[("a.rs", "fn a() {}\n")], &settings());
        assert_eq!(subjects(&found), ["population below the floor"]);
    }

    #[test]
    fn a_holdout_is_skipped_and_a_holdout_naming_no_file_is_stale() {
        let other = "// SPDX-License-Identifier: Apache-2.0\n";
        let mut s = settings();
        s.holdout.insert("v.rs".into(), "verbatim third-party work".into());
        assert!(run(&[("a.rs", GOOD), ("b.rs", GOOD), ("v.rs", other)], &s).is_empty());
        s.holdout.insert("gone.rs".into(), "renamed".into());
        assert_eq!(subjects(&run(&[("a.rs", GOOD), ("b.rs", GOOD), ("v.rs", other)], &s)), ["gate: stale holdout"]);
    }

    #[test]
    fn a_source_extension_outside_the_list_is_a_blind_spot_until_it_is_declined_and_a_stale_decline_is_reported() {
        let files = [("a.rs", GOOD), ("b.rs", GOOD), ("x.py", "print(1)\n")];
        assert_eq!(subjects(&run(&files, &settings())), ["gate: uncovered extension"]);
        let mut s = settings();
        s.not_stamped.insert(".py".into(), "scripts are not licensed here".into());
        assert!(run(&files, &s).is_empty());
        s.not_stamped.insert(".go".into(), "was here".into());
        assert_eq!(subjects(&run(&files, &s)), ["gate: stale not_stamped"]);
    }

    #[test]
    fn a_copyright_notice_in_an_ordinary_file_is_reported_and_a_licence_file_or_a_holdout_is_not() {
        let files = [("a.rs", GOOD), ("b.rs", GOOD), ("LICENSE", "Copyright 2026 Acme\n"), ("notes.md", "(c) copyright someone\n")];
        let found = run(&files, &settings());
        assert_eq!(subjects(&found), ["gate: copyright notice"]);
        assert!(found[0].detail.contains("notes.md") && !found[0].detail.contains("LICENSE"));
        let mut s = settings();
        s.holdout.insert("notes.md".into(), "vendored".into());
        assert_eq!(subjects(&run(&files, &s)), ["gate: stale holdout"]);
        let mut s = settings();
        s.copyright_expected.push("notes".into());
        assert!(run(&files, &s).is_empty());
    }

    #[test]
    fn a_gate_with_no_licence_or_no_extensions_cannot_pass() {
        let mut s = settings();
        s.licence = None;
        assert_eq!(subjects(&run(&[("a.rs", GOOD)], &s)), ["gate configuration"]);
        let mut s = settings();
        s.extensions.clear();
        assert_eq!(subjects(&run(&[("a.rs", GOOD)], &s)), ["gate configuration"]);
    }
}
