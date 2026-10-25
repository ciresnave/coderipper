// SPDX-License-Identifier: MIT OR Apache-2.0
//! `patent-index-match`: a declared dependency that the agent-safety-bed patent index lists.
//!
//! **What this judges:** each dependency a manifest declares is compared by registry name with the `match` lists of the pinned
//! patent-safety index (`data/patent-tiers.json`, with its version and commit in `data/patent-tiers.PIN`). A match is reported by the
//! entry's `action`: `avoid` is High, `flag-for-review` is Medium, `allow-with-conditions` and `separate-component` are Low, `allow`
//! is not reported. The finding carries the entry's id, tier and note.
//!
//! **What a finding means:** an *index match*, nothing more. The index is engineering due diligence, "NOT legal advice. NOT
//! freedom-to-operate clearance", and a dependency with no match is not cleared: the index lists 66 techniques, not the patent
//! landscape. A match also does not prove the avoided technique is in use (an entry's caveat is often about one feature of the
//! package). Nothing here says a project is patent-safe.
//!
//! Manifests read: `Cargo.toml` (dependency, dev-, build-, workspace and target tables; a renamed dependency by its `package`),
//! `package.json` (all four dependency tables; an `npm:` alias by its real name), `pyproject.toml` (PEP 621 dependencies, optional
//! dependencies and dependency groups; Poetry tables), `requirements*.txt`, `pom.xml` (each `<dependency>` by `groupId:artifactId`;
//! a property placeholder is compared as written) and `build.gradle[.kts]` (each quoted `group:artifact[:version]` string). The directories the other rules skip are skipped.
//!
//! **Lockfiles (transitive dependencies):** `Cargo.lock`, `package-lock.json` (lockfileVersion 1, 2 and 3), `poetry.lock`, `uv.lock` and
//! `Pipfile.lock` list every package that is resolved, so a package that no manifest declares (a transitive dependency) can match the
//! index too. A locked match that a manifest in the repository also declares is reported once, from the manifest. A match found only in a
//! lockfile is graded one step below the index's action (High is Medium, Medium and Low are Low) and at low confidence: the repository may
//! not choose it, so it must not fail a gate that a direct dependency would. `yarn.lock` (classic and Berry), `pnpm-lock.yaml` (lockfile
//! versions 5 to 9) and `gradle.lockfile` are read the same way. `go.sum` is **not** read: the index lists no Go module (it names crates, PyPI,
//! npm and Maven packages only), so a Go lockfile could match nothing. Gradle version catalogs (`*.versions.toml`, `[libraries]`) are read as manifests.
//!
//! **What it cannot see:** a coordinate built from variables; a lockfile format listed above as not read; a technique implemented in the project's own source or copied in; a package the index lists under no name
//! (`match` is empty for 18 of 66 entries); code vendored in a skipped directory; a manifest that does not parse (named in a note).

use super::{conflict_marker::SKIPPED, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use toml::Value;

const ID: &str = crate::rule_id::PATENT_INDEX_MATCH;

const INDEX: &str = include_str!("../../../data/patent-tiers.json");
const PIN: &str = include_str!("../../../data/patent-tiers.PIN");

#[derive(Deserialize)]
struct Index {
    techniques: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    technique: String,
    tier: String,
    license: String,
    action: String,
    note: String,
    #[serde(rename = "match")]
    names: Names,
}

#[derive(Deserialize, Default)]
struct Names {
    cargo: Vec<String>,
    pypi: Vec<String>,
    npm: Vec<String>,
    #[serde(default)]
    maven: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Eco {
    Cargo,
    Pypi,
    Npm,
    Maven,
}

/// `(registry, normalised name)` to the entries listing it, each with the extras it requires.
type ByName = BTreeMap<(Eco, String), Vec<(usize, Vec<String>)>>;

/// The loaded index: a lookup from `(registry, normalised name)` to the entries that list it, each with the extras it requires
/// (`ray[rllib]` is `ray` with the extra `rllib`).
struct Loaded {
    entries: Vec<Entry>,
    by_name: ByName,
    version: String,
    commit: String,
}

fn pin_field(key: &str) -> String {
    PIN.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
        })
        .unwrap_or_default()
}

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let index: Index = serde_json::from_str(INDEX).expect("data/patent-tiers.json parses");
        let mut by_name: ByName = BTreeMap::new();
        for (i, entry) in index.techniques.iter().enumerate() {
            for (eco, names) in [
                (Eco::Cargo, &entry.names.cargo),
                (Eco::Pypi, &entry.names.pypi),
                (Eco::Npm, &entry.names.npm),
                (Eco::Maven, &entry.names.maven),
            ] {
                for name in names {
                    let (base, extras) = split_extras(name);
                    by_name
                        .entry((eco, normalise(eco, &base)))
                        .or_default()
                        .push((i, extras));
                }
            }
        }
        Loaded {
            entries: index.techniques,
            by_name,
            version: pin_field("version"),
            commit: pin_field("commit"),
        }
    })
}

/// Registry name normalisation: PEP 503 for PyPI, hyphen and underscore equivalent for crates, lowercase for all.
fn normalise(eco: Eco, name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match eco {
        Eco::Pypi => {
            let mut out = String::new();
            let mut sep = false;
            for c in lower.chars() {
                if matches!(c, '-' | '_' | '.') {
                    sep = true;
                } else {
                    if sep && !out.is_empty() {
                        out.push('-');
                    }
                    sep = false;
                    out.push(c);
                }
            }
            out
        }
        Eco::Cargo => lower.replace('_', "-"),
        Eco::Npm | Eco::Maven => lower,
    }
}

/// `ray[rllib, tune]` into `("ray", ["rllib", "tune"])`.
fn split_extras(spec: &str) -> (String, Vec<String>) {
    match spec.split_once('[') {
        Some((base, rest)) => {
            let extras = rest
                .split(']')
                .next()
                .unwrap_or("")
                .split(',')
                .map(|e| normalise(Eco::Pypi, e))
                .filter(|e| !e.is_empty())
                .collect();
            (base.to_string(), extras)
        }
        None => (spec.to_string(), Vec::new()),
    }
}

