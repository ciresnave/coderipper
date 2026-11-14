// SPDX-License-Identifier: MIT OR Apache-2.0
//! SUP-006: a declared dependency that the package never uses (Rust).
//!
//! **What this judges (Rust, from the manifest and the source text):** every tracked `Cargo.toml` with a `[package]` table is a package;
//! its `.rs` files are the tracked ones below its directory that no nearer `Cargo.toml` owns (outside vendored directories). A
//! dependency is a key of `[dependencies]`, `[build-dependencies]` or `[dev-dependencies]` (and of the `[target.*.*dependencies]` forms),
//! read as the identifier Rust code spells it: the key with `-` as `_`, compared without regard to case (the `package = ".."` key of a
//! renamed dependency does not matter: the key is the name in code). It counts as **used** when that identifier is a token (a run of
//! letters, digits and `_`) anywhere in the text of the package's `.rs` files (comments and macro bodies included, so a use in a macro
//! argument or a doc test counts), or, for a build dependency, in `build.rs` or a file under `build/`; or when the package's `[features]`
//! forward to it (`dep:name`, `name/feature`, `name?/feature`).
//!
//! A dependency that is not used is reported once, at its line in `Cargo.toml`: Low severity, Low confidence.
//!
//! **Not reported:** a dependency the manifest says to ignore (`[package.metadata.cargo-machete] ignored = [..]` or
//! `[package.metadata.cargo-udeps.ignore]`, the lists those tools read, so a declared exception is one that already exists); a crate
//! linked for its side effects that code never names (a name ending in `-sys` or `_sys`, or `openssl`, `openssl-probe`, `native-tls`,
//! `mimalloc`, `tikv-jemallocator`, `jemallocator`, `link-cplusplus`); a package with no `.rs` file.
//!
//! **Not read:** the normal and dev dependencies of a package whose source names `OUT_DIR` (it includes generated code, such as
//! `bindgen` output, that this reading does not have; a build dependency is still read).
//!
//! **What it cannot see (and the record says so):** a crate whose library name differs from its package name (code spells the library
//! name: `sha-1` is used as `sha1`), a crate used only through a derive or attribute whose text never names it, a crate used in generated
//! code or in a file another tool writes, a dependency used only in the *other* kind's code (a normal dependency used only in tests counts as
//! used), the `[workspace.dependencies]` table (a declaration, not a use) and other languages. A file that is not UTF-8 text is skipped.

use super::{dir_and_ancestors, dir_of, join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet};

const ID: &str = crate::rule_id::SUP_006;

/// Directories that hold somebody else's code.
const VENDORED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "target",
];

/// Crates linked for what they do to the binary, which code never names.
const EFFECT_ONLY: &[&str] = &[
    "openssl",
    "openssl_probe",
    "native_tls",
    "mimalloc",
    "tikv_jemallocator",
    "jemallocator",
    "link_cplusplus",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Normal,
    Build,
    Dev,
}

struct Dep {
    key: String,
    ident: String,
    kind: Kind,
}

fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

fn skipped(path: &str) -> bool {
    path.split('/').any(|s| VENDORED.contains(&s))
}

fn dependencies(doc: &toml::Table) -> Vec<Dep> {
    let mut out = Vec::new();
    let mut take = |t: Option<&toml::Value>, kind: Kind| {
        if let Some(t) = t.and_then(|t| t.as_table()) {
            for key in t.keys() {
                out.push(Dep {
                    key: key.clone(),
                    ident: key.replace('-', "_").to_ascii_lowercase(),
                    kind,
                });
            }
        }
    };
    take(doc.get("dependencies"), Kind::Normal);
    take(doc.get("build-dependencies"), Kind::Build);
    take(doc.get("dev-dependencies"), Kind::Dev);
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for (_, v) in targets {
            take(v.get("dependencies"), Kind::Normal);
            take(v.get("build-dependencies"), Kind::Build);
            take(v.get("dev-dependencies"), Kind::Dev);
        }
    }
    out
}

