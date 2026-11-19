// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-022 .. SUP-026: the licence and attribution check, read from `LICENCE-MANIFEST.toml` at the repository root.
//!
//! One schema, shared with licence-tools: `docs/MANIFEST-SCHEMA.md` of that repository (rules 1-7), whose reference validator
//! `tools/licence_manifest.py` (branch `lane/manifest-schema` @27dc254, compatibility table `lane/compat-table` @0dfd055) this
//! file is a port of, so that the two tools agree about one file. Design and crosswalk: `docs/DESIGN-LICENCE-ATTRIBUTION.md`.
//!
//! | rule | validator id | finds |
//! |---|---|---|
//! | SUP-022 | LM-001 | the manifest does not parse, or breaks the schema (an unknown key is an error, rule 5) |
//! | SUP-023 | LM-002 | a required file, the notice that names `origin-copyright` on a copyright line, or the licence text is not tracked |
//! | SUP-024 | LM-003 | a tracked file says "Origin:", "derived from", "ported from" or "vendored" in its first 60 lines and no entry covers it (scope rule 7) |
//! | SUP-025 | LM-004 | an entry's `path` matches no tracked file |
//! | SUP-026 | LM-005 | an entry's `SPDX-License-Identifier` contains a licence its `origin-licence` does not allow it to be carried under |
//!
//! **Not applicable** when the repository tracks no `LICENCE-MANIFEST.toml` (the check judges declared provenance; a repository
//! that declares none has nothing to compare). Files are enumerated from the index and read as committed at `HEAD`, never from the
//! disk. **What it cannot see:** copied code that carries neither a marker nor an entry; the check proves that *declared* provenance
//! is consistent. Findings are advisory, Low severity; prose-file markers (LM-003) and unchecked licences (LM-005) are info notes
//! that never change the verdict.

use super::licence_compat as compat;
use super::{dir_and_ancestors, dir_of, file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

/// The manifest, at the repository root.
const MANIFEST: &str = "LICENCE-MANIFEST.toml";
const REQUIRED: &[&str] = &["path", "SPDX-License-Identifier", "SPDX-FileCopyrightText"];
const KEYS: &[&str] = &[
    "path",
    "SPDX-License-Identifier",
    "SPDX-FileCopyrightText",
    "precedence",
    "origin-url",
    "origin-ref",
    "origin-licence",
    "origin-copyright",
    "relation",
    "relation-comment",
    "requires",
];
/// Keys that must hold a string (the other two required keys take any TOML value, as REUSE lets them hold a list).
const STRING_KEYS: &[&str] = &[
    "path",
    "precedence",
    "origin-url",
    "origin-ref",
    "origin-licence",
    "origin-copyright",
    "relation",
    "relation-comment",
    "lines",
];
const PRECEDENCE: &[&str] = &["closest", "aggregate", "override"];
const RELATIONS: &[&str] = &[
    "COPY_OF",
    "DESCENDANT_OF",
    "FILE_MODIFIED",
    "VARIANT_OF",
    "OTHER",
];
const NOTICE_NAMES: &[&str] = &["NOTICE", "ATTRIBUTION", "COPYING", "LICENSE", "THIRD"];
const MARKERS: &[&str] = &["origin:", "derived from", "ported from", "vendored"];
/// The same four markers as an extended regular expression, in both cases, to ask git which files to read at all.
const MARKER_ERE: &str = "[Oo][Rr][Ii][Gg][Ii][Nn]:|[Dd][Ee][Rr][Ii][Vv][Ee][Dd] from|[Pp][Oo][Rr][Tt][Ee][Dd] from|[Vv][Ee][Nn][Dd][Oo][Rr][Ee][Dd]";
/// Scope rule 7(b): a marker in one of these is credit-only prose, info level.
const PROSE: &[&str] = &["md", "markdown", "rst", "txt", "tsv"];
/// How many lines of a file are read for an origin marker or a notice.
const HEAD_LINES: usize = 60;
/// How many info notes are printed before the rest are counted.
const NOTES_SHOWN: usize = 25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lm {
    Parse,
    Notice,
    Marker,
    Stale,
    Compat,
}

impl Lm {
    fn rule(self) -> &'static str {
        match self {
            Lm::Parse => crate::rule_id::SUP_022,
            Lm::Notice => crate::rule_id::SUP_023,
            Lm::Marker => crate::rule_id::SUP_024,
            Lm::Stale => crate::rule_id::SUP_025,
            Lm::Compat => crate::rule_id::SUP_026,
        }
    }

    fn lm(self) -> &'static str {
        match self {
            Lm::Parse => "LM-001",
            Lm::Notice => "LM-002",
            Lm::Marker => "LM-003",
            Lm::Stale => "LM-004",
            Lm::Compat => "LM-005",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Lm::Parse => "manifest",
            Lm::Notice => "notice",
            Lm::Marker => "origin-marker",
            Lm::Stale => "stale-entry",
            Lm::Compat => "compatibility",
        }
    }

    fn remediation(self) -> &'static str {
        match self {
            Lm::Parse => "Fix LICENCE-MANIFEST.toml to match the shared schema (licence-tools docs/MANIFEST-SCHEMA.md): required keys, closed key set, `lines = \"a:b\"`, a known `relation`, `origin-copyright` for any licence that asks for attribution.",
            Lm::Notice => "Track the notice: add the origin's own copyright line (holder and year, as the origin's LICENSE has it) to a NOTICE or LICENSE file beside the covered files, or list it in `requires`, and track the licence text as `LICENSES/<id>.txt`. A credit in prose is not a notice.",
            Lm::Marker => "Add a `[[annotations]]` entry covering this file (or a `[[snippet]]` for the lines that carry the origin), with the origin's URL, licence and copyright holder; or reword the marker if the file has no origin.",
            Lm::Stale => "Remove the entry, or correct its `path`, if the files it described moved or were deleted.",
            Lm::Compat => "Relicense the covered files under a licence the origin allows, or obtain the origin's permission; check the pair with the licence's own text (the table is advisory, not legal advice).",
        }
    }
}

