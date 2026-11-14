// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-021: a Rust build script that reaches the network, writes outside `OUT_DIR`, never says when to rerun, or uses `unsafe`.
//!
//! **What this judges (Rust, from the source alone):** every `build.rs` that sits beside a tracked `Cargo.toml` (outside vendored
//! directories), parsed with `syn`. A build script runs on the machine of whoever builds the crate, with that person's rights, before
//! anything of theirs is compiled, so what it does is part of what they depend on. Up to four findings per script, each once, at its
//! first site, Low severity:
//!
//! - **network** (Medium confidence): a path that names a socket or an HTTP client: `TcpStream`, `TcpListener`, `UdpSocket`, `std::net`,
//!   `reqwest`, `ureq`, `curl`, `hyper`, `isahc`, `attohttpc`, `minreq`, `surf`. A build that downloads is not reproducible and fails offline.
//! - **writes outside `OUT_DIR`** (Low confidence): a call to `fs::write`, `fs::copy`, `fs::rename`, `fs::create_dir`, `fs::create_dir_all`,
//!   `fs::remove_file`, `fs::remove_dir_all` or `File::create` in a script whose text never names `OUT_DIR`. Cargo gives a build script one
//!   directory to write to; anything else changes the source tree or the machine.
//! - **no rerun directive** (Medium confidence): the script's text has no `rerun` at all (`cargo:rerun-if-changed`,
//!   `cargo:rerun-if-env-changed`, the `cargo::` forms, or a helper such as `emit_rerun_hints()`) and calls none of `prost_build`, `tonic_build`,
//!   `cc::` or `pkg_config`, which print the directives for their inputs. Without one Cargo reruns the script whenever any file of the package changes. Reported at line 1.
//! - **`unsafe`** (Medium confidence): an `unsafe` block, function or impl in the script.
//!
//! **What it cannot see:** a write whose path is built from `OUT_DIR` in one place and escapes it in another (a script that names
//! `OUT_DIR` anywhere is not read for writes); a network call made through a crate the list does not know, or by running `curl`/`git` as a
//! child process; a script named by `[package] build = "..."` other than `build.rs`; code the script runs from a build dependency; a rerun
//! directive printed by a helper whose name does not say `rerun`; **the second half of the record, proc-macro crates** (not
//! built). A script `syn` cannot parse is named in a note, never read as clean. Opt-in until a read of the portfolio's build scripts gives a
//! denominator.