/// The package name and extras at the start of a PEP 508 requirement (`Foo_Bar[x]>=1; python_version<"3.9"`), or `None` for a
/// line that is not a requirement (an option, a path, a URL).
fn pep508(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty()
        || line.starts_with('-')
        || line.contains("://")
        || line.starts_with('.')
        || line.starts_with('/')
    {
        return None;
    }
    let end = line
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(line.len());
    let name = &line[..end];
    if name.is_empty() {
        return None;
    }
    let extras = split_extras(&line[..line.find(']').map_or(end, |i| i + 1).max(end)]).1;
    Some((name.to_string(), extras))
}

/// The trimmed text of the first `<tag>…</tag>` in `block`.
fn xml_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let from = block.find(&open)? + open.len();
    let len = block[from..].find(&format!("</{tag}>"))?;
    Some(block[from..from + len].trim())
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s))
}

fn severity(action: &str) -> Option<Severity> {
    match action {
        "avoid" => Some(Severity::High),
        "flag-for-review" => Some(Severity::Medium),
        "allow-with-conditions" | "separate-component" => Some(Severity::Low),
        _ => None,
    }
}

struct Scan<'a> {
    view: &'a RepoView,
    findings: Vec<Finding>,
    notes: Vec<String>,
    manifests: usize,
    declared: usize,
    matched: usize,
    /// Every `(registry, normalised name)` a manifest declares: a lockfile entry in this set is reported from the manifest, not again.
    direct: BTreeSet<(Eco, String)>,
    /// Lockfiles read, and the packages they list.
    locks: usize,
    locked: usize,
    /// Locked packages already reported, so one package in two lockfiles (or at two versions) is one finding.
    reported_locked: BTreeSet<(Eco, String)>,
}