/// Names a manifest says to ignore (the lists `cargo-machete` and `cargo-udeps` read).
fn ignored(doc: &toml::Table) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let meta = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.as_table());
    let Some(meta) = meta else {
        return out;
    };
    let mut add = |v: Option<&toml::Value>| {
        if let Some(a) = v.and_then(|v| v.as_array()) {
            for s in a.iter().filter_map(|s| s.as_str()) {
                out.insert(s.replace('-', "_").to_ascii_lowercase());
            }
        }
    };
    add(meta.get("cargo-machete").and_then(|m| m.get("ignored")));
    if let Some(u) = meta.get("cargo-udeps").and_then(|m| m.get("ignore")) {
        for k in ["normal", "development", "build"] {
            add(u.get(k));
        }
    }
    out
}

/// Names the `[features]` table forwards to (`dep:x`, `x/f`, `x?/f`).
fn forwarded(doc: &toml::Table) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(f) = doc.get("features").and_then(|f| f.as_table()) {
        for v in f.values() {
            for s in v
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str())
            {
                let s = s.strip_prefix("dep:").unwrap_or(s);
                let head = s.split('/').next().unwrap_or(s).trim_end_matches('?');
                out.insert(head.replace('-', "_").to_ascii_lowercase());
            }
        }
    }
    out
}

