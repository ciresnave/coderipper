// SPDX-License-Identifier: MIT OR Apache-2.0
//! TST-006: tests are hermetic (the static reading, Rust).
//!
//! **What this judges:** every tracked `.rs` file outside vendored and fixture directories is parsed with `syn`, and the body of every
//! function with a `#[test]` attribute (`#[test]`, `#[tokio::test]`, anything whose last path segment is `test`) is read for three
//! kinds of environmental reach:
//!
//! 1. **network**: a call to `TcpStream::connect`, `TcpStream::connect_timeout`, `UdpSocket::connect`, or `get`/`post`/`put`/`delete`/
//!    `head`/`patch`/`request` on `reqwest`, `ureq`, `surf` or `isahc`;
//! 2. **clock**: `SystemTime::now`, `Utc::now`, `Local::now`, `OffsetDateTime::now_utc`/`now_local`, `Timestamp::now`, `Zoned::now`,
//!    `UtcDateTime::now`;
//! 3. **shared filesystem**: a string literal that is an absolute path into a shared location (`/tmp`, `/var/`, `/etc/`, `/home/`,
//!    `/usr/`, `C:\`, `C:/`, `~/`) or a call to `env::temp_dir()`.
//!
//! A test that binds a `TcpListener` or names a loopback address (`127.0.0.1`, `localhost`, `[::1]`, `0.0.0.0`) is a local-server test
//! and its network calls are not reported. A test marked `#[ignore]` (the declared way to keep an environment-dependent test out of
//! the default run) is left alone. `std::time::Instant` is not read as a clock (it is monotonic and measures, it does not date).
//! One **Medium**-severity, **Low**-confidence finding per test, naming the kinds found: this is a static proxy, the full rule is
//! proved by running the suite offline with the clock frozen.
//!
//! **What it cannot see (and the record says so):** reach made by a helper the test calls, by a fixture, or by the code under test;
//! a path built at run time; a network client other than the ones named; a clock injected and frozen (a call to a name this list does
//! not know is invisible, and a frozen `now` that is read through a listed name is reported); other languages. A file `syn` cannot
//! parse is counted in a note, never read as clean.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::TST_006;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub(super) enum Reach {
    Network,
    Clock,
    SharedFile,
}

impl Reach {
    fn label(self) -> &'static str {
        match self {
            Reach::Network => "the network",
            Reach::Clock => "the wall clock",
            Reach::SharedFile => "a shared location on disk",
        }
    }
}

pub(super) struct Site {
    pub line: u32,
    pub name: String,
    pub reach: Vec<Reach>,
}

fn is_test_attr(a: &syn::Attribute) -> bool {
    a.path().segments.last().is_some_and(|s| s.ident == "test")
        && !a.path().is_ident("cfg")
        && matches!(a.meta, syn::Meta::Path(_))
}

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

const LOOPBACK: &[&str] = &["127.0.0.1", "localhost", "[::1]", "0.0.0.0"];
const SHARED_PREFIXES: &[&str] = &[
    "/tmp/", "/var/", "/etc/", "/home/", "/usr/", "C:\\", "C:/", "~/",
];
const HTTP_CLIENTS: &[&str] = &["reqwest", "ureq", "surf", "isahc"];
const HTTP_VERBS: &[&str] = &["get", "post", "put", "delete", "head", "patch", "request"];

#[derive(Default)]
struct Body {
    network: bool,
    clock: bool,
    shared: bool,
    loopback: bool,
}

fn call_reach(path: &syn::Path) -> Option<Reach> {
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let last = segs.last()?.as_str();
    let before = segs.len().checked_sub(2).map(|i| segs[i].as_str());
    match (before, last) {
        (Some("TcpStream"), "connect" | "connect_timeout") | (Some("UdpSocket"), "connect") => {
            Some(Reach::Network)
        }
        (
            Some("SystemTime" | "Utc" | "Local" | "Timestamp" | "Zoned" | "UtcDateTime"),
            "now",
        )
        | (Some("OffsetDateTime"), "now_utc" | "now_local") => Some(Reach::Clock),
        (Some("env"), "temp_dir") => Some(Reach::SharedFile),
        _ if segs.len() == 1 && last == "temp_dir" => Some(Reach::SharedFile),
        _ if segs.len() >= 2
            && HTTP_CLIENTS.contains(&segs[0].as_str())
            && HTTP_VERBS.contains(&last) =>
        {
            Some(Reach::Network)
        }
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Body {
    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*e.func {
            let segs = &p.path.segments;
            if segs.len() >= 2
                && segs[segs.len() - 2].ident == "TcpListener"
                && segs[segs.len() - 1].ident == "bind"
            {
                self.loopback = true;
            }
            match call_reach(&p.path) {
                Some(Reach::Network) => self.network = true,
                Some(Reach::Clock) => self.clock = true,
                Some(Reach::SharedFile) => self.shared = true,
                None => {}
            }
        }
        syn::visit::visit_expr_call(self, e);
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        let v = s.value();
        if LOOPBACK.iter().any(|l| v.contains(l)) {
            self.loopback = true;
        }
        if SHARED_PREFIXES.iter().any(|p| v.starts_with(p)) || v == "/tmp" {
            self.shared = true;
        }
    }
}

fn judge_fn(attrs: &[syn::Attribute], block: &syn::Block) -> Option<Vec<Reach>> {
    if !attrs.iter().any(is_test_attr) || has_attr(attrs, "ignore") {
        return None;
    }
    let mut body = Body::default();
    body.visit_block(block);
    let mut reach = Vec::new();
    if body.network && !body.loopback {
        reach.push(Reach::Network);
    }
    if body.clock {
        reach.push(Reach::Clock);
    }
    if body.shared {
        reach.push(Reach::SharedFile);
    }
    (!reach.is_empty()).then_some(reach)
}

#[derive(Default)]
struct Walk {
    sites: Vec<Site>,
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if let Some(reach) = judge_fn(&f.attrs, &f.block) {
            self.sites.push(Site {
                line: u32::try_from(f.sig.span().start().line).unwrap_or(0),
                name: f.sig.ident.to_string(),
                reach,
            });
        }
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if let Some(reach) = judge_fn(&f.attrs, &f.block) {
            self.sites.push(Site {
                line: u32::try_from(f.sig.span().start().line).unwrap_or(0),
                name: f.sig.ident.to_string(),
                reach,
            });
        }
        syn::visit::visit_impl_item_fn(self, f);
    }
}