impl Scan<'_> {
    /// One declared dependency `name` (with the extras it asks for) in `file`.
    fn dependency(&mut self, eco: Eco, file: &str, text: &str, name: &str, extras: &[String]) {
        self.declared += 1;
        self.direct.insert((eco, normalise(eco, name)));
        self.check(eco, file, text, name, extras, None);
    }

    /// One package `name` at `version` that a lockfile resolves. Silent when a manifest declares it (that finding is the direct one).
    fn locked_package(&mut self, eco: Eco, file: &str, text: &str, name: &str, version: &str) {
        self.locked += 1;
        let key = (eco, normalise(eco, name));
        if self.direct.contains(&key) || !self.reported_locked.insert(key) {
            return;
        }
        self.check(eco, file, text, name, &[], Some(version));
    }

    /// Compares `name` with the index. `locked` is `Some(version)` for a lockfile entry: graded one step lower, at low confidence.
    fn check(
        &mut self,
        eco: Eco,
        file: &str,
        text: &str,
        name: &str,
        extras: &[String],
        locked: Option<&str>,
    ) {
        let index = loaded();
        let Some(hits) = index.by_name.get(&(eco, normalise(eco, name))) else {
            return;
        };
        for (i, required) in hits {
            if !required.iter().all(|r| extras.contains(r)) {
                continue;
            }
            self.matched += 1;
            let entry = &index.entries[*i];
            let Some(mut severity) = severity(&entry.action) else {
                continue;
            };
            if locked.is_some() {
                severity = match severity {
                    Severity::High => Severity::Medium,
                    other => other.min(Severity::Low),
                };
            }
            let quoted = format!("\"{name}\"");
            let line = text
                .lines()
                .position(|l| l.contains(&quoted))
                .or_else(|| text.lines().position(|l| l.contains(name)))
                .and_then(|n| u32::try_from(n + 1).ok());
            let (what, confidence, lock_note) = match locked {
                Some(version) => (
                    format!("locked package `{name}` {version} (declared by no manifest read, so probably transitive)"),
                    Confidence::Low,
                    " It is in a lockfile and no manifest in this repository declares it, so you may not choose it; the severity is one step below the index's action.",
                ),
                None => (format!("dependency `{name}`"), Confidence::Medium, ""),
            };
            self.findings.push(
                Finding::new(
                    ID,
                    severity,
                    confidence,
                    self.view.name(),
                    format!(
                        "{file}: {what} is an index match for `{}` ({}; tier {}, action {}, licence {})",
                        entry.id, entry.technique, entry.tier, entry.action, entry.license
                    ),
                    format!(
                        "{} Index match only, from agent-safety-bed {} ({}): not legal advice, not a patent clearance or a finding of infringement, \
                         and a dependency with no match is not cleared.{lock_note} Follow the index's action for `{}`, or waive this with an `[[allow]]` \
                         entry (file = the manifest or lockfile, symbol = the package) that records who reviewed it.",
                        entry.note,
                        index.version,
                        &index.commit[..index.commit.len().min(12)],
                        entry.id
                    ),
                )
                .location(Location::new(file, line))
                .subject(name),
            );
        }
    }

    fn cargo_manifest(&mut self, file: &str, text: &str) {
        let Ok(root) = text.parse::<toml::Table>() else {
            self.notes
                .push(format!("{file} does not parse as TOML, so it was not read"));
            return;
        };
        self.manifests += 1;
        let mut tables: Vec<&toml::Table> = Vec::new();
        for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
            tables.extend(root.get(key).and_then(Value::as_table));
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
                for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                    tables.extend(cfg.get(key).and_then(Value::as_table));
                }
            }
        }
        for table in tables {
            for (key, spec) in table {
                // `foo = { package = "real-name", .. }` is the crate `real-name`
                let name = spec
                    .as_table()
                    .and_then(|t| t.get("package"))
                    .and_then(Value::as_str)
                    .unwrap_or(key);
                self.dependency(Eco::Cargo, file, text, name, &[]);
            }
        }
    }

    fn package_json(&mut self, file: &str, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            self.notes
                .push(format!("{file} does not parse as JSON, so it was not read"));
            return;
        };
        self.manifests += 1;
        for table in [
            "dependencies",
            "devDependencies",
            "optionalDependencies",
            "peerDependencies",
        ] {
            let Some(deps) = json.get(table).and_then(|d| d.as_object()) else {
                continue;
            };
            for (key, spec) in deps {
                // `"x": "npm:real-name@^1"` is the package `real-name`
                let name =
                    spec.as_str()
                        .and_then(|s| s.strip_prefix("npm:"))
                        .map_or(key.as_str(), |s| match s.rfind('@') {
                            Some(at) if at > 0 => &s[..at],
                            _ => s,
                        });
                self.dependency(Eco::Npm, file, text, name, &[]);
            }
        }
    }

    fn requirement(&mut self, file: &str, text: &str, line: &str) {
        if let Some((name, extras)) = pep508(line) {
            self.dependency(Eco::Pypi, file, text, &name, &extras);
        }
    }

    /// `pom.xml`: the `groupId:artifactId` of every `<dependency>` and `<plugin>` block (a property placeholder is read as written).
    fn pom(&mut self, file: &str, text: &str) {
        self.manifests += 1;
        let mut rest = text;
        while let Some(start) = rest.find("<dependency>") {
            let after = &rest[start + "<dependency>".len()..];
            let Some(end) = after.find("</dependency>") else {
                self.notes.push(format!(
                    "{file} has an unclosed <dependency>, so the rest was not read"
                ));
                return;
            };
            let block = &after[..end];
            if let (Some(g), Some(a)) = (xml_text(block, "groupId"), xml_text(block, "artifactId"))
            {
                self.dependency(Eco::Maven, file, text, &format!("{g}:{a}"), &[]);
            }
            rest = &after[end..];
        }
    }

    /// `build.gradle` and `build.gradle.kts`: every quoted `group:artifact[:version]` coordinate.
    fn gradle(&mut self, file: &str, text: &str) {
        self.manifests += 1;
        for quote in ['"', '\''] {
            for (i, piece) in text.split(quote).enumerate() {
                if i % 2 == 0 {
                    continue;
                }
                let mut parts = piece.split(':');
                let (Some(g), Some(a)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let coordinate = |s: &str| {
                    !s.is_empty()
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                };
                if coordinate(g) && coordinate(a) && parts.count() <= 2 {
                    self.dependency(Eco::Maven, file, text, &format!("{g}:{a}"), &[]);
                }
            }
        }
    }

    fn pyproject(&mut self, file: &str, text: &str) {
        let Ok(root) = text.parse::<toml::Table>() else {
            self.notes
                .push(format!("{file} does not parse as TOML, so it was not read"));
            return;
        };
        self.manifests += 1;
        let strings = |v: Option<&Value>| -> Vec<String> {
            v.and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut lines = Vec::new();
        let project = root.get("project");
        lines.extend(strings(project.and_then(|p| p.get("dependencies"))));
        if let Some(optional) = project
            .and_then(|p| p.get("optional-dependencies"))
            .and_then(Value::as_table)
        {
            for group in optional.values() {
                lines.extend(strings(Some(group)));
            }
        }
        if let Some(groups) = root.get("dependency-groups").and_then(Value::as_table) {
            for group in groups.values() {
                lines.extend(strings(Some(group)));
            }
        }
        for line in &lines {
            self.requirement(file, text, line);
        }
        // Poetry: the key is the package name
        if let Some(poetry) = root.get("tool").and_then(|t| t.get("poetry")) {
            let mut tables: Vec<&toml::Table> = Vec::new();
            for key in ["dependencies", "dev-dependencies"] {
                tables.extend(poetry.get(key).and_then(Value::as_table));
            }
            if let Some(groups) = poetry.get("group").and_then(Value::as_table) {
                for g in groups.values() {
                    tables.extend(g.get("dependencies").and_then(Value::as_table));
                }
            }
            for table in tables {
                for (name, spec) in table.iter().filter(|(n, _)| n.as_str() != "python") {
                    let extras: Vec<String> = spec
                        .as_table()
                        .and_then(|t| t.get("extras"))
                        .map(|e| {
                            strings(Some(e))
                                .iter()
                                .map(|x| normalise(Eco::Pypi, x))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.dependency(Eco::Pypi, file, text, name, &extras);
                }
            }
        }
    }
}

impl Scan<'_> {
    /// A Gradle version catalog (`*.versions.toml`): each `[libraries]` entry is `"group:artifact:version"`, `{ module = "group:artifact" }`
    /// or `{ group = "..", name = ".." }`.
    fn version_catalog(&mut self, file: &str, text: &str) {
        let Ok(root) = text.parse::<toml::Table>() else {
            self.notes
                .push(format!("{file} does not parse as TOML, so it was not read"));
            return;
        };
        self.manifests += 1;
        let Some(libraries) = root.get("libraries").and_then(Value::as_table) else {
            return;
        };
        for spec in libraries.values() {
            let coordinate = match spec {
                Value::String(s) => s.split(':').take(2).collect::<Vec<_>>().join(":"),
                Value::Table(t) => match (t.get("module"), t.get("group"), t.get("name")) {
                    (Some(Value::String(m)), _, _) => m.clone(),
                    (_, Some(Value::String(g)), Some(Value::String(n))) => format!("{g}:{n}"),
                    _ => continue,
                },
                _ => continue,
            };
            self.dependency(Eco::Maven, file, text, &coordinate, &[]);
        }
    }

    /// `Cargo.lock`, `poetry.lock` and `uv.lock`: `[[package]]` tables with a `name` and `version`. A Cargo package with no `source` is
    /// a workspace or path crate (the repository's own code), and a uv package whose source is `virtual` or `editable` is the project itself.
    fn toml_lock(&mut self, eco: Eco, file: &str, text: &str) {
        let Ok(root) = text.parse::<toml::Table>() else {
            self.notes
                .push(format!("{file} does not parse as TOML, so it was not read"));
            return;
        };
        self.locks += 1;
        let Some(packages) = root.get("package").and_then(Value::as_array) else {
            return;
        };
        for package in packages.iter().filter_map(Value::as_table) {
            let Some(name) = package.get("name").and_then(Value::as_str) else {
                continue;
            };
            let source = package.get("source");
            let own = match eco {
                Eco::Cargo => source.is_none(),
                _ => source
                    .and_then(Value::as_table)
                    .is_some_and(|s| s.contains_key("virtual") || s.contains_key("editable")),
            };
            if own {
                continue;
            }
            let version = package
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("?");
            self.locked_package(eco, file, text, name, version);
        }
    }

    /// `package-lock.json`: lockfileVersion 2 and 3 key `packages` by `node_modules/<name>` path (the last `node_modules/` segment is the
    /// name; an entry's own `name` is an alias target); version 1 nests `dependencies` by name.
    fn package_lock(&mut self, file: &str, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            self.notes
                .push(format!("{file} does not parse as JSON, so it was not read"));
            return;
        };
        self.locks += 1;
        let version_of = |v: &serde_json::Value| {
            v.get("version")
                .and_then(|x| x.as_str())
                .unwrap_or("?")
                .to_string()
        };
        if let Some(packages) = json.get("packages").and_then(|p| p.as_object()) {
            for (path, entry) in packages.iter().filter(|(p, _)| !p.is_empty()) {
                // a workspace member is linked from outside `node_modules`: the repository's own code
                if entry.get("link").and_then(|l| l.as_bool()) == Some(true)
                    || !path.contains("node_modules/")
                {
                    continue;
                }
                let name = entry
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_else(|| path.rsplit("node_modules/").next().unwrap_or(path));
                self.locked_package(Eco::Npm, file, text, name, &version_of(entry));
            }
        } else if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
            let mut stack: Vec<(&String, &serde_json::Value)> = deps.iter().collect();
            while let Some((name, entry)) = stack.pop() {
                self.locked_package(Eco::Npm, file, text, name, &version_of(entry));
                if let Some(nested) = entry.get("dependencies").and_then(|d| d.as_object()) {
                    stack.extend(nested.iter());
                }
            }
        }
    }

    /// `yarn.lock`, classic (`"name@^1", name@^2:` then `  version "1.2.3"`) and Berry (`"name@npm:^1":` then `  version: 1.2.3`). A header lists the
    /// ranges that resolved to one version; the name is the same in each, except for an `npm:` alias, which names the real package.
    fn yarn_lock(&mut self, file: &str, text: &str) {
        self.locks += 1;
        let mut name: Option<String> = None;
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if !line.starts_with(' ') {
                name = line
                    .trim_end_matches(':')
                    .split(',')
                    .next()
                    .and_then(|spec| yarn_spec_name(spec.trim().trim_matches('"')));
                continue;
            }
            let Some(rest) = line.trim().strip_prefix("version") else {
                continue;
            };
            let version = rest.trim_start_matches(':').trim().trim_matches('"');
            if let Some(name) = name.take() {
                self.locked_package(Eco::Npm, file, text, &name, version);
            }
        }
    }

    /// `pnpm-lock.yaml`: the keys of `packages` are `/name/1.0.0` (version 5), `/name@1.0.0` (6) or `name@1.0.0` (9), with an optional
    /// `(peer@2)` suffix; `snapshots` repeats them in 9 and is not read.
    fn pnpm_lock(&mut self, file: &str, text: &str) {
        let Ok(doc) = yaml_serde::from_str::<yaml_serde::Value>(text) else {
            self.notes
                .push(format!("{file} does not parse as YAML, so it was not read"));
            return;
        };
        self.locks += 1;
        let Some(packages) = doc.get("packages").and_then(|p| p.as_mapping()) else {
            return;
        };
        for key in packages.keys().filter_map(|k| k.as_str()) {
            if let Some((name, version)) = pnpm_key(key) {
                self.locked_package(Eco::Npm, file, text, &name, &version);
            }
        }
    }

    /// `gradle.lockfile`: `group:artifact:version=configuration,..` per line; `#` comments and the `empty=` line are skipped.
    fn gradle_lockfile(&mut self, file: &str, text: &str) {
        self.locks += 1;
        for line in text.lines().map(str::trim) {
            let coordinate = line.split('=').next().unwrap_or("");
            if line.starts_with('#') || line.starts_with("empty=") {
                continue;
            }
            let mut parts = coordinate.split(':');
            if let (Some(g), Some(a), Some(v)) = (parts.next(), parts.next(), parts.next()) {
                if !g.is_empty() && !a.is_empty() {
                    self.locked_package(Eco::Maven, file, text, &format!("{g}:{a}"), v);
                }
            }
        }
    }

    /// `Pipfile.lock`: `default` and `develop` map a package name to `{ "version": "==x" }`.
    fn pipfile_lock(&mut self, file: &str, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            self.notes
                .push(format!("{file} does not parse as JSON, so it was not read"));
            return;
        };
        self.locks += 1;
        for section in ["default", "develop"] {
            let Some(packages) = json.get(section).and_then(|s| s.as_object()) else {
                continue;
            };
            for (name, entry) in packages {
                let version = entry
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .trim_start_matches('=');
                self.locked_package(Eco::Pypi, file, text, name, version);
            }
        }
    }
}

