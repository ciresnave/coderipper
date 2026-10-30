// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-012: a command-line tool follows the platform's conventions (the static half of the OPS-011 follow-up, ROADMAP M15).
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file that is a binary's source (`main.rs`, or a file under
//! `src/bin/`), outside a vendored directory, `tests/`, `benches/`, `examples/` and a `#[cfg(test)]` module, is parsed with `syn`.
//! Four things are found:
//!
//! 1. **a fixed editor:** `Command::new("nano")` (or `vim`, `vi`, `nvim`, `emacs`, `pico`, `notepad`) in a file that never names
//!    `EDITOR` or `VISUAL`;
//! 2. **a dotfile in the home root:** `.join(".name")` in a file that reads the home directory (`home_dir`, `HOME`, `USERPROFILE`) and
//!    never names `XDG_`, `APPDATA`, `config_dir`, `ProjectDirs` or `BaseDirs`. The advice is per platform: `%APPDATA%` on Windows, the
//!    XDG directories on Linux (XDG is a Linux convention, not a Windows one). `.config`, `.local` and `.cache` are the XDG
//!    directories' own names and are not flagged;
//! 3. **a hand-rolled argument loop:** the file reads `env::args` or `env::args_os` and names two or more flags, and the package's
//!    `Cargo.toml` (the nearest one up the tree) names no argument parser (`clap`, `argh`, `pico-args`, `lexopt`, `bpaf`, `gumdrop`,
//!    `structopt`, `docopt`, `getopts`, `xflags`, `argp`, `seahorse`);
//! 4. **`-v` and `-V` swapped:** a `match` arm on `"-v"` whose body prints a version, or on `"-V"` whose body sets verbosity; and the
//!    clap forms `#[arg(short = 'v')]` on a field called `version` and `#[arg(short = 'V')]` on one called `verbose`.
//!
//! **What it cannot see (and the record says so):** another language; an editor or a parser named in a *different* file than the one
//! that uses it; a path built by `format!`; a parser it does not know; macros (`syn` does not look into their tokens, except that a
//! macro's tokens are searched for the words `version` and `verbose` in check 4). A file `syn` cannot parse is counted in a note,
//! never read as clean. The finding is a convention, not a failure: Low severity, Medium confidence.

use super::{dir_and_ancestors, dir_of, join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, Expr, ExprCall, ExprMatch, ExprMethodCall, Field, ItemMod, Lit, Pat};

const ID: &str = crate::rule_id::OPS_012;

/// Directory names whose contents are somebody else's code, or tests.
const SKIPPED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bower_components",
    "tests",
    "benches",
    "examples",
];

const EDITORS: &[&str] = &["nano", "vim", "vi", "nvim", "emacs", "pico", "notepad"];

const PARSERS: &[&str] = &[
    "clap",
    "argh",
    "pico-args",
    "pico_args",
    "lexopt",
    "bpaf",
    "gumdrop",
    "structopt",
    "docopt",
    "getopts",
    "xflags",
    "argp",
    "seahorse",
];

/// What a file that already keeps its configuration where the platform does says.
const PLATFORM_DIRS: &[&str] = &["XDG_", "APPDATA", "config_dir", "ProjectDirs", "BaseDirs"];

const HOME_WORDS: &[&str] = &["home_dir", "\"HOME\"", "USERPROFILE"];

/// The XDG directories' own names: `home.join(".config").join("tool")` is the convention spelled by hand.
const XDG_NAMES: &[&str] = &[".config", ".local", ".cache"];

fn skipped(path: &str) -> bool {
    path.split('/')
        .any(|s| SKIPPED.iter().any(|d| s.eq_ignore_ascii_case(d)))
}

fn is_binary_source(path: &str) -> bool {
    path.ends_with(".rs")
        && (path == "main.rs"
            || path.ends_with("/main.rs")
            || path.starts_with("src/bin/")
            || path.contains("/src/bin/"))
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Editor,
    Dotfile,
    HandRolled,
    SwappedFlag,
}

struct Hit {
    kind: Kind,
    line: u32,
    what: String,
}

/// The words (lower-cased identifiers, string literals and macro tokens) under an expression.
#[derive(Default)]
struct Words(String);

impl<'ast> Visit<'ast> for Words {
    fn visit_ident(&mut self, i: &'ast proc_macro2::Ident) {
        self.0.push_str(&i.to_string().to_lowercase());
        self.0.push(' ');
    }
    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        self.0.push_str(&s.value().to_lowercase());
        self.0.push(' ');
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.0.push_str(&m.tokens.to_string().to_lowercase());
        self.0.push(' ');
    }
}

