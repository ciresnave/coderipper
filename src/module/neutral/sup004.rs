//! SUP-004: a dependency comes from a published registry, never from where something sits on a disk or on GitHub.
//!
//! **What this judges (CireSnave, 2026-10-07: dependencies are by published version, not by relative placement on a laptop, and
//! not on GitHub where it can be avoided), from tracked files:**
//!
//! 1. *A Cargo path dependency that leaves the repository* (`path = "../other-repo"`), in `[dependencies]`, `[dev-dependencies]`,
//!    `[build-dependencies]`, `[workspace.dependencies]`, a `[target.*]` table, `[patch.*]` and `[replace]`, and a `paths = [..]`
//!    override in `.cargo/config.toml`. High. A path that stays inside the repository (a sibling workspace member) is fine.
//! 2. *A Cargo `git =` dependency*, in the same places. Medium: it is a temporary state with a named blocker, so it is waived
//!    through an `[[allow]]` entry (file = the manifest, symbol = the dependency name) that carries the reason.
//! 3. *A Rust source file that reads a file from outside its crate*: `include!`, `include_str!`, `include_bytes!` or `#[path]` with a
//!    string literal that climbs out of the crate directory (the nearest ancestor holding a tracked `Cargo.toml`), or a
//!    `CARGO_MANIFEST_DIR` join that does. High when it leaves the repository, Medium when it only leaves the crate (the published
//!    package would lack the file).
//! 4. *An npm dependency* written `file:` / `link:` / `portal:` that leaves the repository (High), or fetched from git (`git+`, `git://`,
//!    `github:`, `gitlab:`, `bitbucket:`, `gist:`, `user/repo`) or from a tarball URL (Medium).
//!
//! The directories the other rules skip (`fixtures`, `testdata`, `conformance`, `snapshots`, `golden`, vendored code) are skipped here.
//!
//! **What it cannot see:** another ecosystem's manifests; a build script or test that finds a sibling by a hardcoded `..` join at run
//! time; an `include!` whose literal is not on the line of the macro or the next; a dependency renamed through a Cargo config
//! `[source]` replacement; whether an in-repository path dependency carries a `version` for publishing; a manifest or `package.json`
//! that does not parse (it is named in a note, never read as clean).

use super::{conflict_marker::SKIPPED, dir_of, join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use toml::Value;

const ID: &str = crate::rule_id::SUP_004;

const DEP_TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];
const NPM_TABLES: &[&str] = &[
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
];

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s))
}

/// `rel` read from directory `base` (both forward-slash, relative to the project root): the normalised path, or `None` when it
/// climbs above the root or is absolute.
fn resolve(base: &str, rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || rel.starts_with('~') || rel.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

fn within(path: &str, root: &str) -> bool {
    root.is_empty() || path == root || path.starts_with(&format!("{root}/"))
}

struct Scan<'a> {
    view: &'a RepoView,
    findings: Vec<Finding>,
    notes: Vec<String>,
    manifests: usize,
    sources: usize,
}

