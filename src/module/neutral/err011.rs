// SPDX-License-Identifier: MIT OR Apache-2.0
//! ERR-011: a parse that can fail is not silently turned into a default value.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. It reports a parse whose failure is replaced
//! on the spot by a value nobody chose: `text.parse::<T>().unwrap_or_default()`, `.unwrap_or(..)`, `.unwrap_or_else(..)`, the same
//! through `.ok()` first (`.parse().ok().unwrap_or(..)`), and `T::from_str(text)` in the same position. The failure signal is thrown
//! away, so a malformed input flows onward as if it were real data.
//!
//! A defaulting parse with a justification is not reported: a `//` comment on the line, or in the comment block directly above it, that
//! says why (`default`, `fall back`, `fallback`, `lenient`, `best effort`, `on purpose`, `deliberate`, `intentional`). One **Medium**
//! severity, Medium-confidence finding per site, at the defaulting call.
//!
//! **What it cannot see:** whether the receiver is a string at all (`syn` has no types: a zero-argument `.parse()` is read as
//! `str::parse`), a parse that is `.ok()`-ed and then tested later, a parse wrapped in a function of the project, and a default that is
//! the right answer (an optional setting). A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::ERR_011;

/// The methods that replace a failed parse with a value.
const DEFAULTING: &[&str] = &["unwrap_or_default", "unwrap_or", "unwrap_or_else"];

const JUSTIFICATIONS: &[&str] = &[
    "default",
    "fall back",
    "fallback",
    "falls back",
    "lenient",
    "best effort",
    "best-effort",
    "on purpose",
    "deliberate",
    "intentional",
];

struct Site {
    line: u32,
    function: String,
    parse: String,
    defaulting: String,
}

struct Walk<'a> {
    lines: Vec<&'a str>,
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

/// The parse this expression is, when it is `x.parse()` (any turbofish, no arguments) or `T::from_str(x)`, looking through one `.ok()`.
fn parse_source(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::MethodCall(m) if m.method == "ok" && m.args.is_empty() => {
            parse_source(&m.receiver)
        }
        syn::Expr::MethodCall(m) if m.method == "parse" && m.args.is_empty() => {
            Some("parse".into())
        }
        syn::Expr::Call(c) if c.args.len() == 1 => {
            let syn::Expr::Path(p) = &*c.func else {
                return None;
            };
            let segs = &p.path.segments;
            (segs.len() >= 2 && segs.last().is_some_and(|s| s.ident == "from_str"))
                .then(|| "from_str".into())
        }
        syn::Expr::Paren(p) => parse_source(&p.expr),
        _ => None,
    }
}

impl Walk<'_> {
    /// A justifying comment: on the line itself, or in the comment block directly above it (comment lines and blank lines up to the
    /// first line of code, three lines at most).
    fn justified(&self, line: u32) -> bool {
        let says = |l: &str| {
            l.find("//").is_some_and(|i| {
                let comment = l[i..].to_lowercase();
                JUSTIFICATIONS.iter().any(|j| comment.contains(j))
            })
        };
        let line = usize::try_from(line).unwrap_or(0);
        if self
            .lines
            .get(line.wrapping_sub(1))
            .is_some_and(|l| says(l))
        {
            return true;
        }
        for n in (line.saturating_sub(3).max(1)..line).rev() {
            let Some(l) = self.lines.get(n - 1) else {
                break;
            };
            let t = l.trim_start();
            if t.is_empty() {
                continue;
            }
            if !t.starts_with("//") {
                break;
            }
            if says(l) {
                return true;
            }
        }
        false
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let name = m.method.to_string();
        if DEFAULTING.contains(&name.as_str()) {
            if let Some(parse) = parse_source(&m.receiver) {
                let line = u32::try_from(m.method.span().start().line).unwrap_or(0);
                let start = u32::try_from(m.span().start().line).unwrap_or(0);
                if !self.justified(line) && !self.justified(start) {
                    self.sites.push(Site {
                        line,
                        function: self
                            .functions
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "<module>".into()),
                        parse,
                        defaulting: name,
                    });
                }
            }
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            let mut walk = Walk {
                lines: s.text.lines().collect(),
                functions: Vec::new(),
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!(
                            "`{}` in `{}` turns a failed `{}` into `{}`'s value without looking at the failure",
                            site.defaulting, site.function, site.parse, site.defaulting
                        ),
                        "A malformed input becomes a default that flows onward as if it were real data. Match on the Result, report or propagate the failure, or say in a comment on the line why falling back is right.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.parse))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads parse calls and the comments beside them"
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_parse_defaulted_on_failure_is_reported() {
        let src = "pub fn f(s: &str) -> u32 {
    let a: u32 = s.parse().unwrap_or_default();
    let b = s.parse::<u32>().unwrap_or(0);
    let c = s.parse::<u32>().ok().unwrap_or(1);
    let d = u32::from_str(s).unwrap_or_else(|_| 2);
    a + b + c + d
}
";
        let f = found(src);
        assert_eq!(f.len(), 4, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(f[3].subject.as_deref().unwrap().ends_with(":f:from_str"));
    }

    #[test]
    fn a_checked_parse_is_not_reported() {
        let src = "pub fn f(s: &str) -> Result<u32, std::num::ParseIntError> {
    let a = s.parse::<u32>()?;
    let b = match s.parse::<u32>() { Ok(v) => v, Err(_) => return Ok(0) };
    let c = s.parse::<u32>().map_err(|e| e)?;
    let d = other.unwrap_or(3);
    let e = parser.parse(s).unwrap_or_default();
    Ok(a + b + c + d)
}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_justifying_comment_above_or_on_the_line_clears_the_site() {
        let src = "pub fn f(s: &str) {
    // the port is optional: fall back to the default
    let a = s.parse::<u16>().unwrap_or(80);
    let b = s.parse::<u16>().unwrap_or(81); // lenient on purpose
    let c = s.parse::<u16>().unwrap_or(82);
}
";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(5));
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn f(s: &str) { s.parse::<u8>().unwrap_or(0); } }\n#[test]\nfn g() { \"1\".parse::<u8>().unwrap_or(0); }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(s: &str) -> u8 { s.parse().unwrap_or_default() }\n") {
            f.validate().unwrap();
        }
    }
}