/// What a yarn or pnpm range starts with when it points at the repository's own code or at something other than the registry.
const LOCAL_RANGES: &[&str] = &[
    "workspace:",
    "link:",
    "file:",
    "portal:",
    "exec:",
    "http:",
    "https:",
    "git+",
    "git:",
    "github:",
];

/// The package a yarn lock header spec names: `@scope/name@^1` is `@scope/name`; `alias@npm:real@^1` is `real`; `name@npm:^1` is `name`.
fn yarn_spec_name(spec: &str) -> Option<String> {
    let at = spec.get(1..)?.find('@')? + 1;
    let (name, range) = (&spec[..at], &spec[at + 1..]);
    // the repository's own code (a workspace member, a linked or file dependency, a generated one) is not a registry package
    if LOCAL_RANGES.iter().any(|p| range.starts_with(p)) {
        return None;
    }
    let target = match range.strip_prefix("npm:") {
        // `npm:real@^1`: the real name ends at the `@` that starts the range (not at the scope's own leading `@`)
        Some(real) if real.get(1..).is_some_and(|r| r.contains('@')) => {
            let end = real.get(1..)?.find('@')? + 1;
            &real[..end]
        }
        // `npm:real`, a bare alias with no range: the whole rest is the real name; `npm:^1` (a range) leaves the name
        Some(real)
            if real.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '@')
                && !real.starts_with(|c: char| c.is_ascii_digit()) =>
        {
            real
        }
        _ => name,
    };
    (!target.is_empty()).then(|| target.to_string())
}