/// The non-hermetic tests of one file, or `None` when `syn` cannot parse it.
pub(super) fn sites(text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut walk = Walk::default();
    walk.visit_file(&parsed);
    walk.sites.sort_by_key(|s| s.line);
    Some(walk.sites)
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| super::conflict_marker::SKIPPED.contains(s))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !skipped(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust source file".into());
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        let Some(found) = view.read(path).and_then(|t| sites(&t)) else {
            unparsed.push(path);
            continue;
        };
        for s in found {
            let what = s
                .reach
                .iter()
                .map(|r| r.label())
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::Low,
                    view.name(),
                    format!("test `{}` reaches {what}", s.name),
                    "A test that reads the network, the wall clock or a shared path gives a different answer on another machine or at another time. Inject the time, serve from a local fake, use a temporary directory (`tempfile`), or mark the test `#[ignore]` with a reason and run it on purpose.",
                )
                .location(Location::new(path.as_str(), Some(s.line)))
                .subject(s.name)
                .positive_control(format!(
                    "{} Rust source file(s) were parsed with the same walk that finds `#[test]` functions",
                    files.len()
                )),
            );
        }
    }
    let note = (!unparsed.is_empty()).then(|| {
        format!(
            "{ID}: {} Rust file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        )
    });
    match note {
        Some(n) => Verdict::Noted(findings, vec![n]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<(String, Vec<Reach>)> {
        sites(src)
            .unwrap()
            .into_iter()
            .map(|s| (s.name, s.reach))
            .collect()
    }

    #[test]
    fn network_clock_and_shared_path_are_reported() {
        let src = r#"
#[test]
fn net() { let _ = std::net::TcpStream::connect("example.org:80"); }
#[test]
fn http() { let _ = reqwest::blocking::get("https://example.org"); }
#[test]
fn clock() { let _ = std::time::SystemTime::now(); }
#[test]
fn chrono_clock() { let _ = chrono::Utc::now(); }
#[test]
fn file() { std::fs::write("/tmp/out.txt", "x").unwrap(); }
#[test]
fn tmp() { let _ = std::env::temp_dir(); }
#[test]
fn all() { let _ = TcpStream::connect("a:1"); let _ = Local::now(); let _ = "/etc/hosts"; }
"#;
        assert_eq!(
            names(src),
            [
                ("net".to_string(), vec![Reach::Network]),
                ("http".to_string(), vec![Reach::Network]),
                ("clock".to_string(), vec![Reach::Clock]),
                ("chrono_clock".to_string(), vec![Reach::Clock]),
                ("file".to_string(), vec![Reach::SharedFile]),
                ("tmp".to_string(), vec![Reach::SharedFile]),
                (
                    "all".to_string(),
                    vec![Reach::Network, Reach::Clock, Reach::SharedFile]
                ),
            ]
        );
    }

    #[test]
    fn local_servers_ignored_tests_instants_and_non_tests_are_not_reported() {
        let src = r#"
#[test]
fn loopback() { let _ = std::net::TcpStream::connect("127.0.0.1:8080"); }
#[test]
fn serves() { let l = TcpListener::bind(addr).unwrap(); let _ = TcpStream::connect(l.local_addr().unwrap()); }
#[test]
#[ignore = "needs the network"]
fn live() { let _ = reqwest::get("https://example.org"); }
#[test]
fn timing() { let _ = std::time::Instant::now(); }
#[test]
fn tempfile_dir() { let _ = tempfile::tempdir(); }
fn helper() { let _ = std::time::SystemTime::now(); }
#[test]
fn relative() { let _ = std::fs::read("fixtures/a.txt"); }
"#;
        assert_eq!(names(src), []);
    }

    #[test]
    fn tests_in_cfg_test_modules_and_async_tests_are_read() {
        let src = r#"
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a() { let _ = chrono::Local::now(); }
}
"#;
        assert_eq!(names(src), [("a".to_string(), vec![Reach::Clock])]);
    }

    #[test]
    fn an_unparsable_file_is_noted_not_clean() {
        let v = judge(&RepoView::from_files(&[("src/lib.rs", "fn (((")]));
        assert!(matches!(v, Verdict::Noted(f, n) if f.is_empty() && n.len() == 1));
    }

    #[test]
    fn every_finding_validates() {
        let v = judge(&RepoView::from_files(&[(
            "tests/a.rs",
            "#[test]\nfn t() { let _ = Utc::now(); }\n",
        )]));
        let Verdict::Findings(f) = v else {
            panic!("expected findings")
        };
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        f.into_iter().next().unwrap().validate().unwrap();
    }
}
