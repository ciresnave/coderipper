// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-002: an environment variable that the code reads is documented.
//!
//! **What this judges (Rust and the tracked documentation, from the files alone):** every tracked `.rs` file of a package (outside
//! vendored directories, tests, examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`, and each
//! **named read of the process environment** is collected: a call of `env::var("NAME")` or `env::var_os("NAME")` (`std::env::var`
//! included) with a string literal, and a clap attribute `#[arg(env = "NAME")]` (or `#[clap(..)]`). A name is reported when it appears as
//! a whole word in **no tracked documentation file**: a `.md`, `.markdown`, `.txt`, `.rst` or `.adoc` file or a file named `.env*`,
//! `*.example` or `*.sample`, outside vendored, test and fixture directories, and not a `CHANGELOG` (a variable named only in a
//! changelog entry is not documented). A name must look like an environment key (upper-case letters, digits and `_`, three characters or
//! more). The names the platform or the toolchain provide are not read as configuration: `HOME`, `PATH`, `USER`, `USERNAME`,
//! `USERPROFILE`, `SHELL`, `PWD`, `TMP`, `TEMP`, `TMPDIR`, `TERM`, `LANG`, `EDITOR`, `APPDATA`, `LOCALAPPDATA`, `PATHEXT`, `COMSPEC`, `TZ`,
//! the proxy variables (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`, `ALL_PROXY`), `CI`, `NO_COLOR` and the like, and any name that starts with `CARGO_`, `RUST_`, `XDG_`, `LC_`, `GITHUB_` or `RUNNER_`. One **Low**-severity,
//! Medium-confidence finding per name, at its first read (path order), subject the name.
//!
//! **What it cannot see:** a name built at run time (`env::var(&name)`), a read through a wrapper or a configuration crate
//! (`config`, `figment`, `envy`), a clap builder's `.env("NAME")`, the other half of the record (that a key has a default and is used),
//! and the other languages. Documentation outside the repository is not read. A file `syn` cannot parse is named in a note, never read
//! as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::OPS_002;

const PROVIDED: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "USERNAME",
    "USERPROFILE",
    "LOGNAME",
    "SHELL",
    "PWD",
    "OLDPWD",
    "TMP",
    "TEMP",
    "TMPDIR",
    "TERM",
    "COLORTERM",
    "LANG",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "DISPLAY",
    "HOSTNAME",
    "COMPUTERNAME",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "SYSTEMROOT",
    "WINDIR",
    "CI",
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "FORCE_COLOR",
    "COLUMNS",
    "LINES",
    "SSH_AUTH_SOCK",
    "PATHEXT",
    "COMSPEC",
    "SYSTEMDRIVE",
    "USERDOMAIN",
    "PROGRAMFILES",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "PROCESSOR_IDENTIFIER",
    "PROCESSOR_LEVEL",
    "PROCESSOR_REVISION",
    "NUMBER_OF_PROCESSORS",
    "TZ",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
];
const PROVIDED_PREFIXES: &[&str] = &["CARGO_", "RUST_", "XDG_", "LC_", "GITHUB_", "RUNNER_"];