fn words_of(expr: &Expr) -> String {
    let mut w = Words::default();
    w.visit_expr(expr);
    w.0
}

/// The string literals in a pattern (`"-v" | "--version"` is two).
fn pattern_strings(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Lit(l) => {
            if let Lit::Str(s) = &l.lit {
                out.push(s.value());
            }
        }
        Pat::Or(o) => o.cases.iter().for_each(|c| pattern_strings(c, out)),
        Pat::Paren(p) => pattern_strings(&p.pat, out),
        Pat::Reference(r) => pattern_strings(&r.pat, out),
        Pat::TupleStruct(t) => t.elems.iter().for_each(|c| pattern_strings(c, out)),
        Pat::Tuple(t) => t.elems.iter().for_each(|c| pattern_strings(c, out)),
        _ => {}
    }
}

fn is_flag(s: &str) -> bool {
    let rest = s.strip_prefix("--").or_else(|| s.strip_prefix('-'));
    rest.is_some_and(|r| {
        r.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && r.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta.require_list().is_ok_and(|l| {
                l.tokens
                    .to_string()
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|t| t == "test")
            })
    })
}

#[derive(Default)]
struct Scan {
    hits: Vec<Hit>,
    flags: usize,
    reads_args: Option<u32>,
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        if !is_cfg_test(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        if is_flag(&s.value()) {
            self.flags += 1;
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(p) = &*call.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let tail: Vec<&str> = segs.iter().map(String::as_str).collect();
            if tail.ends_with(&["Command", "new"]) {
                if let Some(Expr::Lit(l)) = call.args.first() {
                    if let Lit::Str(s) = &l.lit {
                        let value = s.value();
                        let name = value.rsplit(['/', '\\']).next().unwrap_or(&value);
                        let name = name.strip_suffix(".exe").unwrap_or(name);
                        if EDITORS.contains(&name) {
                            self.hits.push(Hit {
                                kind: Kind::Editor,
                                line: line_of(s.span()),
                                what: value,
                            });
                        }
                    }
                }
            }
            if tail.ends_with(&["env", "args"]) || tail.ends_with(&["env", "args_os"]) {
                self.reads_args.get_or_insert(line_of(
                    p.path
                        .segments
                        .last()
                        .map_or_else(proc_macro2::Span::call_site, |s| s.ident.span()),
                ));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if call.method == "join" && call.args.len() == 1 {
            if let Some(Expr::Lit(l)) = call.args.first() {
                if let Lit::Str(s) = &l.lit {
                    let v = s.value();
                    if v.len() > 1
                        && v.starts_with('.')
                        && !v.starts_with("..")
                        && !v.contains(['/', '\\'])
                        && !XDG_NAMES.contains(&v.as_str())
                    {
                        self.hits.push(Hit {
                            kind: Kind::Dotfile,
                            line: line_of(s.span()),
                            what: v,
                        });
                    }
                }
            }
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_match(&mut self, m: &'ast ExprMatch) {
        for arm in &m.arms {
            let mut strings = Vec::new();
            pattern_strings(&arm.pat, &mut strings);
            let body = words_of(&arm.body);
            let version = body.contains("version");
            let verbose = body.contains("verbose");
            let line = line_of(arm.pat.span());
            if strings.iter().any(|s| s == "-v") && version && !verbose {
                self.hits.push(Hit {
                    kind: Kind::SwappedFlag,
                    line,
                    what: "-v prints the version".into(),
                });
            }
            if strings.iter().any(|s| s == "-V") && verbose && !version {
                self.hits.push(Hit {
                    kind: Kind::SwappedFlag,
                    line,
                    what: "-V sets verbosity".into(),
                });
            }
        }
        syn::visit::visit_expr_match(self, m);
    }

    fn visit_field(&mut self, f: &'ast Field) {
        if let Some(name) = &f.ident {
            for a in &f.attrs {
                let t: String = a
                    .meta
                    .require_list()
                    .map(|l| l.tokens.to_string())
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if name == "version" && t.contains("short='v'") {
                    self.hits.push(Hit {
                        kind: Kind::SwappedFlag,
                        line: line_of(name.span()),
                        what: "-v is the short flag of `version`".into(),
                    });
                }
                if name == "verbose" && t.contains("short='V'") {
                    self.hits.push(Hit {
                        kind: Kind::SwappedFlag,
                        line: line_of(name.span()),
                        what: "-V is the short flag of `verbose`".into(),
                    });
                }
            }
        }
        syn::visit::visit_field(self, f);
    }
}

/// Whether the nearest `Cargo.toml` at or above `path` names an argument parser (`None`: no `Cargo.toml` found).
fn package_has_parser(view: &RepoView, path: &str) -> Option<bool> {
    for dir in dir_and_ancestors(dir_of(path)) {
        if let Some(text) = view.read(&join(&dir, "Cargo.toml")) {
            // a workspace root has no `[package]`: keep climbing from a member only when this one is not a package
            if text.contains("[package]") || dir.is_empty() {
                return Some(PARSERS.iter().any(|p| text.contains(p)));
            }
        }
    }
    None
}

fn mentions(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| is_binary_source(p) && !skipped(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust binary source (main.rs or src/bin/) outside tests, benches, examples and vendored trees".into());
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let mut parsed_count = 0usize;
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        let Ok(parsed) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        parsed_count += 1;
        let mut scan = Scan::default();
        scan.visit_file(&parsed);
        let editor_named = mentions(&text, &["EDITOR", "VISUAL"]);
        let reads_home = mentions(&text, HOME_WORDS);
        let platform_dirs = mentions(&text, PLATFORM_DIRS);
        let mut hits: Vec<(Kind, u32, String, String, String)> = Vec::new();
        for h in &scan.hits {
            match h.kind {
                Kind::Editor if !editor_named => hits.push((
                    h.kind,
                    h.line,
                    format!("{path} starts the fixed editor `{}`", h.what),
                    "The tool ignores the editor its user chose. Read $VISUAL, then $EDITOR, and fall back to a fixed program only when neither is set (the `edit` crate does this and knows the platform defaults).".into(),
                    h.what.clone(),
                )),
                Kind::Dotfile if reads_home && !platform_dirs => hits.push((
                    h.kind,
                    h.line,
                    format!("{path} keeps `{}` in the home directory root", h.what),
                    "A dotfile in the home root clutters it and ignores where the platform keeps configuration: %APPDATA% (or %LOCALAPPDATA% for caches) on Windows, $XDG_CONFIG_HOME (default ~/.config) on Linux; XDG is a Linux convention and does not apply on Windows. The `dirs` and `directories` crates give the right directory per platform.".into(),
                    h.what.clone(),
                )),
                Kind::SwappedFlag => hits.push((
                    h.kind,
                    h.line,
                    format!("{path}: {}", h.what),
                    "By convention -V prints the version and -v raises verbosity (-q lowers it); users guess these without reading the help. Swap them, or keep --version/--verbose only.".into(),
                    h.what.clone(),
                )),
                _ => {}
            }
        }
        if let Some(line) = scan.reads_args {
            if scan.flags >= 2 && package_has_parser(view, path) == Some(false) {
                hits.push((
                    Kind::HandRolled,
                    line,
                    format!("{path} parses its arguments by hand ({} flags named, no argument parser in Cargo.toml)", scan.flags),
                    "A hand-written loop over `env::args` usually gets `--`, `--flag=value`, clustered short flags and error messages wrong. Declare the arguments with clap (derive) or another parser, which also answers --help and --version.".into(),
                    "env::args".into(),
                ));
            }
        }
        for (_, line, summary, detail, subject) in hits {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    summary,
                    detail,
                )
                .location(Location::new(path.as_str(), Some(line)))
                .subject(format!("{path}:{subject}"))
                .positive_control(format!(
                    "{parsed_count} binary source file(s) were parsed with syn; the site was read at {path}:{line}"
                )),
            );
        }
    }
    let mut notes = vec![format!(
        "{ID}: {parsed_count} Rust binary source file(s) read"
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

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    const MANIFEST: (&str, &str) = ("Cargo.toml", "[package]\nname = \"t\"\n");

    #[test]
    fn a_fixed_editor_is_found_unless_the_file_reads_editor_or_visual() {
        let fixed =
            "fn main() { std::process::Command::new(\"nano\").arg(\"x\").status().unwrap(); }\n";
        let f = run(&[MANIFEST, ("src/main.rs", fixed)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].check_id, "OPS-012");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(1));
        let honours = "fn main() { let e = std::env::var(\"EDITOR\").unwrap_or_else(|_| \"nano\".into()); std::process::Command::new(\"nano\").status().unwrap(); let _ = e; }\n";
        assert!(run(&[MANIFEST, ("src/main.rs", honours)]).is_empty());
        let windows = "fn main() { std::process::Command::new(\"C:\\\\Windows\\\\notepad.exe\").status().unwrap(); }\n";
        assert_eq!(run(&[MANIFEST, ("src/main.rs", windows)]).len(), 1);
    }

    #[test]
    fn a_home_root_dotfile_is_found_unless_the_platform_directory_is_used() {
        let dot = "fn main() { let h = std::env::var(\"HOME\").unwrap(); let p = std::path::Path::new(&h).join(\".mytool\"); let _ = p; }\n";
        let f = run(&[MANIFEST, ("src/main.rs", dot)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].detail.contains("%APPDATA%") && f[0].detail.contains("XDG"));
        let xdg = dot.replace(
            "fn main() {",
            "fn main() { let _x = std::env::var(\"XDG_CONFIG_HOME\");",
        );
        assert!(run(&[MANIFEST, ("src/main.rs", &xdg)]).is_empty());
        let by_hand = "fn main() { let h = std::env::var(\"HOME\").unwrap(); let p = std::path::Path::new(&h).join(\".config\").join(\"t\"); let _ = p; }\n";
        assert!(run(&[MANIFEST, ("src/main.rs", by_hand)]).is_empty());
        // positive control for the quiet cases: the same file with a dotfile is found
        assert_eq!(run(&[MANIFEST, ("src/main.rs", dot)]).len(), 1);
    }

    #[test]
    fn hand_rolled_argument_parsing_is_found_only_without_a_parser_dependency() {
        let src = "fn main() { for a in std::env::args().skip(1) { match a.as_str() { \"--quiet\" => {}, \"--force\" => {}, _ => {} } } }\n";
        let f = run(&[MANIFEST, ("src/main.rs", src)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("by hand"));
        let with_clap = (
            "Cargo.toml",
            "[package]\nname = \"t\"\n[dependencies]\nclap = \"4\"\n",
        );
        assert!(run(&[with_clap, ("src/main.rs", src)]).is_empty());
        let one_flag = "fn main() { for a in std::env::args() { if a == \"--x\" {} } }\n";
        assert!(run(&[MANIFEST, ("src/main.rs", one_flag)]).is_empty());
    }

    #[test]
    fn swapped_version_and_verbose_flags_are_found_in_match_arms_and_clap_fields() {
        const PARSED: (&str, &str) = (
            "Cargo.toml",
            "[package]
name = \"t\"
[dependencies]
clap = \"4\"
",
        );
        let arms = "fn main() { match std::env::args().nth(1).as_deref() { Some(\"-v\") => println!(\"{}\", env!(\"CARGO_PKG_VERSION\")), Some(\"-V\") => { let verbose = true; let _ = verbose; } _ => {} } }\n";
        // the arm patterns are `Some(..)`, so the strings are found through the tuple-struct pattern
        let f = run(&[PARSED, ("src/main.rs", arms)]);
        assert_eq!(f.len(), 2, "{f:?}");
        let right = "fn main() { match std::env::args().nth(1).as_deref() { Some(\"-V\") => println!(\"{}\", env!(\"CARGO_PKG_VERSION\")), Some(\"-v\") => { let verbose = true; let _ = verbose; } _ => {} } }\n";
        assert!(run(&[PARSED, ("src/main.rs", right)]).is_empty());
        let clap = "struct A {\n #[arg(short = 'v')]\n version: bool,\n #[arg(short = 'V')]\n verbose: bool,\n}\nfn main() {}\n";
        assert_eq!(run(&[PARSED, ("src/main.rs", clap)]).len(), 2);
    }

    #[test]
    fn tests_and_non_binary_files_are_not_read_and_an_unparsable_file_is_counted() {
        let bad = "fn main() { std::process::Command::new(\"vim\"); }\n";
        assert!(matches!(
            judge(&RepoView::from_files(&[MANIFEST, ("src/lib.rs", bad)])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[MANIFEST, ("tests/main.rs", bad)])),
            Verdict::NotApplicable(_)
        ));
        let in_test_mod = "fn main() {}\n#[cfg(test)]\nmod tests { fn t() { std::process::Command::new(\"vim\"); } }\n";
        assert!(run(&[MANIFEST, ("src/main.rs", in_test_mod)]).is_empty());
        match judge(&RepoView::from_files(&[MANIFEST, ("src/main.rs", "fn (")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(
                    notes.iter().any(|n| n.contains("could not be parsed")),
                    "{notes:?}"
                );
            }
            _ => panic!("expected a Noted verdict"),
        }
    }
}