impl Scan<'_> {
    fn finding(
        &mut self,
        severity: Severity,
        file: &str,
        line: Option<u32>,
        subject: &str,
        summary: String,
        detail: &str,
    ) {
        self.findings.push(
            Finding::new(
                ID,
                severity,
                Confidence::High,
                self.view.name(),
                summary,
                detail,
            )
            .location(Location::new(file, line))
            .subject(subject),
        );
    }

    /// One Cargo dependency entry (`name = { path = .. }` or `name = { git = .. }`).
    fn cargo_dep(&mut self, file: &str, text: &str, name: &str, spec: &Value) {
        let Some(table) = spec.as_table() else { return };
        let dir = dir_of(file);
        let line = line_of(text, name);
        if let Some(path) = table.get("path").and_then(Value::as_str) {
            if resolve(dir, path).is_none() {
                self.finding(
                    Severity::High,
                    file,
                    line,
                    name,
                    format!("{file}: dependency `{name}` is a path outside the repository (`{path}`)"),
                    "A dependency on another repository's crate is by its crates.io version, never by where it sits on a disk: \
                     the build breaks on any other machine and the published package cannot resolve it. Depend on the published version.",
                );
            }
        }
        if let Some(git) = table.get("git").and_then(Value::as_str) {
            self.finding(
                Severity::Medium,
                file,
                line,
                name,
                format!("{file}: dependency `{name}` is fetched from git (`{git}`)"),
                "A git dependency is a temporary state with a named blocker (the crate is not yet published) and an owner. \
                 Depend on the published version, or waive it with an `[[allow]]` entry that states the blocker.",
            );
        }
    }

    fn cargo_tables(&mut self, file: &str, text: &str, root: &toml::Table) {
        let mut tables: Vec<&toml::Table> = Vec::new();
        for key in DEP_TABLES {
            tables.extend(root.get(*key).and_then(Value::as_table));
        }
        if let Some(t) = root
            .get("workspace")
            .and_then(|w| w.get("dependencies"))
            .and_then(Value::as_table)
        {
            tables.push(t);
        }
        if let Some(targets) = root.get("target").and_then(Value::as_table) {
            for cfg in targets.values().filter_map(Value::as_table) {
                for key in DEP_TABLES {
                    tables.extend(cfg.get(*key).and_then(Value::as_table));
                }
            }
        }
        if let Some(patch) = root.get("patch").and_then(Value::as_table) {
            tables.extend(patch.values().filter_map(Value::as_table));
        }
        if let Some(replace) = root.get("replace").and_then(Value::as_table) {
            tables.push(replace);
        }
        for table in tables {
            for (name, spec) in table {
                self.cargo_dep(file, text, name, spec);
            }
        }
    }

    fn cargo_manifest(&mut self, file: &str, text: &str) {
        match text.parse::<toml::Table>() {
            Ok(root) => {
                self.manifests += 1;
                self.cargo_tables(file, text, &root);
                // `.cargo/config.toml`: `paths = [..]` overrides and `[patch]`
                if let Some(paths) = root.get("paths").and_then(Value::as_array) {
                    for path in paths.iter().filter_map(Value::as_str) {
                        // a config's relative paths are read from the directory that holds `.cargo`
                        let base = dir_of(dir_of(file));
                        if resolve(base, path).is_none() {
                            self.finding(
                                Severity::High,
                                file,
                                line_of(text, path),
                                path,
                                format!("{file}: `paths` overrides a crate with a directory outside the repository (`{path}`)"),
                                "A path override makes the build depend on where a directory sits on one disk. Depend on the published version.",
                            );
                        }
                    }
                }
            }
            Err(e) => self.notes.push(format!(
                "{file} does not parse as TOML, so it was not read: {e}"
            )),
        }
    }

    fn package_json(&mut self, file: &str, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            self.notes
                .push(format!("{file} does not parse as JSON, so it was not read"));
            return;
        };
        self.manifests += 1;
        let dir = dir_of(file);
        for table in NPM_TABLES {
            let Some(deps) = json.get(*table).and_then(|d| d.as_object()) else {
                continue;
            };
            for (name, spec) in deps {
                let Some(spec) = spec.as_str() else { continue };
                let line = line_of(text, name);
                let local = ["file:", "link:", "portal:"]
                    .iter()
                    .find_map(|p| spec.strip_prefix(p));
                if let Some(path) = local {
                    if resolve(dir, path).is_none() {
                        self.finding(
                            Severity::High,
                            file,
                            line,
                            name,
                            format!("{file}: dependency `{name}` is a local path outside the repository (`{spec}`)"),
                            "A dependency on another repository is by its published version, never by where it sits on a disk.",
                        );
                    }
                } else if npm_git(spec) {
                    self.finding(
                        Severity::Medium,
                        file,
                        line,
                        name,
                        format!("{file}: dependency `{name}` is fetched from git or a URL (`{spec}`)"),
                        "Depend on the published registry version, or waive it with an `[[allow]]` entry that states the blocker.",
                    );
                }
            }
        }
    }

    fn rust_source(&mut self, file: &str, text: &str) {
        self.sources += 1;
        let dir = dir_of(file);
        let crate_root = crate_root(self.view, dir);
        let lines: Vec<&str> = text.lines().collect();
        for (i, raw) in lines.iter().enumerate() {
            let code = raw.trim_start();
            let from_manifest = code.contains("CARGO_MANIFEST_DIR");
            let reads_file = ["include!", "include_str!", "include_bytes!", "#[path"]
                .iter()
                .any(|m| code.contains(m));
            if code.starts_with("//") || !(reads_file || from_manifest) {
                continue;
            }
            // the literal on this line, else on the next
            let joined = format!("{code} {}", lines.get(i + 1).map_or("", |l| l.trim()));
            let Some(literal) = joined.split('"').nth(1) else {
                continue;
            };
            // `concat!(env!("CARGO_MANIFEST_DIR"), "/../x")`: the literal is read from the crate root
            let (base, rel) = if from_manifest {
                match joined.split('"').nth(3) {
                    Some(l) if l.contains("..") => (crate_root.as_str(), l),
                    _ => continue,
                }
            } else {
                (dir, literal)
            };
            let rel = rel.trim_start_matches('/');
            let n = u32::try_from(i + 1).ok();
            match resolve(base, rel) {
                None => self.finding(
                    Severity::High,
                    file,
                    n,
                    rel,
                    format!("{file}:{} reads `{rel}`, which is outside the repository", i + 1),
                    "A build that reads a file from outside the repository works only where that file sits. Keep the file in the crate, or depend on the published crate that owns it.",
                ),
                // a `CARGO_MANIFEST_DIR` join that stays in the repository is a test finding its workspace, not a published-crate defect
                Some(path) if !from_manifest && !within(&path, &crate_root) => self.finding(
                    Severity::Medium,
                    file,
                    n,
                    rel,
                    format!("{file}:{} reads `{rel}`, which is outside its crate", i + 1),
                    "`cargo package` ships only the crate's own directory, so the published crate cannot build. Move the file into the crate.",
                ),
                Some(_) => {}
            }
        }
    }
}

