// SPDX-License-Identifier: MIT OR Apache-2.0
//! lint-suppression: a Rust lint suppression or an ignored test says why.
//!
//! **What this judges:** every tracked `.rs` file outside a vendored directory is parsed, and each `#[allow(..)]` / `#![allow(..)]`
//! (also inside `#[cfg_attr(.., allow(..))]`) and each `#[ignore]` is read. One without a reason is a finding: `allow(lints,
//! reason = "..")` and `#[ignore = ".."]` pass, with a non-blank string. `dead_code` is named in its finding, because the cure
//! there is almost never a suppression: delete the code, use it, or gate it with `#[cfg(test)]` / `#[cfg(feature = ..)]`. Every
//! attribute that does carry a reason is counted in a note on the run, so the debt stays visible when no finding is reported.
//!
//! **The ratchet (GOV-001, `docs/portfolio-ci-coverage.md` build order 5):** a branch adds no suppression. The `allow` and the `ignore`
//! attributes of the repository at `HEAD` are counted, reasons or not, and compared with the same count of the tree the branch
//! started from (the merge-base, as `version-and-changelog` finds it). A kind whose total rose is a **Medium** finding at the first file
//! whose own count rose; totals, not files, are compared, so a moved or renamed file adds nothing. No base (a shallow clone, the default
//! branch itself) or a base file `syn` cannot parse is a note that the ratchet did not run, never a clean pass. Owner and detector per
//! suppression are not judged: an attribute has no field for them, and a `reason` that names them is the author's to write.
//!
//! **What it cannot see:** attributes written inside a macro's token stream (`macro_rules!` bodies, a function-like macro's
//! arguments), which `syn` does not parse as attributes; a comment beside the attribute (it is not a reason: `reason =` is);
//! `#[expect(..)]`, which rustc itself fails when it has nothing left to suppress; suppressions in `Cargo.toml`
//! `[lints]` tables or in `clippy.toml`; any language but Rust. A file `syn` cannot parse is counted in a note, never read as clean.

use super::{Base, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Attribute, Meta, Token};

const ID: &str = crate::rule_id::LINT_SUPPRESSION;

/// Directory names whose contents are somebody else's code.
const VENDORED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bower_components",
];

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Allow(Vec<String>),
    Ignore,
}

#[derive(Debug)]
struct Site {
    kind: Kind,
    line: u32,
    has_reason: bool,
}

impl Site {
    /// What the attribute is called in a finding and as the allowlist symbol: `allow(dead_code)`, `ignore`.
    fn subject(&self) -> String {
        match &self.kind {
            Kind::Allow(lints) => format!("allow({})", lints.join(", ")),
            Kind::Ignore => "ignore".to_string(),
        }
    }
}

struct Sites {
    out: Vec<Site>,
}

fn non_blank(lit: &syn::Expr) -> bool {
    matches!(
        lit,
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) if !s.value().trim().is_empty()
    )
}

/// The site a single meta item describes, when it is a suppression.
fn site_of(meta: &Meta, line: u32) -> Option<Site> {
    let name = meta.path().get_ident()?.to_string();
    match (name.as_str(), meta) {
        ("allow", Meta::List(list)) => {
            let mut lints = Vec::new();
            let mut has_reason = false;
            let parsed = list.parse_nested_meta(|m| {
                if m.path.is_ident("reason") {
                    let value: syn::Expr = m.value()?.parse()?;
                    has_reason = non_blank(&value);
                } else {
                    lints.push(
                        m.path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::"),
                    );
                }
                Ok(())
            });
            parsed.ok()?;
            Some(Site {
                kind: Kind::Allow(lints),
                line,
                has_reason,
            })
        }
        ("ignore", Meta::Path(_)) => Some(Site {
            kind: Kind::Ignore,
            line,
            has_reason: false,
        }),
        ("ignore", Meta::NameValue(nv)) => Some(Site {
            kind: Kind::Ignore,
            line,
            has_reason: non_blank(&nv.value),
        }),
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Sites {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        let line = u32::try_from(attr.pound_token.spans[0].start().line).unwrap_or(0);
        if attr.path().is_ident("cfg_attr") {
            // `cfg_attr(condition, attr, attr, ..)`: the first item is the condition
            if let Ok(items) = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            {
                self.out
                    .extend(items.iter().skip(1).filter_map(|m| site_of(m, line)));
            }
        } else if let Some(site) = site_of(&attr.meta, line) {
            self.out.push(site);
        }
    }
}

/// The `allow` and `ignore` attributes of one file, reason or not, as `(is an allow, line)`; `None` when `syn` cannot parse it.
fn tally(text: &str) -> Option<Vec<(bool, u32)>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut sites = Sites { out: Vec::new() };
    sites.visit_file(&parsed);
    Some(
        sites
            .out
            .iter()
            .map(|s| (matches!(s.kind, Kind::Allow(_)), s.line))
            .collect(),
    )
}

