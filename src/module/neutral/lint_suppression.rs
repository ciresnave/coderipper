//! lint-suppression: a Rust lint suppression or an ignored test says why.
//!
//! **What this judges:** every tracked `.rs` file outside a vendored directory is parsed, and each `#[allow(..)]` / `#![allow(..)]`
//! (also inside `#[cfg_attr(.., allow(..))]`) and each `#[ignore]` is read. One without a reason is a finding: `allow(lints,
//! reason = "..")` and `#[ignore = ".."]` pass, with a non-blank string. `dead_code` is named in its finding, because the cure
//! there is almost never a suppression: delete the code, use it, or gate it with `#[cfg(test)]` / `#[cfg(feature = ..)]`. Every
//! attribute that does carry a reason is counted in a note on the run, so the debt stays visible when no finding is reported.
//!
//! **What it cannot see:** attributes written inside a macro's token stream (`macro_rules!` bodies, a function-like macro's
//! arguments), which `syn` does not parse as attributes; a comment beside the attribute (it is not a reason: `reason =` is);
//! `#[expect(..)]`, which rustc itself fails when it has nothing left to suppress; suppressions in `Cargo.toml`
//! `[lints]` tables or in `clippy.toml`; any language but Rust. A file `syn` cannot parse is counted in a note, never read as clean.

use super::{RepoView, Verdict};
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
}