/// The nearest ancestor directory of `dir` (itself included) with a tracked `Cargo.toml`, else the project root.
fn crate_root(view: &RepoView, dir: &str) -> String {
    let mut current = dir;
    loop {
        if view.has(&join(current, "Cargo.toml")) {
            return current.to_string();
        }
        if current.is_empty() {
            return String::new();
        }
        current = dir_of(current);
    }
}

fn npm_git(spec: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "git+",
        "git://",
        "github:",
        "gitlab:",
        "bitbucket:",
        "gist:",
        "http://",
        "https://",
    ];
    if PREFIXES.iter().any(|p| spec.starts_with(p)) {
        return true;
    }
    // `user/repo` or `user/repo#ref` shorthand: no scheme, no `@scope`, one slash
    let bare = spec.split('#').next().unwrap_or(spec);
    !bare.starts_with('@')
        && !bare.contains(':')
        && bare.matches('/').count() == 1
        && !bare.starts_with('.')
        && !bare.contains(' ')
}

/// The first line that mentions `needle` (1-based): where the dependency is written, best effort.
fn line_of(text: &str, needle: &str) -> Option<u32> {
    text.lines()
        .position(|l| l.contains(needle))
        .and_then(|i| u32::try_from(i + 1).ok())
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut scan = Scan {
        view,
        findings: Vec::new(),
        notes: Vec::new(),
        manifests: 0,
        sources: 0,
    };
    let rust_candidates =
        view.paths_matching(r"(include(_str|_bytes)?!|#\[path|CARGO_MANIFEST_DIR)");
    for path in view.paths().iter().filter(|p| !skipped(p)) {
        let name = path.rsplit('/').next().unwrap_or(path);
        let cargo_config = path.ends_with(".cargo/config.toml") || path.ends_with(".cargo/config");
        if name == "Cargo.toml" || cargo_config {
            if let Some(text) = view.read(path) {
                scan.cargo_manifest(path, &text);
            }
        } else if name == "package.json" {
            if let Some(text) = view.read(path) {
                scan.package_json(path, &text);
            }
        } else if path.ends_with(".rs")
            && rust_candidates
                .as_ref()
                .is_none_or(|found| found.contains(path))
        {
            if let Some(text) = view.read(path) {
                scan.rust_source(path, &text);
            }
        }
    }
    if scan.manifests == 0 && scan.sources == 0 && scan.notes.is_empty() {
        return Verdict::NotApplicable(
            "no Cargo.toml, package.json or Rust source file outside vendored and fixture directories".into(),
        );
    }
    let positive = format!(
        "{} manifest(s) and {} Rust source file(s) with an include or path attribute were read",
        scan.manifests, scan.sources
    );
    let mut notes = scan.notes;
    notes.push(positive.clone());
    let findings = scan
        .findings
        .into_iter()
        .map(|f| f.positive_control(positive.clone()))
        .collect();
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

    fn manifest(deps: &str) -> String {
        format!("[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n{deps}")
    }

    fn subjects(f: &[Finding]) -> Vec<&str> {
        f.iter().filter_map(|f| f.subject.as_deref()).collect()
    }

    #[test]
    fn a_path_that_leaves_the_repository_is_high_in_every_table() {
        let m = manifest(
            "[dependencies]\na = { path = \"../a\" }\n[dev-dependencies]\nb = { path = \"../../b\" }\n\
             [build-dependencies]\nc = { path = \"..\\\\c\" }\n[target.'cfg(unix)'.dependencies]\nd = { path = \"../d\" }\n\
             [patch.crates-io]\ne = { path = \"../e\" }\n[replace]\nf = { path = \"../f\" }\n[workspace.dependencies]\ng = { path = \"../g\" }\n",
        );
        let found = run(&[("Cargo.toml", &m)]);
        assert_eq!(found.len(), 7, "{found:?}");
        assert!(found.iter().all(|f| f.severity == Severity::High));
        let mut s = subjects(&found);
        s.sort_unstable();
        assert_eq!(s, ["a", "b", "c", "d", "e", "f", "g"]);
    }

    #[test]
    fn a_path_inside_the_repository_is_clean() {
        let m = manifest("[dependencies]\nsib = { path = \"../sib\", version = \"0.1\" }\nsame = { path = \"crates/x\" }\n");
        assert!(run(&[("crates/a/Cargo.toml", &m)]).is_empty());
        // the same `..` from the root leaves the repository
        assert_eq!(run(&[("Cargo.toml", &m)]).len(), 1);
    }

    #[test]
    fn a_git_dependency_is_medium_and_names_the_dependency() {
        let m = manifest("[dependencies]\nx = { git = \"https://github.com/a/x\" }\ny = \"1\"\n");
        let found = run(&[("Cargo.toml", &m)]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Medium);
        assert_eq!(found[0].subject.as_deref(), Some("x"));
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(6));
        assert!(found[0].clone().validate().is_ok());
    }

    #[test]
    fn a_cargo_config_paths_override_that_leaves_the_repository_is_reported() {
        let found = run(&[(
            ".cargo/config.toml",
            "paths = [\"../other\", \"vendor/x\"]\n",
        )]);
        assert_eq!(subjects(&found), ["../other"]);
    }

    #[test]
    fn an_include_that_leaves_the_crate_is_medium_and_one_that_leaves_the_repository_is_high() {
        let m = manifest("");
        let src = "const A: &str = include_str!(\"../README.md\");\n\
                   const B: &str = include_str!(\"../../shared/x.txt\");\n\
                   const C: &str = include_str!(\"../../../../x\");\n\
                   // include_str!(\"../../../../../comment\")\n";
        let found = run(&[("crates/a/Cargo.toml", &m), ("crates/a/src/lib.rs", src)]);
        let by: Vec<(Severity, Option<u32>)> = found
            .iter()
            .map(|f| (f.severity, f.location.as_ref().unwrap().line))
            .collect();
        // README.md one level up from src/ is the crate root: inside; B leaves the crate; C leaves the repository
        assert_eq!(
            by,
            [(Severity::Medium, Some(2)), (Severity::High, Some(3))],
            "{found:?}"
        );
    }

    #[test]
    fn path_attributes_and_manifest_dir_joins_are_read() {
        let m = manifest("");
        let src = "#[path = \"../../other/mod.rs\"]\nmod other;\n\
                   const T: &str = concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../sibling/x.txt\");\n\
                   const U: &str = concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/data/x.txt\");\n";
        let found = run(&[("a/Cargo.toml", &m), ("a/src/lib.rs", src)]);
        // the `#[path]` leaves the crate; the sibling join stays in the repository and is not reported
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(1));
        let far = "const T: &str = concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../x\");