fn line_of_key(manifest: &str, key: &str) -> u32 {
    for (i, l) in manifest.lines().enumerate() {
        let t = l.trim_start();
        let name = t
            .split(['=', '.', ' ', '\t'])
            .next()
            .unwrap_or("")
            .trim_matches('"');
        if name == key {
            return u32::try_from(i + 1).unwrap_or(0);
        }
        if t.starts_with('[') && t.ends_with(&format!(".{key}]")) {
            return u32::try_from(i + 1).unwrap_or(0);
        }
    }
    0
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let manifests: Vec<String> = view
        .paths()
        .iter()
        .filter(|p| (p.as_str() == "Cargo.toml" || p.ends_with("/Cargo.toml")) && !skipped(p))
        .cloned()
        .collect();
    let dirs: BTreeSet<String> = manifests.iter().map(|m| dir_of(m).to_string()).collect();
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in view.paths() {
        if !p.ends_with(".rs") || skipped(p) {
            continue;
        }
        if let Some(d) = dir_and_ancestors(dir_of(p))
            .into_iter()
            .find(|d| dirs.contains(d))
        {
            by_dir.entry(d).or_default().push(p.clone());
        }
    }
    let mut findings = Vec::new();
    let mut packages = 0usize;
    let mut declared = 0usize;
    for m in &manifests {
        let Some(text) = view.read(m) else { continue };
        let Ok(doc) = text.parse::<toml::Table>() else {
            continue;
        };
        if doc.get("package").is_none() {
            continue;
        }
        let dir = dir_of(m).to_string();
        let Some(files) = by_dir.get(&dir) else {
            continue;
        };
        let deps = dependencies(&doc);
        if deps.is_empty() {
            continue;
        }
        packages += 1;
        declared += deps.len();
        let mut all: BTreeSet<String> = BTreeSet::new();
        let mut build: BTreeSet<String> = BTreeSet::new();
        for f in files {
            let Some(t) = view.read(f) else { continue };
            let toks = tokens(&t);
            let rel = f.strip_prefix(&join(&dir, "")).unwrap_or(f);
            if rel == "build.rs" || rel.starts_with("build/") {
                build.extend(toks.iter().cloned());
            }
            all.extend(toks);
        }
        // Code that is generated into OUT_DIR and included is not in the files read: a use in it cannot be seen.
        let includes_generated = all.contains("out_dir");
        let ign = ignored(&doc);
        let fwd = forwarded(&doc);
        let pkg_name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or(&dir)
            .to_string();
        for d in deps {
            let used = match d.kind {
                Kind::Build => build.contains(&d.ident),
                _ => all.contains(&d.ident),
            };
            if used
                || (includes_generated && d.kind != Kind::Build)
                || fwd.contains(&d.ident)
                || ign.contains(&d.ident)
                || d.ident.ends_with("_sys")
                || EFFECT_ONLY.contains(&d.ident.as_str())
            {
                continue;
            }
            let kind = match d.kind {
                Kind::Normal => "dependency",
                Kind::Build => "build dependency",
                Kind::Dev => "dev dependency",
            };
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Low,
                    view.name(),
                    format!(
                        "the {kind} `{}` of `{pkg_name}` is declared and its name appears in none of the package's {} Rust file(s)",
                        d.key,
                        files.len()
                    ),
                    "Remove the declaration, or say why a dependency that looks unused is needed (a crate linked for its side effects, a derive or attribute that never names it, a library name that differs from the package name): list it under `[package.metadata.cargo-machete] ignored`.",
                )
                .location(Location::new(m.as_str(), Some(line_of_key(&text, &d.key))))
                .subject(format!("{pkg_name}:{}", d.key))
                .positive_control(format!(
                    "{packages} package(s) with {declared} declared dependencies were read against the identifier tokens of their Rust files; a dependency whose name appears is not reported"
                )),
            );
        }
    }
    if packages == 0 {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package that declares a dependency and has a Rust source file".into(),
        );
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn manifest(deps: &str) -> String {
        format!("[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n{deps}")
    }

    #[test]
    fn a_dependency_no_source_names_is_reported_and_a_used_one_is_not() {
        let m = manifest("[dependencies]\nserde_json = \"1\"\nanyhow = \"1\"\n");
        let f = found(&[
            ("Cargo.toml", m.as_str()),
            (
                "src/lib.rs",
                "pub fn f() -> serde_json::Value { todo!() }\n",
            ),
        ]);
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("`anyhow`"));
    }

    #[test]
    fn hyphens_case_macro_arguments_and_use_in_tests_count_as_use() {
        let m = manifest(
            "[dependencies]\nserde-json = \"1\"\nInflector = \"0.1\"\nthiserror = \"1\"\n",
        );
        assert!(found(&[
            ("Cargo.toml", m.as_str()),
            (
                "src/lib.rs",
                "use serde_json::Value;\nfn f() { m!(inflector::cases::x); }\n"
            ),
            ("tests/t.rs", "use thiserror::Error;\n"),
        ])
        .is_empty());
    }

    #[test]
    fn a_build_dependency_is_used_in_the_build_script_and_a_feature_can_forward_to_one() {
        let m = manifest("[build-dependencies]\ncc = \"1\"\n\n[dependencies]\nzeroize = { version = \"1\", optional = true }\n\n[features]\nz = [\"dep:zeroize\"]\n");
        assert!(found(&[
            ("Cargo.toml", m.as_str()),
            ("src/lib.rs", ""),
            ("build.rs", "fn main() { cc::Build::new(); }\n"),
        ])
        .is_empty());
        let f = found(&[
            ("Cargo.toml", m.as_str()),
            ("src/lib.rs", "fn f() { let cc = 1; }\n"),
        ]);
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("build dependency `cc`"));
    }

    #[test]
    fn ignored_effect_only_and_sys_crates_are_not_reported() {
        let m = manifest("[dependencies]\nopenssl = \"0.10\"\nlibz-sys = \"1\"\nodd = \"1\"\n\n[package.metadata.cargo-machete]\nignored = [\"odd\"]\n");
        assert!(found(&[("Cargo.toml", m.as_str()), ("src/lib.rs", "")]).is_empty());
    }

    #[test]
    fn a_package_that_includes_generated_code_is_not_read_for_normal_dependencies() {
        let m = manifest(
            "[dependencies]
libc = \"0.2\"
",
        );
        assert!(found(&[
            ("Cargo.toml", m.as_str()),
            (
                "src/lib.rs",
                "include!(concat!(env!(\"OUT_DIR\"), \"/bindings.rs\"));
",
            ),
        ])
        .is_empty());
    }

    #[test]
    fn a_member_package_is_read_on_its_own_files() {
        let root = "[workspace]\nmembers = [\"a\"]\n";
        let a = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nregex = \"1\"\n";
        let f = found(&[
            ("Cargo.toml", root),
            ("a/Cargo.toml", a),
            ("a/src/lib.rs", "pub fn f() {}\n"),
            ("src/lib.rs", "use regex::Regex;\n"),
        ]);
        assert_eq!(f.len(), 1);
    }
}