use super::rust_package::skipped;
use super::{dir_of, join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;

const ID: &str = crate::rule_id::SUP_021;

/// A path segment that names a socket type or an HTTP client crate.
const NETWORK: &[&str] = &[
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "reqwest",
    "ureq",
    "curl",
    "hyper",
    "isahc",
    "attohttpc",
    "minreq",
    "surf",
];

/// `(module or type, function)` pairs that change the file system.
const WRITES: &[(&str, &str)] = &[
    ("fs", "write"),
    ("fs", "copy"),
    ("fs", "rename"),
    ("fs", "create_dir"),
    ("fs", "create_dir_all"),
    ("fs", "remove_file"),
    ("fs", "remove_dir_all"),
    ("File", "create"),
];

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

/// The first site of each kind in one script.
#[derive(Default)]
struct Sites {
    network: Option<(u32, String)>,
    write: Option<(u32, String)>,
    unsafe_code: Option<u32>,
}

impl Sites {
    fn network(&mut self, line: u32, what: &str) {
        if self.network.is_none() {
            self.network = Some((line, what.to_string()));
        }
    }
}

impl<'ast> Visit<'ast> for Sites {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let names: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let net = names.iter().position(|n| NETWORK.contains(&n.as_str()));
        if let Some(i) = net {
            self.network(line_of(path.segments[i].ident.span()), &names[i]);
        } else if names.windows(2).any(|w| w[0] == "std" && w[1] == "net") {
            self.network(line_of(path.segments[0].ident.span()), "std::net");
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn walk(tree: &syn::UseTree, trail: &mut Vec<String>, found: &mut Option<(u32, String)>) {
            match tree {
                syn::UseTree::Path(p) => {
                    trail.push(p.ident.to_string());
                    if found.is_none() {
                        if NETWORK.contains(&p.ident.to_string().as_str()) {
                            *found = Some((line_of(p.ident.span()), p.ident.to_string()));
                        } else if trail.len() >= 2 && trail[0] == "std" && trail[1] == "net" {
                            *found = Some((line_of(p.ident.span()), "std::net".to_string()));
                        }
                    }
                    walk(&p.tree, trail, found);
                    trail.pop();
                }
                syn::UseTree::Group(g) => {
                    for t in &g.items {
                        walk(t, trail, found);
                    }
                }
                syn::UseTree::Name(n)
                    if found.is_none() && NETWORK.contains(&n.ident.to_string().as_str()) =>
                {
                    *found = Some((line_of(n.ident.span()), n.ident.to_string()));
                }
                _ => {}
            }
        }
        let mut found = None;
        walk(&item.tree, &mut Vec::new(), &mut found);
        if let Some((line, what)) = found {
            self.network(line, &what);
        }
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*call.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if let [.., a, b] = segs.as_slice() {
                if self.write.is_none() && WRITES.iter().any(|(x, y)| a == x && b == y) {
                    self.write = Some((
                        line_of(p.path.segments[0].ident.span()),
                        format!("{a}::{b}"),
                    ));
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        if self.unsafe_code.is_none() {
            self.unsafe_code = Some(line_of(e.unsafe_token.span));
        }
        syn::visit::visit_expr_unsafe(self, e);
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if self.unsafe_code.is_none() {
            if let syn::Safety::Unsafe(u) = &f.sig.safety {
                self.unsafe_code = Some(line_of(u.span));
            }
        }
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if self.unsafe_code.is_none() {
            if let Some(u) = &i.unsafety {
                self.unsafe_code = Some(line_of(u.span));
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

/// Build helpers that print Cargo's rerun directives for the inputs they read (`prost_build` for its `.proto` files, `cc` for its
/// sources, `pkg_config` for its library); a script that calls one of them has said when to rerun.
const EMITTERS: &[&str] = &[
    "prost_build",
    "tonic_build",
    "tonic_prost_build",
    "cc::",
    "pkg_config",
];

/// Whether the script tells Cargo when to rerun it: it spells out a directive, calls a helper whose name says `rerun`
/// (`emit_rerun_hints()`), or calls a build crate that prints them for its inputs.
fn says_when_to_rerun(text: &str) -> bool {
    text.contains("rerun") || EMITTERS.iter().any(|e| text.contains(e))
}

/// What every finding of one script shares: the repository, the script's path and how many scripts were read.
struct Script<'a> {
    view: &'a RepoView,
    path: &'a str,
    scripts: usize,
}

impl Script<'_> {
    fn finding(
        &self,
        line: u32,
        confidence: Confidence,
        kind: &str,
        message: String,
        remediation: &str,
    ) -> Finding {
        let (path, scripts) = (self.path, self.scripts);
        Finding::new(ID, Severity::Low, confidence, self.view.name(), message, remediation)
            .location(Location::new(path, Some(line)))
            .subject(format!("{path}: {kind}"))
            .positive_control(format!(
                "{scripts} build script(s) were read for the same four things, and a script that does none of them is not reported"
            ))
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut scripts: Vec<String> = Vec::new();
    for path in view.paths() {
        let is_script = path == "build.rs" || path.ends_with("/build.rs");
        if !is_script || skipped(path) {
            continue;
        }
        if view.has(&join(dir_of(path), "Cargo.toml")) {
            scripts.push(path.clone());
        }
    }
    if scripts.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no build script (a build.rs beside a Cargo.toml)".into(),
        );
    }
    let total = scripts.len();
    let mut findings = Vec::new();
    let mut unparsed = Vec::new();
    for path in &scripts {
        let Some(text) = view.read(path) else {
            unparsed.push(path.clone());
            continue;
        };
        let Ok(file) = syn::parse_file(&text) else {
            unparsed.push(path.clone());
            continue;
        };
        let script = Script {
            view,
            path,
            scripts: total,
        };
        let mut sites = Sites::default();
        sites.visit_file(&file);
        if let Some((line, what)) = &sites.network {
            findings.push(script.finding(
                *line,
                Confidence::Medium,
                "network",
                format!("the build script `{path}` reaches the network (`{what}`): a build that downloads is not reproducible and fails offline"),
                "Vendor or pin what the script fetches and check it in, or move the download to an explicit step the builder runs and can see; a build script that needs the network should say so in the crate's documentation.",
            ));
        }
        if !text.contains("OUT_DIR") {
            if let Some((line, what)) = &sites.write {
                findings.push(script.finding(
                    *line,
                    Confidence::Low,
                    "write",
                    format!("the build script `{path}` writes files (`{what}`) and never names `OUT_DIR`: it writes into the source tree or elsewhere on the builder's machine"),
                    "Write generated files under the directory in the `OUT_DIR` environment variable, which Cargo gives a build script for this, and `include!` them from there.",
                ));
            }
        }
        if !says_when_to_rerun(&text) {
            findings.push(script.finding(
                1,
                Confidence::Medium,
                "rerun",
                format!("the build script `{path}` prints no `cargo:rerun-if-changed` or `cargo:rerun-if-env-changed`: Cargo reruns it whenever any file of the package changes"),
                "Print `cargo:rerun-if-changed=build.rs` and one `cargo:rerun-if-changed=<input>` or `cargo:rerun-if-env-changed=<VAR>` per input the script reads.",
            ));
        }
        if let Some(line) = sites.unsafe_code {
            findings.push(script.finding(
                line,
                Confidence::Medium,
                "unsafe",
                format!("the build script `{path}` uses `unsafe`: it runs unreviewed on every builder's machine before their code compiles"),
                "Avoid `unsafe` in a build script; use a safe wrapper crate for what it needs, or move the work to a build dependency that is reviewed as one.",
            ));
        }
    }
    match super::rust_package::unparsed_note(ID, &unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";
    const RERUN: &str = "println!(\"cargo:rerun-if-changed=build.rs\");";

    fn found(script: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("build.rs", script),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn kinds(script: &str) -> Vec<String> {
        let mut k: Vec<String> = found(script)
            .iter()
            .map(|f| f.subject.clone().unwrap_or_default())
            .map(|s| s.rsplit(": ").next().unwrap_or("").to_string())
            .collect();
        k.sort();
        k
    }

    #[test]
    fn a_tidy_script_is_clean() {
        let body = format!(
            "fn main() {{ {RERUN} let out = std::env::var(\"OUT_DIR\").unwrap(); std::fs::write(format!(\"{{out}}/x.rs\"), \"\").unwrap(); }}"
        );
        assert!(found(&body).is_empty());
    }

    #[test]
    fn a_script_without_a_rerun_directive_is_reported_at_line_one() {
        let f = found("fn main() {}\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.as_ref().map(|l| l.line), Some(Some(1)));
        assert!(kinds("fn main() { println!(\"cargo::rerun-if-env-changed=X\"); }").is_empty());
    }

    #[test]
    fn a_helper_or_build_crate_that_prints_the_directives_counts() {
        assert!(found("fn main() { my_build::emit_rerun_hints(); }").is_empty());
        assert!(found("fn main() -> std::io::Result<()> { prost_build::compile_protos(&[\"a.proto\"], &[\"src/\"])?; Ok(()) }").is_empty());
        assert!(found("fn main() { cc::Build::new().file(\"a.c\").compile(\"a\"); }").is_empty());
    }

    #[test]
    fn a_network_client_or_socket_is_reported() {
        for body in [
            "fn main() { let _ = std::net::TcpStream::connect(\"a:1\"); }",
            "use std::net::TcpStream;\nfn main() {}",
            "fn main() { let _ = reqwest::blocking::get(\"http://a\"); }",
            "use ureq::get;\nfn main() {}",
        ] {
            let script = format!("{body}\n// {RERUN}\n");
            assert_eq!(kinds(&script), vec!["network"], "{body}");
        }
    }

    #[test]
    fn a_write_is_reported_only_when_out_dir_is_never_named() {
        let bad = format!("fn main() {{ {RERUN} std::fs::write(\"src/gen.rs\", \"\").unwrap(); }}");
        assert_eq!(kinds(&bad), vec!["write"]);
        let good = format!(
            "fn main() {{ {RERUN} let _ = std::env::var(\"OUT_DIR\"); std::fs::write(\"src/gen.rs\", \"\").unwrap(); }}"
        );
        assert!(found(&good).is_empty());
    }

    #[test]
    fn unsafe_in_a_script_is_reported() {
        for body in [
            "fn main() { unsafe { std::env::set_var(\"A\", \"b\"); } }",
            "unsafe fn f() {}\nfn main() {}",
            "struct S;\nunsafe impl Send for S {}\nfn main() {}",
        ] {
            let script = format!("{body}\n// {RERUN}\n");
            assert_eq!(kinds(&script), vec!["unsafe"], "{body}");
        }
    }

    #[test]
    fn each_kind_is_reported_once_per_script() {
        let body = "use std::net::TcpStream;\nfn main() { let _ = TcpStream::connect(\"a\"); std::fs::write(\"a\", \"\").unwrap(); std::fs::copy(\"a\", \"b\").unwrap(); unsafe {} unsafe {} }\n";
        assert_eq!(kinds(body), vec!["network", "rerun", "unsafe", "write"]);
    }

    #[test]
    fn a_repository_without_a_build_script_is_not_applicable() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub fn f() {}"),
        ]));
        assert!(matches!(v, Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_vendored_script_and_one_with_no_manifest_are_not_read() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("vendor/x/Cargo.toml", MANIFEST),
            ("vendor/x/build.rs", "fn main() {}"),
            ("loose/build.rs", "fn main() {}"),
        ]));
        assert!(matches!(v, Verdict::NotApplicable(_)));
    }

    #[test]
    fn an_unparsable_script_is_a_note_not_a_clean_pass() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("build.rs", "fn main( {"),
        ]));
        assert!(matches!(v, Verdict::Noted(f, n) if f.is_empty() && n.len() == 1));
    }
}