/// Why a suppression-ratchet run produced nothing: a run on the default branch has no branch to compare (nothing to judge, not a gap),
/// anything else is a gap (no base found, a file that could not be read).
enum Skip {
    NotApplicable(String),
    Gap(String),
}

impl Skip {
    /// The note for the run's output. A gap says "not run" (the portfolio workflow fails a job on it); a run on the default branch says
    /// "not applicable", which is not a rule that failed to run.
    fn note(&self) -> String {
        match self {
            Skip::NotApplicable(why) => format!("{ID}: ratchet not applicable: {why}"),
            Skip::Gap(why) => format!("{ID}: ratchet not run: {why}"),
        }
    }
}

/// A suppression-ratchet run: the findings, or why it produced none.
fn ratchet(view: &RepoView, files: &[&String]) -> Result<Vec<Finding>, Skip> {
    let base = match view.base() {
        Base::Tree(base) => base,
        Base::SameAsHead(why) => return Err(Skip::NotApplicable(why)),
        Base::Gap(why) => return Err(Skip::Gap(why)),
    };
    // a file that cannot be counted (at HEAD or at the base) makes the comparison unsound: say so instead of reading it as zero
    let counted = |text: &str| {
        if text.contains("allow") || text.contains("ignore") {
            tally(text)
        } else {
            Some(Vec::new())
        }
    };
    let kinds = |sites: &[(bool, u32)], allow: bool| sites.iter().filter(|s| s.0 == allow).count();
    let mut then = [0usize; 2];
    for path in base
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p))
    {
        let Some(text) = base.read(path) else {
            return Err(Skip::Gap(format!(
                "{path} could not be read at {}",
                base.how()
            )));
        };
        let Some(sites) = counted(&text) else {
            return Err(Skip::Gap(format!(
                "{path} could not be parsed at {}",
                base.how()
            )));
        };
        then[0] += kinds(&sites, true);
        then[1] += kinds(&sites, false);
    }
    let mut now = [0usize; 2];
    // for each kind, every file at HEAD with more of it than the base has of that file, and the line of its first surplus one
    let mut rose: [Vec<(&str, u32)>; 2] = [Vec::new(), Vec::new()];
    for path in files {
        let Some(text) = view.read(path) else {
            return Err(Skip::Gap(format!("{path} could not be read")));
        };
        let Some(sites) = counted(&text) else {
            return Err(Skip::Gap(format!("{path} could not be parsed")));
        };
        let before = base
            .read(path)
            .and_then(|t| counted(&t))
            .unwrap_or_default();
        for (slot, allow) in [(0usize, true), (1, false)] {
            let have = kinds(&sites, allow);
            now[slot] += have;
            let had = kinds(&before, allow);
            if have > had {
                let line = sites.iter().filter(|s| s.0 == allow).nth(had).map(|s| s.1);
                rose[slot].push((path.as_str(), line.unwrap_or(1)));
            }
        }
    }
    let mut findings = Vec::new();
    for (slot, kind) in [(0usize, "allow"), (1, "ignore")] {
        let (before, after) = (then[slot], now[slot]);
        if after <= before {
            continue;
        }
        // one finding per file that gained one, so a file that is not reported (a planted fixture) cannot hide another that is
        for (file, line) in rose[slot].iter() {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::High,
                    view.name(),
                    format!(
                        "the number of `{kind}` attributes rose from {before} to {after} against {}",
                        base.how()
                    ),
                    "This branch adds a lint suppression or an ignored test. Fix what the lint objects to, or run the test where it can run                      (CLAUDE.md: a test that cannot run on GitHub runs on a self-hosted runner, never `#[ignore]`); the count only goes down.",
                )
                .location(Location::new(*file, Some(*line)))
                .subject(format!("{kind}-count"))
                .positive_control(format!(
                    "{} Rust file(s) at HEAD and those of {} counted with syn; the totals are {before} before and {after} now",
                    files.len(),
                    base.how()
                )),
            );
        }
    }
    Ok(findings)
}