/// One thing the analysis found: a finding, or (`info`) a note that never changes the verdict.
struct Hit {
    lm: Lm,
    file: String,
    line: Option<u32>,
    subject: String,
    text: String,
    info: bool,
}

/// What every check of one run shares.
struct Run<'a> {
    which: Lm,
    view: &'a RepoView,
    /// The manifest's lines, for the line of the nth `[[annotations]]` or `[[snippet]]` header.
    manifest: &'a str,
    hits: Vec<Hit>,
}

impl Run<'_> {
    fn push(&mut self, lm: Lm, line: Option<u32>, subject: &str, text: String, info: bool) {
        if lm == self.which {
            self.hits.push(Hit {
                lm,
                file: MANIFEST.into(),
                line,
                subject: format!("{subject}: {}", lm.kind()),
                text,
                info,
            });
        }
    }

    /// The 1-based line of the `i`th (0-based) `[[kind]]` header of the manifest.
    fn header_line(&self, kind: &str, i: usize) -> Option<u32> {
        let header = format!("[[{kind}]]");
        self.manifest
            .lines()
            .enumerate()
            .filter(|(_, l)| l.trim_start().starts_with(&header))
            .nth(i)
            .and_then(|(n, _)| u32::try_from(n + 1).ok())
    }
}

/// REUSE globs: `*` stays inside a directory, `**` crosses them; everything else is literal.
fn glob_match(pattern: &str, path: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = &p[2..];
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            Some(b'*') => {
                let rest = &p[1..];
                let mut i = 0;
                loop {
                    if go(rest, &s[i..]) {
                        return true;
                    }
                    if i == s.len() || s[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    go(pattern.as_bytes(), path.as_bytes())
}

/// Lower case, whitespace collapsed to single spaces.
fn norm(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A credit is not a notice: the holder must sit on a line that also carries `copyright`, `(c)` or the copyright sign.
fn notice_names_holder(corpus: &[String], holder: &str) -> bool {
    let h = norm(holder);
    corpus.iter().any(|l| {
        l.contains(&h) && (l.contains("copyright") || l.contains("(c)") || l.contains('\u{a9}'))
    })
}

fn is_licence_text(path: &str) -> bool {
    let upper = file_name(path).to_uppercase();
    path.starts_with("LICENSES/")
        || path.contains("/LICENSES/")
        || upper.starts_with("LICENSE")
        || upper.starts_with("COPYING")
}

fn first_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().take(HEAD_LINES)
}

/// Text a notice may live in: the entry's `requires`, NOTICE-like files in the covered files' directories or their ancestors
/// (never `LICENSES/`), and the covered files' own first 60 lines.
fn notice_corpus(view: &RepoView, requires: &[String], hits: &[String]) -> Vec<String> {
    let mut names: Vec<String> = requires.to_vec();
    let mut dirs: Vec<String> = Vec::new();
    for f in hits {
        for d in dir_and_ancestors(dir_of(f)) {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    }
    for f in view.paths() {
        let upper = file_name(f).to_uppercase();
        if !f.starts_with("LICENSES/")
            && dirs.iter().any(|d| d == dir_of(f))
            && NOTICE_NAMES.iter().any(|n| upper.starts_with(n))
            && !names.contains(f)
        {
            names.push(f.clone());
        }
    }
    let mut corpus: Vec<String> = Vec::new();
    for n in &names {
        if let Some(text) = view.read(n) {
            corpus.extend(text.lines().map(norm));
        }
    }
    for f in hits {
        if let Some(text) = view.read(f) {
            corpus.extend(first_lines(&text).map(norm));
        }
    }
    corpus
}

/// `a:b`, both 1-based decimal numbers.
fn parse_lines(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once(':')?;
    let ok =
        |x: &str| !x.is_empty() && !x.starts_with('0') && x.bytes().all(|c| c.is_ascii_digit());
    (ok(a) && ok(b)).then_some(())?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn str_of<'a>(e: &'a toml::Table, key: &str) -> Option<&'a str> {
    e.get(key).and_then(toml::Value::as_str)
}

/// One `[[annotations]]` or `[[snippet]]` entry: LM-001, LM-004, LM-002 and LM-005 for it. Returns the tracked files it covers.
fn check_entry(run: &mut Run, kind: &str, i: usize, e: &toml::Table) -> Vec<String> {
    let line = run.header_line(kind, i);
    let path = str_of(e, "path").unwrap_or("?");
    let tag = format!("[[{kind}]] #{} ({path})", i + 1);
    let snippet = kind == "snippet";
    let bad = |run: &mut Run, text: String| {
        run.push(Lm::Parse, line, path, format!("{tag}: {text}"), false)
    };

    for k in e.keys() {
        let known = KEYS.contains(&k.as_str()) && !(snippet && k == "precedence")
            || (snippet && k == "lines");
        if !known {
            bad(run, format!("unknown key `{k}`"));
        }
    }
    let mut required: Vec<&str> = REQUIRED.to_vec();
    if snippet {
        required.push("lines");
    }
    for k in &required {
        if !e.contains_key(*k) {
            bad(run, format!("`{k}` is required here and absent"));
        }
    }
    for k in STRING_KEYS {
        if e.get(*k).is_some_and(|v| !v.is_str()) {
            bad(run, format!("`{k}` must be a string"));
        }
    }
    let requires: Vec<String> = match e.get("requires") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) if a.iter().all(toml::Value::is_str) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(_) => {
            bad(run, "`requires` must be an array of strings".into());
            Vec::new()
        }
    };
    let lines = str_of(e, "lines");
    if snippet {
        if let Some(l) = lines {
            match parse_lines(l) {
                None => bad(run, "`lines` must be `a:b`, 1-based".into()),
                Some((a, b)) if a > b => bad(run, format!("`lines` {l} runs backwards")),
                Some(_) => {}
            }
        }
    }
    if e.keys().any(|k| k.starts_with("origin-")) {
        for k in ["origin-url", "origin-licence"] {
            if !e.contains_key(k) {
                bad(run, format!("an origin entry also needs `{k}`"));
            }
        }
    }
    if let Some(p) = str_of(e, "precedence") {
        if !PRECEDENCE.contains(&p) {
            bad(run, format!("`precedence` must be one of {PRECEDENCE:?}"));
        }
    }
    if let Some(r) = str_of(e, "relation") {
        if !RELATIONS.contains(&r) {
            bad(run, format!("`relation` must be one of {RELATIONS:?}"));
        }
    }
    if str_of(e, "relation") == Some("OTHER") && !e.contains_key("relation-comment") {
        bad(run, "relation OTHER needs `relation-comment`".into());
    }
    if e.contains_key("relation-comment") && !e.contains_key("relation") {
        bad(run, "`relation-comment` without `relation`".into());
    }

    let hits: Vec<String> = match str_of(e, "path") {
        Some(p) => run
            .view
            .paths()
            .iter()
            .filter(|f| glob_match(p, f))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    if str_of(e, "path").is_some() && hits.is_empty() {
        run.push(
            Lm::Stale,
            line,
            path,
            format!("{tag}: the path matches nothing that git tracks (a stale entry)"),
            false,
        );
    }
    let lic = str_of(e, "origin-licence").filter(|l| !l.is_empty());
    if let (Some(lic), Some(ours)) = (lic, str_of(e, "SPDX-License-Identifier")) {
        let (conflicts, unjudged) = compat::conflicts(lic, ours);
        for b in conflicts {
            run.push(
                Lm::Compat,
                line,
                path,
                format!("{tag}: origin-licence {lic} cannot be carried under `{b}`"),
                false,
            );
        }
        let mut unchecked: Vec<String> = unjudged;
        if !compat::known(lic) {
            unchecked.push(lic.to_string());
        }
        unchecked.sort();
        unchecked.dedup();
        if !unchecked.is_empty() {
            run.push(
                Lm::Compat,
                line,
                path,
                format!(
                    "{tag}: not in the compatibility table, unchecked: {}",
                    unchecked.join(", ")
                ),
                true,
            );
        }
    }
    // LM-002: one report per missing required file, else one for the holder's notice, else one for the licence text
    let mut notice: Vec<String> = requires
        .iter()
        .filter(|n| !run.view.has(n))
        .map(|n| format!("{tag}: required file `{n}` is not a tracked file"))
        .collect();
    if let Some(lic) = lic.filter(|l| compat::attribution_required(l)) {
        match str_of(e, "origin-copyright").filter(|h| !h.is_empty()) {
            None => bad(
                run,
                format!(
                    "origin-licence {lic} asks for attribution, the entry lacks `origin-copyright`"
                ),
            ),
            Some(holder) => {
                if run.which == Lm::Notice
                    && notice.is_empty()
                    && !hits.is_empty()
                    && !notice_names_holder(&notice_corpus(run.view, &requires, &hits), holder)
                {
                    notice.push(format!(
                        "{tag}: the tracked notices do not name `{holder}` on a copyright line (origin-licence {lic})"
                    ));
                }
            }
        }
    }
    if let Some(lic) = lic {
        let text = format!("LICENSES/{lic}.txt");
        if notice.is_empty() && !run.view.has(&text) && requires.is_empty() {
            notice.push(format!(
                "{tag}: the licence text for `{lic}` is not tracked (`{text}` absent, `requires` empty)"
            ));
        }
    }
    for n in notice {
        run.push(Lm::Notice, line, path, n, false);
    }
    hits
}

/// LM-003: the files whose first 60 lines carry an origin marker that no entry covers.
fn scan_markers(run: &mut Run, covered: &[String], spans: &[(String, u64, u64)]) {
    let view = run.view;
    let mut candidates: Vec<String> = match view.paths_matching(MARKER_ERE) {
        Some(found) => found,
        None => view.paths().to_vec(),
    };
    candidates.sort();
    candidates.dedup();
    for f in candidates {
        if f == MANIFEST || covered.contains(&f) || is_licence_text(&f) {
            continue;
        }
        let Some(text) = view.read(&f) else { continue };
        if text.contains('\0') {
            continue;
        }
        let uncovered = first_lines(&text).enumerate().find_map(|(n, l)| {
            let n = (n + 1) as u64;
            let lower = l.to_lowercase();
            let marked = MARKERS.iter().any(|m| lower.contains(m));
            let inside = spans.iter().any(|(p, a, b)| *p == f && *a <= n && n <= *b);
            (marked && !inside).then_some(n)
        });
        let Some(n) = uncovered else { continue };
        let prose = file_name(&f).rsplit_once('.').is_some_and(|(stem, ext)| {
            !stem.is_empty() && PROSE.contains(&ext.to_lowercase().as_str())
        });
        let (text, info) = if prose {
            (
                "credit-only prose, no notice (not a miss)".to_string(),
                true,
            )
        } else {
            (
                "origin marker in the first 60 lines, and no manifest entry covers the file"
                    .to_string(),
                false,
            )
        };
        if run.which == Lm::Marker {
            run.hits.push(Hit {
                lm: Lm::Marker,
                file: f.clone(),
                line: u32::try_from(n).ok(),
                subject: format!("{f}: {}", Lm::Marker.kind()),
                text,
                info,
            });
        }
    }
}

/// Everything the manifest says about `which`; `None` when the repository tracks no manifest.
fn analyse(view: &RepoView, which: Lm) -> Option<Vec<Hit>> {
    let manifest = view.read(MANIFEST)?;
    let mut run = Run {
        which,
        view,
        manifest: &manifest,
        hits: Vec::new(),
    };
    let data: toml::Table = match manifest.parse() {
        Ok(t) => t,
        Err(e) => {
            let line = e.span().map(|s| {
                u32::try_from(
                    manifest[..s.start.min(manifest.len())]
                        .matches('\n')
                        .count()
                        + 1,
                )
                .unwrap_or(1)
            });
            run.push(
                Lm::Parse,
                line,
                MANIFEST,
                format!("{MANIFEST} does not parse: {}", e.message()),
                false,
            );
            return Some(run.hits);
        }
    };
    if data.get("version").and_then(toml::Value::as_integer) != Some(1) {
        run.push(
            Lm::Parse,
            None,
            MANIFEST,
            "`version` must be 1".into(),
            false,
        );
    }
    let mut covered: Vec<String> = Vec::new();
    let mut spans: Vec<(String, u64, u64)> = Vec::new();
    for kind in ["annotations", "snippet"] {
        let Some(value) = data.get(kind) else {
            continue;
        };
        let Some(entries) = value.as_array() else {
            run.push(
                Lm::Parse,
                None,
                MANIFEST,
                format!("`{kind}` must be an array of tables"),
                false,
            );
            continue;
        };
        for (i, entry) in entries.iter().enumerate() {
            let Some(table) = entry.as_table() else {
                run.push(
                    Lm::Parse,
                    None,
                    MANIFEST,
                    format!("[[{kind}]] #{} must be a table", i + 1),
                    false,
                );
                continue;
            };
            let hits = check_entry(&mut run, kind, i, table);
            if kind == "annotations" {
                covered.extend(hits);
            } else if let (Some(p), Some((a, b))) = (
                str_of(table, "path"),
                str_of(table, "lines").and_then(parse_lines),
            ) {
                spans.push((p.to_string(), a, b));
            }
        }
    }
    if which == Lm::Marker {
        scan_markers(&mut run, &covered, &spans);
    }
    Some(run.hits)
}

fn judge(view: &RepoView, which: Lm) -> Verdict {
    let Some(hits) = analyse(view, which) else {
        return Verdict::NotApplicable(format!(
            "the repository tracks no {MANIFEST} (this rule compares declared provenance with the tracked files)"
        ));
    };
    let read = view.paths().len();
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    for h in hits {
        let tag = format!("{} ({})", h.lm.rule(), h.lm.lm());
        if h.info {
            notes.push(format!("info {tag} {}: {}", h.file, h.text));
            continue;
        }
        let confidence = if h.lm == Lm::Marker {
            Confidence::Medium
        } else {
            Confidence::High
        };
        findings.push(
            Finding::new(
                which.rule(),
                Severity::Low,
                confidence,
                view.name(),
                format!("{tag} {}", h.text),
                which.remediation(),
            )
            .location(Location::new(h.file, h.line))
            .subject(h.subject)
            .positive_control(format!(
                "{read} tracked file(s) were enumerated from the index for the same check; an entry or file that meets it is not reported"
            )),
        );
    }
    if notes.len() > NOTES_SHOWN {
        let more = notes.len() - NOTES_SHOWN;
        notes.truncate(NOTES_SHOWN);
        notes.push(format!(
            "{} ({}): {more} more info note(s) not shown",
            which.rule(),
            which.lm()
        ));
    }
    if notes.is_empty() {
        Verdict::Findings(findings)
    } else {
        Verdict::Noted(findings, notes)
    }
}

pub(super) fn judge_sup022(view: &RepoView) -> Verdict {
    judge(view, Lm::Parse)
}

pub(super) fn judge_sup023(view: &RepoView) -> Verdict {
    judge(view, Lm::Notice)
}

pub(super) fn judge_sup024(view: &RepoView) -> Verdict {
    judge(view, Lm::Marker)
}

pub(super) fn judge_sup025(view: &RepoView) -> Verdict {
    judge(view, Lm::Stale)
}

pub(super) fn judge_sup026(view: &RepoView) -> Verdict {
    judge(view, Lm::Compat)
}

#[cfg(test)]
mod tests;