";
        let found = run(&[("a/Cargo.toml", &m), ("a/src/lib.rs", far)]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::High);
    }

    #[test]
    fn npm_local_paths_outside_and_git_forms_are_reported() {
        let pkg = r#"{"dependencies":{"a":"file:../a","b":"link:./vendored","c":"github:u/c","d":"u/d#v1","e":"^1.2.3","f":"npm:g@1","h":"@scope/h","i":"git+https://x/i.git","j":"https://x/j.tgz","k":"workspace:*"}}"#;
        let found = run(&[("package.json", pkg)]);
        let mut s = subjects(&found);
        s.sort_unstable();
        assert_eq!(s, ["a", "c", "d", "i", "j"], "{found:?}");
    }

    #[test]
    fn fixture_and_vendored_directories_are_not_read() {
        let m = manifest("[dependencies]\na = { path = \"../a\" }\n");
        assert!(run(&[
            ("Cargo.toml", &manifest("")),
            ("tests/fixtures/x/Cargo.toml", &m),
            ("vendor/y/Cargo.toml", &m)
        ])
        .is_empty());
    }

    #[test]
    fn a_manifest_that_does_not_parse_is_a_note_never_clean() {
        match judge(&RepoView::from_files(&[("Cargo.toml", "[package\n")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(
                    notes.iter().any(|n| n.contains("does not parse")),
                    "{notes:?}"
                );
            }
            _ => panic!("expected a note"),
        }
    }

    #[test]
    fn a_repository_without_manifests_is_not_applicable() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn resolve_normalises_and_refuses_to_climb_out() {
        assert_eq!(resolve("a/b", "../c").as_deref(), Some("a/c"));
        assert_eq!(resolve("a", "./x/../y").as_deref(), Some("a/y"));
        assert_eq!(resolve("a", "../../x"), None);
        assert_eq!(resolve("", "/etc"), None);
        assert_eq!(resolve("", "C:/x"), None);
    }
}