fn vendored(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| VENDORED.contains(s))
}

fn describe(site: &Site) -> (String, String) {
    match &site.kind {
        Kind::Allow(lints) if lints.iter().any(|l| l == "dead_code" || l == "unused") => (
            format!("{} has no reason", site.subject()),
            "Dead code is deleted, used, or gated with `#[cfg(test)]` / `#[cfg(feature = ..)]`; a suppression is the last resort. \
             If it must stay, say why: `allow(dead_code, reason = \"..\")`."
                .to_string(),
        ),
        Kind::Allow(_) => (
            format!("{} has no reason", site.subject()),
            "Fix what the lint objects to; if the lint is wrong here, say why in the attribute: `allow(lint, reason = \"..\")`."
                .to_string(),
        ),
        Kind::Ignore => (
            "#[ignore] has no reason".to_string(),
            "An ignored test does not run in the default suite. Run it where it can (a scheduled or self-hosted job), or say where and why: `#[ignore = \"..\"]`."
                .to_string(),
        ),
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust source file".into());
    }
    let mut findings = Vec::new();
    let (mut allowed, mut ignored) = (0usize, 0usize);
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        let Ok(parsed) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        let mut sites = Sites { out: Vec::new() };
        sites.visit_file(&parsed);
        for site in sites.out {
            if site.has_reason {
                match site.kind {
                    Kind::Allow(_) => allowed += 1,
                    Kind::Ignore => ignored += 1,
                }
                continue;
            }
            let (title, why) = describe(&site);
            findings.push(
                Finding::new(ID, Severity::Low, Confidence::High, view.name(), title, why)
                    .location(Location::new(path.as_str(), Some(site.line)))
                    .subject(site.subject())
                    .positive_control(format!(
                        "{} Rust file(s) parsed with syn; the attribute was read at {path}:{}",
                        files.len() - unparsed.len(),
                        site.line
                    )),
            );
        }
    }
    let mut notes = vec![format!(
        "{ID}: {allowed} allow and {ignored} ignore attribute(s) carry a reason and pass"
    )];
    match ratchet(view, &files) {
        Ok(rose) => findings.extend(rose),
        Err(skip) => notes.push(skip.note()),
    }
    if !unparsed.is_empty() {
        notes.push(format!(
            "{ID}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        ));
    }
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> (Vec<Finding>, Vec<String>) {
        match judge(&RepoView::from_files(&[("src/lib.rs", src)])) {
            Verdict::Noted(f, n) => (f, n),
            _ => panic!("not a noted verdict"),
        }
    }

    fn subjects(src: &str) -> Vec<String> {
        run(src)
            .0
            .iter()
            .filter_map(|f| f.subject.clone())
            .collect()
    }

    #[test]
    fn a_bare_allow_is_a_finding_and_names_the_lint() {
        assert_eq!(
            subjects("#[allow(dead_code)]\nfn f() {}\n"),
            ["allow(dead_code)"]
        );
        assert_eq!(
            subjects("#![allow(clippy::too_many_arguments, unused)]\n"),
            ["allow(clippy::too_many_arguments, unused)"]
        );
    }

    #[test]
    fn dead_code_is_told_to_delete_use_or_gate() {
        let (found, _) = run("#[allow(dead_code)]\nfn f() {}\n");
        assert!(found[0].detail.contains("#[cfg(test)]"), "{:?}", found[0]);
    }

    #[test]
    fn an_allow_with_a_reason_passes_and_is_counted() {
        let (found, notes) =
            run("#[allow(dead_code, reason = \"kept for the linker\")]\nfn f() {}\n");
        assert!(found.is_empty());
        assert!(notes[0].contains("1 allow and 0 ignore"), "{notes:?}");
    }

    #[test]
    fn a_blank_reason_or_a_comment_is_not_a_reason() {
        assert_eq!(
            subjects("#[allow(unused, reason = \"  \")]\nfn f() {}\n").len(),
            1
        );
        assert_eq!(
            subjects("#[allow(unused)] // because\nfn f() {}\n").len(),
            1
        );
    }

    #[test]
    fn cfg_attr_wrapped_suppressions_are_read() {
        assert_eq!(
            subjects("#[cfg_attr(not(test), allow(dead_code))]\nfn f() {}\n"),
            ["allow(dead_code)"]
        );
        assert!(
            run("#[cfg_attr(test, allow(dead_code, reason = \"x\"))]\nfn f() {}\n")
                .0
                .is_empty()
        );
    }

    #[test]
    fn ignore_needs_a_reason() {
        assert_eq!(subjects("#[test]\n#[ignore]\nfn t() {}\n"), ["ignore"]);
        let (found, notes) =
            run("#[test]\n#[ignore = \"live network; scheduled job\"]\nfn t() {}\n");
        assert!(found.is_empty());
        assert!(notes[0].contains("0 allow and 1 ignore"), "{notes:?}");
    }

    #[test]
    fn text_in_strings_and_comments_is_not_an_attribute() {
        assert!(
            run("// #[allow(dead_code)]\nconst S: &str = \"#[allow(dead_code)]\";\n")
                .0
                .is_empty()
        );
    }

    #[test]
    fn expect_is_not_judged() {
        assert!(run("#[expect(unused)]\nfn f() {}\n").0.is_empty());
    }

    #[test]
    fn a_file_that_does_not_parse_is_a_note_never_clean() {
        let (found, notes) = run("fn (((\n");
        assert!(found.is_empty());
        assert!(
            notes.iter().any(|n| n.contains("could not be parsed")),
            "{notes:?}"
        );
    }

    #[test]
    fn vendored_code_is_skipped_and_no_rust_is_not_applicable() {
        let view = RepoView::from_files(&[("vendor/x/lib.rs", "#[allow(dead_code)]\nfn f() {}\n")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
        let view = RepoView::from_files(&[("README.md", "x")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
    }

    #[test]
    fn the_line_is_the_attributes_own() {
        let (found, _) = run("fn a() {}\n\n#[allow(unused)]\nfn b() {}\n");
        assert_eq!(found[0].location.as_ref().and_then(|l| l.line), Some(3));
    }

    fn ratchet(now: &[(&str, &str)], then: Option<&[(&str, &str)]>) -> (Vec<Finding>, Vec<String>) {
        match judge(&RepoView::from_files_with_base(now, then)) {
            Verdict::Noted(f, n) => (f, n),
            _ => panic!("not a noted verdict"),
        }
    }

    const ONE_ALLOW: &str = "#[allow(dead_code, reason = \"kept\")]\nfn f() {}\n";
    const TWO_ALLOWS: &str = "#[allow(dead_code, reason = \"kept\")]\nfn f() {}\n#[allow(unused, reason = \"kept\")]\nfn g() {}\n";

    #[test]
    fn a_branch_that_adds_a_suppression_with_a_reason_is_a_finding() {
        let (found, _) = ratchet(
            &[("src/lib.rs", TWO_ALLOWS)],
            Some(&[("src/lib.rs", ONE_ALLOW)]),
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::Medium);
        assert!(found[0].summary.contains("allow"), "{:?}", found[0]);
        assert!(found[0].summary.contains("from 1 to 2"), "{:?}", found[0]);
        assert_eq!(
            found[0].location.as_ref().map(|l| l.file.as_str()),
            Some("src/lib.rs")
        );
    }

    #[test]
    fn an_ignore_is_counted_apart_from_an_allow() {
        let (found, _) = ratchet(
            &[("src/lib.rs", "#[test]\n#[ignore = \"live\"]\nfn t() {}\n")],
            Some(&[("src/lib.rs", ONE_ALLOW)]),
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].summary.contains("ignore"), "{:?}", found[0]);
    }

    #[test]
    fn the_same_count_or_fewer_is_clean_and_a_moved_suppression_is_not_new() {
        // equal
        assert!(ratchet(
            &[("src/lib.rs", ONE_ALLOW)],
            Some(&[("src/lib.rs", ONE_ALLOW)])
        )
        .0
        .is_empty());
        // fewer
        assert!(ratchet(
            &[("src/lib.rs", ONE_ALLOW)],
            Some(&[("src/lib.rs", TWO_ALLOWS)])
        )
        .0
        .is_empty());
        // the file was renamed: the total is what is compared
        assert!(ratchet(
            &[("src/new.rs", ONE_ALLOW)],
            Some(&[("src/old.rs", ONE_ALLOW)])
        )
        .0
        .is_empty());
    }

    #[test]
    fn a_new_file_with_a_suppression_counts_against_a_base_without_it() {
        let (found, _) = ratchet(
            &[("src/lib.rs", ONE_ALLOW), ("src/new.rs", ONE_ALLOW)],
            Some(&[("src/lib.rs", ONE_ALLOW)]),
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].location.as_ref().map(|l| l.file.as_str()),
            Some("src/new.rs")
        );
    }

    #[test]
    fn every_file_that_gained_one_is_reported_so_one_cannot_hide_another() {
        let (found, _) = ratchet(
            &[("a/x.rs", ONE_ALLOW), ("src/y.rs", ONE_ALLOW)],
            Some(&[(
                "a/x.rs",
                "fn f() {}
",
            )]),
        );
        let files: Vec<_> = found
            .iter()
            .filter_map(|f| f.location.as_ref().map(|l| l.file.as_str()))
            .collect();
        assert_eq!(files, ["a/x.rs", "src/y.rs"], "{found:?}");
    }

    #[test]
    fn a_default_branch_run_is_not_applicable_and_a_missing_base_is_not_run() {
        let na = Skip::NotApplicable("HEAD is the merge-base (a run on the default branch)".into())
            .note();
        assert!(
            na.contains("ratchet not applicable") && !na.contains("not run"),
            "{na}"
        );
        let gap = Skip::Gap("the repository has no origin/main".into()).note();
        assert!(
            gap.contains("ratchet not run") && !gap.contains("not applicable"),
            "{gap}"
        );
    }

    #[test]
    fn without_a_base_the_ratchet_is_a_stated_gap_not_a_clean_pass() {
        let (found, notes) = ratchet(&[("src/lib.rs", TWO_ALLOWS)], None);
        assert!(found.is_empty());
        assert!(
            notes.iter().any(|n| n.contains("ratchet not run")),
            "{notes:?}"
        );
    }

    #[test]
    fn a_base_file_that_does_not_parse_is_a_gap_not_zero() {
        let (found, notes) = ratchet(
            &[("src/lib.rs", ONE_ALLOW)],
            Some(&[("src/lib.rs", "#[allow(((\n")]),
        );
        assert!(found.is_empty());
        assert!(
            notes.iter().any(|n| n.contains("ratchet not run")),
            "{notes:?}"
        );
    }
}