/// `(name, version)` of a pnpm `packages` key (see [`Scan::pnpm_lock`]); `None` for a key that is neither shape.
fn pnpm_key(key: &str) -> Option<(String, String)> {
    let key = key.trim_start_matches('/');
    let key = key.split('(').next().unwrap_or(key);
    // version 5: `name/1.0.0`, the version possibly followed by a `_peer@1` suffix; told from `@scope/name@1.0.0` by a version-looking tail
    if let Some((name, tail)) = key.rsplit_once('/') {
        let version = tail.split(['_', '@']).next().unwrap_or("");
        if !name.is_empty()
            && version.starts_with(|c: char| c.is_ascii_digit())
            && version.contains('.')
        {
            return Some((name.to_string(), version.to_string()));
        }
    }
    if let Some(at) = key.get(1..).and_then(|k| k.find('@')) {
        let at = at + 1;
        let version = &key[at + 1..];
        // a version, not a path or a tarball address (`file:../x`, `link:`, `https://..`)
        if !version.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        return Some((key[..at].to_string(), version.to_string()));
    }
    None
}

fn is_requirements(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".txt")
        && (name.starts_with("requirements")
            || path.contains("/requirements/")
            || path.starts_with("requirements/"))
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let mut scan = Scan {
        view,
        findings: Vec::new(),
        notes: Vec::new(),
        manifests: 0,
        declared: 0,
        matched: 0,
        direct: BTreeSet::new(),
        locks: 0,
        locked: 0,
        reported_locked: BTreeSet::new(),
    };
    // Lockfiles are read after every manifest, so a locked package a manifest declares is known to be direct.
    let mut lockfiles: Vec<&String> = Vec::new();
    let paths = view.paths();
    for path in paths.iter().filter(|p| !skipped(p)) {
        let name = path.rsplit('/').next().unwrap_or(path);
        if matches!(
            name,
            "Cargo.lock"
                | "package-lock.json"
                | "poetry.lock"
                | "uv.lock"
                | "Pipfile.lock"
                | "yarn.lock"
                | "pnpm-lock.yaml"
                | "gradle.lockfile"
        ) {
            lockfiles.push(path);
            continue;
        }
        if !matches!(
            name,
            "Cargo.toml"
                | "package.json"
                | "pyproject.toml"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
        ) && !is_requirements(path)
            && !name.ends_with(".versions.toml")
        {
            continue;
        }
        let Some(text) = view.read(path) else {
            continue;
        };
        match name {
            "Cargo.toml" => scan.cargo_manifest(path, &text),
            "package.json" => scan.package_json(path, &text),
            "pyproject.toml" => scan.pyproject(path, &text),
            "pom.xml" => scan.pom(path, &text),
            "build.gradle" | "build.gradle.kts" => scan.gradle(path, &text),
            n if n.ends_with(".versions.toml") => scan.version_catalog(path, &text),
            _ => {
                scan.manifests += 1;
                for line in text.lines() {
                    scan.requirement(path, &text, line);
                }
            }
        }
    }
    for path in lockfiles {
        let name = path.rsplit('/').next().unwrap_or(path);
        let Some(text) = view.read(path) else {
            continue;
        };
        match name {
            "Cargo.lock" => scan.toml_lock(Eco::Cargo, path, &text),
            "poetry.lock" | "uv.lock" => scan.toml_lock(Eco::Pypi, path, &text),
            "package-lock.json" => scan.package_lock(path, &text),
            "yarn.lock" => scan.yarn_lock(path, &text),
            "pnpm-lock.yaml" => scan.pnpm_lock(path, &text),
            "gradle.lockfile" => scan.gradle_lockfile(path, &text),
            _ => scan.pipfile_lock(path, &text),
        }
    }
    if scan.manifests == 0 && scan.locks == 0 && scan.notes.is_empty() {
        return Verdict::NotApplicable(
            "no Cargo.toml, package.json, pyproject.toml, pom.xml, Gradle build file, version catalog, requirements file or lockfile outside vendored and fixture directories".into(),
        );
    }
    let index = loaded();
    let positive = format!(
        "{} manifest(s) read, {} declared dependencies compared with {} index entries (agent-safety-bed {}), {} matched; {} lockfile(s) read, {} locked package(s) compared",
        scan.manifests,
        scan.declared,
        index.entries.len(),
        index.version,
        scan.matched,
        scan.locks,
        scan.locked
    );
    let mut notes = scan.notes;
    notes.push(positive.clone());
    notes.push("an index match is not a patent opinion, and no match is not a clearance".into());
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
    use sha2::{Digest, Sha256};

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    fn pairs(f: &[Finding]) -> Vec<(Severity, String)> {
        let mut v: Vec<(Severity, String)> = f
            .iter()
            .map(|f| {
                (
                    f.severity,
                    f.subject.clone().unwrap_or_default().to_lowercase(),
                )
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn the_pinned_index_matches_its_pin() {
        // the pin is over the bytes the repository stores (LF); a Windows checkout may have converted them to CRLF
        let stored = INDEX.replace("\r\n", "\n");
        let digest: String = Sha256::digest(stored.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            digest,
            pin_field("sha256"),
            "data/patent-tiers.json and data/patent-tiers.PIN disagree"
        );
        assert!(!pin_field("version").is_empty() && pin_field("commit").len() == 40);
        assert_eq!(
            loaded().entries.len(),
            66,
            "positive control: the index has 66 entries"
        );
    }

    #[test]
    fn a_dependency_on_an_avoid_entry_is_high_and_an_unlisted_one_is_not_reported() {
        let found = run(&[(
            "requirements.txt",
            "requests==2.0\ngensim>=4  # word2vec\nGensim\n",
        )]);
        assert_eq!(
            pairs(&found),
            [
                (Severity::High, "gensim".to_string()),
                (Severity::High, "gensim".to_string())
            ]
        );
        let clean = run(&[("requirements.txt", "requests==2.0\nnumpy\n")]);
        assert!(clean.is_empty(), "{clean:?}");
    }

    #[test]
    fn the_action_sets_the_severity_and_allow_is_silent() {
        // gensim avoid; openhands-ai flag-for-review; z3-solver allow-with-conditions; aider-chat allow
        let found = run(&[(
            "requirements.txt",
            "gensim\nopenhands-ai\nz3-solver\naider-chat\n",
        )]);
        assert_eq!(
            pairs(&found),
            [
                (Severity::Low, "z3-solver".to_string()),
                (Severity::Medium, "openhands-ai".to_string()),
                (Severity::High, "gensim".to_string())
            ]
        );
    }

    #[test]
    fn the_finding_says_index_match_and_never_claims_clearance() {
        let found = run(&[("requirements.txt", "gensim\n")]);
        let f = &found[0];
        assert!(f.summary.contains("index match"), "{}", f.summary);
        assert!(f.detail.contains("not a patent clearance"), "{}", f.detail);
        assert!(
            f.detail.contains("Google US 9,037,464"),
            "carries the entry's note: {}",
            f.detail
        );
        assert!(f.detail.contains(&loaded().version));
    }

    #[test]
    fn cargo_names_match_by_hyphen_underscore_and_by_package_rename() {
        let m = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nllama_cpp_2 = \"0.1\"\nz = { package = \"z3-sys\", version = \"0.1\" }\nserde = \"1\"\n\n[dev-dependencies]\ncadical = \"0.1\"\n";
        let found = run(&[("Cargo.toml", m)]);
        // Z3 is allow-with-conditions (Low); llama.cpp and CaDiCaL are flag-for-review (Medium) since index 1.4.0 marked entries
        // "reported, not verified" (they were allow-with-conditions and allow in 1.2.0)
        assert_eq!(
            pairs(&found),
            [
                (Severity::Low, "z3-sys".to_string()),
                (Severity::Medium, "cadical".to_string()),
                (Severity::Medium, "llama_cpp_2".to_string())
            ]
        );
    }

    #[test]
    fn npm_scoped_names_and_aliases_match() {
        let j = r#"{"dependencies":{"@google/adk":"^1","left-pad":"1","zz":"npm:z3-solver@^5"}}"#;
        let found = run(&[("package.json", j)]);
        // @google/adk is allow (silent); the alias is the real package z3-solver (Low)
        assert_eq!(pairs(&found), [(Severity::Low, "z3-solver".to_string())]);
    }

    #[test]
    fn pyproject_pep621_optional_groups_and_poetry_are_read() {
        let p = "[project]\nname = \"a\"\ndependencies = [\"requests\", \"Gensim>=4\"]\n[project.optional-dependencies]\nx = [\"openhands_ai\"]\n[dependency-groups]\ndev = [\"z3-solver\"]\n[tool.poetry.dependencies]\npython = \"^3.11\"\nstable-baselines3 = \"*\"\n";
        let found = run(&[("pyproject.toml", p)]);
        assert_eq!(
            pairs(&found),
            [
                (Severity::Low, "z3-solver".to_string()),
                (Severity::Medium, "openhands_ai".to_string()),
                (Severity::Medium, "stable-baselines3".to_string()),
                (Severity::High, "gensim".to_string()),
            ]
        );
    }

    #[test]
    fn an_extra_is_required_when_the_index_names_one() {
        // the index lists `ray[rllib]`, not `ray`
        let plain = run(&[("requirements.txt", "ray>=2\n")]);
        assert!(plain.is_empty(), "{plain:?}");
        let with = run(&[("requirements.txt", "ray[rllib]>=2\n")]);
        assert_eq!(with.len(), 1, "{with:?}");
        let with_many = run(&[("requirements.txt", "ray[tune, rllib]\n")]);
        assert_eq!(with_many.len(), 1, "{with_many:?}");
    }

    #[test]
    fn requirement_lines_that_are_not_names_are_skipped() {
        let found = run(&[(
            "requirements.txt",
            "-r other.txt\n-e .\n./local\nhttps://x/y.whl\n# gensim\n\n",
        )]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn maven_pom_and_gradle_coordinates_match() {
        let pom = "<project><dependencies>
<dependency>
<groupId>org.drools</groupId>
<artifactId>drools-core</artifactId>
</dependency>
                   <dependency><groupId>junit</groupId><artifactId>junit</artifactId></dependency>
</dependencies></project>";
        let gradle = "dependencies {
  implementation(\"org.apache.jena:jena-arq:5.0.0\")
  testImplementation 'junit:junit:4.13'
}";
        let f = run(&[("pom.xml", pom), ("app/build.gradle.kts", gradle)]);
        assert_eq!(
            pairs(&f),
            vec![
                (Severity::Low, "org.apache.jena:jena-arq".to_string()),
                (Severity::Low, "org.drools:drools-core".to_string()),
            ]
        );
    }

    #[test]
    fn a_gradle_version_catalog_is_read_in_all_three_library_forms() {
        let catalog = "[versions]\ndrools = \"9\"\n[libraries]\na = \"org.drools:drools-core:9.0\"\nb = { module = \"org.apache.jena:jena-arq\", version.ref = \"drools\" }\nc = { group = \"org.drools\", name = \"drools-core\" }\nd = { module = \"junit:junit\" }\n";
        let f = run(&[("gradle/libs.versions.toml", catalog)]);
        assert_eq!(
            pairs(&f),
            vec![
                (Severity::Low, "org.apache.jena:jena-arq".to_string()),
                (Severity::Low, "org.drools:drools-core".to_string()),
                (Severity::Low, "org.drools:drools-core".to_string()),
            ]
        );
    }

    #[test]
    fn a_locked_package_no_manifest_declares_is_one_step_lower_and_low_confidence() {
        // gensim is `avoid` (High) when declared; only in a lockfile it is Medium. Cargo.lock: cadical is flag-for-review (Medium), Low when only locked.
        let poetry = "[[package]]\nname = \"Gensim\"\nversion = \"4.3.0\"\n\n[[package]]\nname = \"requests\"\nversion = \"2.33.0\"\n";
        let cargo = "[[package]]\nname = \"cadical\"\nversion = \"0.1.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let f = run(&[("poetry.lock", poetry), ("Cargo.lock", cargo)]);
        assert_eq!(
            pairs(&f),
            vec![
                (Severity::Low, "cadical".to_string()),
                (Severity::Medium, "gensim".to_string())
            ]
        );
        assert!(f.iter().all(|f| f.confidence == Confidence::Low), "{f:?}");
        let g = f.iter().find(|f| f.summary.contains("Gensim")).unwrap();
        assert!(g.summary.contains("locked package"), "{}", g.summary);
        assert!(g.summary.contains("4.3.0"), "{}", g.summary);
        assert_eq!(g.location.as_ref().and_then(|l| l.line), Some(2));
    }

    #[test]
    fn a_package_a_manifest_declares_is_reported_once_from_the_manifest() {
        let f = run(&[
            ("requirements.txt", "gensim>=4\n"),
            (
                "poetry.lock",
                "[[package]]\nname = \"gensim\"\nversion = \"4.3.0\"\n",
            ),
        ]);
        assert_eq!(pairs(&f), vec![(Severity::High, "gensim".to_string())]);
        assert!(
            f[0].summary.contains("requirements.txt"),
            "{}",
            f[0].summary
        );
        // the same package in two lockfiles is one finding
        let g = run(&[
            (
                "a/poetry.lock",
                "[[package]]\nname = \"gensim\"\nversion = \"4.3.0\"\n",
            ),
            (
                "b/uv.lock",
                "[[package]]\nname = \"gensim\"\nversion = \"4.2.0\"\n",
            ),
        ]);
        assert_eq!(g.len(), 1, "{g:?}");
    }

    #[test]
    fn the_projects_own_packages_in_a_lockfile_are_not_judged() {
        // a Cargo package with no `source` is a workspace or path crate; a uv package with a virtual or editable source is the project
        let cargo = "[[package]]\nname = \"cadical\"\nversion = \"0.1.0\"\n";
        let uv = "[[package]]\nname = \"gensim\"\nversion = \"0.1.0\"\nsource = { virtual = \".\" }\n\n[[package]]\nname = \"z3-solver\"\nversion = \"1\"\nsource = { editable = \".\" }\n";
        match judge(&RepoView::from_files(&[
            ("Cargo.lock", cargo),
            ("uv.lock", uv),
        ])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty(), "{f:?}");
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("2 lockfile(s) read, 0 locked package(s) compared")),
                    "{notes:?}"
                );
            }
            _ => panic!("lockfiles alone are readable"),
        }
    }

    #[test]
    fn package_lock_v3_v1_and_pipfile_lock_are_read() {
        let v3 = r#"{"lockfileVersion":3,"packages":{"":{"name":"app"},"node_modules/left-pad":{"version":"1.0.0"},"node_modules/a/node_modules/z3-solver":{"version":"5.0.0"},"node_modules/zz":{"name":"@google/adk","version":"1.0.0"},"packages/app":{"version":"1"},"node_modules/ws":{"link":true,"resolved":"ws"}}}"#;
        let v1 = r#"{"lockfileVersion":1,"dependencies":{"left-pad":{"version":"1.0.0","dependencies":{"z3-solver":{"version":"5.0.0"}}}}}"#;
        let pipfile = r#"{"default":{"gensim":{"version":"==4.3.0"}},"develop":{"requests":{"version":"==2.33.0"}}}"#;
        let a = run(&[("package-lock.json", v3)]);
        assert_eq!(pairs(&a), vec![(Severity::Low, "z3-solver".to_string())]);
        assert!(a[0].summary.contains("5.0.0"), "{}", a[0].summary);
        let b = run(&[("web/package-lock.json", v1)]);
        assert_eq!(pairs(&b), vec![(Severity::Low, "z3-solver".to_string())]);
        let c = run(&[("Pipfile.lock", pipfile)]);
        assert_eq!(pairs(&c), vec![(Severity::Medium, "gensim".to_string())]);
        assert!(
            c[0].summary.contains("4.3.0") && !c[0].summary.contains("=="),
            "{}",
            c[0].summary
        );
    }

    #[test]
    fn an_unparseable_lockfile_is_a_note_not_a_clean_run() {
        match judge(&RepoView::from_files(&[("Cargo.lock", "[[package\n")])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(
                    notes
                        .iter()
                        .any(|n| n.contains("Cargo.lock does not parse")),
                    "{notes:?}"
                );
            }
            _ => panic!("an unparseable lockfile must be noted"),
        }
    }

    #[test]
    fn an_unparseable_manifest_is_a_note_not_a_clean_run() {
        match judge(&RepoView::from_files(&[("Cargo.toml", "[dependencies\n")])) {
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
    fn no_manifest_is_not_applicable_and_fixture_directories_are_skipped() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "gensim")])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            judge(&RepoView::from_files(&[(
                "tests/fixtures/requirements.txt",
                "gensim\n"
            )])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn yarn_lock_classic_berry_scoped_and_aliased_entries_are_read() {
        // z3-solver is an npm name in the index with a reported action; @google/adk is `allow`, so it is silent
        let classic = "# yarn lockfile v1

\"z3-solver@^5.0.0\", \"z3-solver@^5.1.0\":
  version \"5.2.0\"
  resolved \"x\"

left-pad@^1.0.0:
  version \"1.0.0\"
";
        let berry = "__metadata:
  version: 8

\"z3-solver@npm:^5.0.0\":
  version: 5.1.0
  resolution: \"z3-solver@npm:5.1.0\"

\"alias@npm:z3-solver@^5\":
  version: 5.1.0
";
        let a = run(&[("yarn.lock", classic)]);
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(
            a[0].summary.contains("z3-solver") && a[0].summary.contains("5.2.0"),
            "{}",
            a[0].summary
        );
        let b = run(&[("web/yarn.lock", berry)]);
        // the alias names the same real package as a second entry, so one finding per package
        assert_eq!(b.len(), 1, "{b:?}");
        assert_eq!(
            yarn_spec_name("left-pad@^1.0.0").as_deref(),
            Some("left-pad")
        );
        assert_eq!(yarn_spec_name("@a/b@npm:^1").as_deref(), Some("@a/b"));
        assert_eq!(yarn_spec_name("x@npm:@a/b@^1").as_deref(), Some("@a/b"));
    }

    #[test]
    fn yarn_headers_that_are_the_repositorys_own_code_or_a_bare_alias_are_told_apart() {
        // a workspace, link, file, portal or exec descriptor is not a registry package, whatever its name
        for own in [
            "z3-solver@workspace:packages/z3-solver",
            "z3-solver@link:../z3-solver",
            "z3-solver@file:../z3-solver",
            "z3-solver@portal:../z3-solver",
            "z3-solver@exec:./gen.js",
        ] {
            assert_eq!(yarn_spec_name(own), None, "{own}");
        }
        // a bare alias (no range after the real name) names the real package
        assert_eq!(
            yarn_spec_name("alias@npm:z3-solver").as_deref(),
            Some("z3-solver")
        );
        assert_eq!(yarn_spec_name("alias@npm:@a/b").as_deref(), Some("@a/b"));
        // end to end: a Berry workspace entry whose name is an index name reports nothing
        let berry = "__metadata:
  version: 8

\"z3-solver@workspace:packages/z3-solver\":
  version: 0.0.0-use.local
";
        assert!(run(&[("yarn.lock", berry)]).is_empty());
    }

    #[test]
    fn pnpm_keys_that_are_local_paths_or_tarballs_are_not_registry_packages() {
        for own in [
            "z3-solver@file:../z3-solver",
            "z3-solver@link:../z3-solver",
            "/z3-solver@file:../z3-solver(react@17.0.0)",
            "z3-solver@https://example.com/z3-solver-5.0.0.tgz",
        ] {
            assert_eq!(pnpm_key(own), None, "{own}");
        }
    }

    #[test]
    fn pnpm_lock_versions_5_6_and_9_keys_are_read() {
        assert_eq!(
            pnpm_key("/left-pad/1.0.0"),
            Some(("left-pad".into(), "1.0.0".into()))
        );
        assert_eq!(
            pnpm_key("/@google/adk/1.2.0_react@17.0.0"),
            Some(("@google/adk".into(), "1.2.0".into()))
        );
        assert_eq!(
            pnpm_key("/@google/adk@1.2.0(react@17.0.0)"),
            Some(("@google/adk".into(), "1.2.0".into()))
        );
        assert_eq!(
            pnpm_key("z3-solver@5.0.0"),
            Some(("z3-solver".into(), "5.0.0".into()))
        );
        assert_eq!(
            pnpm_key("@scope/3d@1.0.0"),
            Some(("@scope/3d".into(), "1.0.0".into()))
        );
        let v9 = "lockfileVersion: '9.0'
packages:
  z3-solver@5.0.0:
    resolution: {integrity: x}
  left-pad@1.0.0:
    resolution: {integrity: y}
snapshots:
  gensim@4.3.0: {}
";
        let f = run(&[("pnpm-lock.yaml", v9)]);
        // only `packages` is read: gensim is an index name but sits in `snapshots`, and left-pad is not in the index
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("z3-solver"), "{}", f[0].summary);
    }

    #[test]
    fn gradle_lockfile_coordinates_are_read_and_comments_skipped() {
        let lock = "# This is a Gradle generated file for dependency locking.
# Manual edits can break the build.
org.drools:drools-core:9.0.0=compileClasspath,runtimeClasspath
junit:junit:4.13=testCompileClasspath
empty=annotationProcessor
";
        let f = run(&[("app/gradle.lockfile", lock)]);
        assert_eq!(
            pairs(&f),
            vec![(Severity::Low, "org.drools:drools-core".to_string())]
        );
        assert!(f[0].confidence == Confidence::Low);
    }

    #[test]
    fn every_name_in_the_index_is_claimed_once() {
        let index = loaded();
        let total: usize = index
            .entries
            .iter()
            .map(|e| {
                e.names.cargo.len() + e.names.pypi.len() + e.names.npm.len() + e.names.maven.len()
            })
            .sum();
        let keys: usize = index.by_name.values().map(Vec::len).sum();
        assert_eq!(total, keys);
        assert!(
            total >= 50,
            "positive control: the index lists {total} cargo, pypi and npm names"
        );
        for ((_, name), hits) in &index.by_name {
            assert_eq!(hits.len(), 1, "{name} is claimed by more than one entry");
        }
    }
}