fn looks_like_key(name: &str) -> bool {
    name.len() >= 3
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn provided(name: &str) -> bool {
    PROVIDED.contains(&name) || PROVIDED_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn skipped_dir(path: &str) -> bool {
    path.split('/').any(|s| {
        matches!(
            s.to_lowercase().as_str(),
            "vendor"
                | "node_modules"
                | "target"
                | "third_party"
                | "tests"
                | "test"
                | "__tests__"
                | "fixtures"
                | "fixture"
                | "testdata"
                | "conformance"
        )
    })
}

fn is_doc(path: &str) -> bool {
    if skipped_dir(path) {
        return false;
    }
    let leaf = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    if leaf.starts_with("changelog") {
        return false;
    }
    leaf.starts_with(".env")
        || leaf.ends_with(".example")
        || leaf.ends_with(".sample")
        || [".md", ".markdown", ".txt", ".rst", ".adoc"]
            .iter()
            .any(|e| leaf.ends_with(e))
}

fn mentions(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(i, _)| {
        !text[..i].chars().next_back().is_some_and(word)
            && !text[i + name.len()..].chars().next().is_some_and(word)
    })
}

fn string_arg(call: &syn::ExprCall) -> Option<String> {
    match call.args.first()? {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

#[derive(Default)]
struct Walk {
    reads: Vec<(String, u32)>,
}

impl<'ast> Visit<'ast> for Walk {
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
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        syn::visit::visit_impl_item_fn(self, i);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let n = segs.len();
            if n >= 2 && segs[n - 2] == "env" && (segs[n - 1] == "var" || segs[n - 1] == "var_os") {
                if let Some(name) = string_arg(c) {
                    self.reads
                        .push((name, u32::try_from(c.func.span().start().line).unwrap_or(0)));
                }
            }
        }
        syn::visit::visit_expr_call(self, c);
    }

    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if a.path().is_ident("arg") || a.path().is_ident("clap") {
            let line = u32::try_from(a.span().start().line).unwrap_or(0);
            let mut found = Vec::new();
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("env") {
                    if let Ok(v) = m.value() {
                        if let Ok(s) = v.parse::<syn::LitStr>() {
                            found.push(s.value());
                        }
                    }
                } else if m.input.peek(syn::Token![=]) {
                    // another `key = value`: consume it so the next key is read
                    let _ = m.value().and_then(|v| v.parse::<syn::Expr>());
                }
                Ok(())
            });
            for name in found {
                self.reads.push((name, line));
            }
        }
        syn::visit::visit_attribute(self, a);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let docs: Vec<String> = view
        .paths()
        .iter()
        .filter(|p| is_doc(p))
        .filter_map(|p| view.read(p))
        .collect();
    let mut first: BTreeMap<String, (String, u32)> = BTreeMap::new();
    let mut files = 0usize;
    for p in &loaded.packages {
        for s in &p.sources {
            files += 1;
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for (name, line) in walk.reads {
                if looks_like_key(&name) && !provided(&name) {
                    first.entry(name).or_insert_with(|| (s.path.clone(), line));
                }
            }
        }
    }
    let total = first.len();
    let mut findings = Vec::new();
    for (name, (path, line)) in &first {
        if docs.iter().any(|d| mentions(d, name)) {
            continue;
        }
        findings.push(
            Finding::new(
                ID,
                Severity::Low,
                Confidence::Medium,
                view.name(),
                format!("the environment variable `{name}` is read by the code and no tracked document mentions it"),
                "A configuration key nobody wrote down has to be found by reading the source. Say in the README or a docs page what the variable does, its default and its accepted values, or stop reading it.",
            )
            .location(Location::new(path.as_str(), Some(*line)))
            .subject(name.clone())
            .positive_control(format!(
                "{files} Rust source file(s) were parsed for named environment reads ({total} configuration name(s) found) and {} documentation file(s) were searched for each as a whole word",
                docs.len()
            )),
        );
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

    fn found(src: &str, docs: &[(&str, &str)]) -> Vec<Finding> {
        let mut files = vec![("Cargo.toml", MANIFEST), ("src/lib.rs", src)];
        files.extend_from_slice(docs);
        match judge(&RepoView::from_files(&files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn names(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|x| x.subject.as_deref().unwrap()).collect()
    }

    #[test]
    fn an_undocumented_named_read_is_reported_and_a_documented_one_is_not() {
        let src = "pub fn f() {
    let _ = std::env::var(\"APP_TOKEN\");
    let _ = env::var_os(\"APP_PORT\");
}
#[derive(clap::Parser)]
pub struct Args { #[arg(long, env = \"APP_MODE\")] mode: String }
";
        let f = found(
            src,
            &[("README.md", "Set `APP_PORT` to choose the port.\n")],
        );
        assert_eq!(f.len(), 2, "{:?}", names(&f));
        assert!(names(&f).contains(&"APP_TOKEN") && names(&f).contains(&"APP_MODE"));
        assert!(!names(&f).contains(&"APP_PORT"));
    }

    #[test]
    fn whole_word_matching_and_the_excluded_files_are_honoured() {
        let src = "pub fn f() { let _ = std::env::var(\"APP_KEY\"); }\n";
        // a longer name that contains it does not document it, and a changelog does not either
        let f = found(
            src,
            &[
                ("docs/a.md", "APP_KEY_ID is another variable\n"),
                ("CHANGELOG.md", "added APP_KEY\n"),
            ],
        );
        assert_eq!(names(&f), ["APP_KEY"]);
        // `.env.example` and any markdown file document it
        assert!(found(src, &[(".env.example", "APP_KEY=\n")]).is_empty());
        assert!(found(src, &[("docs/config.md", "- `APP_KEY`: the key\n")]).is_empty());
    }

    #[test]
    fn platform_and_toolchain_names_dynamic_names_and_test_code_are_not_read() {
        let src = "pub fn f(n: &str) {
    let _ = std::env::var(\"HOME\");
    let _ = std::env::var(\"PATHEXT\");
    let _ = std::env::var(\"PROCESSOR_IDENTIFIER\");
    let _ = std::env::var(\"CARGO_MANIFEST_DIR\");
    let _ = std::env::var(\"lower_case\");
    let _ = std::env::var(n);
}
#[cfg(test)]
mod t { fn g() { let _ = std::env::var(\"ONLY_IN_TESTS\"); } }
";
        assert!(found(src, &[]).is_empty(), "{:?}", names(&found(src, &[])));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f() { let _ = std::env::var(\"APP_X\"); }\n", &[]) {
            f.validate().unwrap();
        }
    }
}
