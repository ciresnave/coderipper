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
//! **What it cannot see:** a Gradle version catalog (`libs.versions.toml`) or a coordinate built from variables; lockfiles and transitive
//! dependencies; a technique implemented in the project's own source or copied in; a package the index lists under no name
//! (`match` is empty for 18 of 66 entries); code vendored in a skipped directory; a manifest that does not parse (named in a note).

use super::{conflict_marker::SKIPPED, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use serde::Deserialize;
use std::collections::BTreeMap;
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
}

impl Scan<'_> {
    /// One declared dependency `name` (with the extras it asks for) in `file`.
    fn dependency(&mut self, eco: Eco, file: &str, text: &str, name: &str, extras: &[String]) {
        self.declared += 1;
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
            let Some(severity) = severity(&entry.action) else {
                continue;
            };
            let line = text
                .lines()
                .position(|l| l.contains(name))
                .and_then(|n| u32::try_from(n + 1).ok());
            self.findings.push(
                Finding::new(
                    ID,
                    severity,
                    Confidence::Medium,
                    self.view.name(),
                    format!(
                        "{file}: dependency `{name}` is an index match for `{}` ({}; tier {}, action {}, licence {})",
                        entry.id, entry.technique, entry.tier, entry.action, entry.license
                    ),
                    format!(
                        "{} Index match only, from agent-safety-bed {} ({}): not legal advice, not a patent clearance or a finding of infringement, \
                         and a dependency with no match is not cleared. Follow the index's action for `{}`, or waive this with an `[[allow]]` \
                         entry (file = the manifest, symbol = the package) that records who reviewed it.",
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
    };
    for path in view.paths().iter().filter(|p| !skipped(p)) {
        let name = path.rsplit('/').next().unwrap_or(path);
        if !matches!(
            name,
            "Cargo.toml"
                | "package.json"
                | "pyproject.toml"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
        ) && !is_requirements(path)
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
            _ => {
                scan.manifests += 1;
                for line in text.lines() {
                    scan.requirement(path, &text, line);
                }
            }
        }
    }
    if scan.manifests == 0 && scan.notes.is_empty() {
        return Verdict::NotApplicable(
            "no Cargo.toml, package.json, pyproject.toml, pom.xml, Gradle build file or requirements file outside vendored and fixture directories".into(),
        );
    }
    let index = loaded();
    let positive = format!(
        "{} manifest(s) read, {} declared dependencies compared with {} index entries (agent-safety-bed {}), {} matched",
        scan.manifests,
        scan.declared,
        index.entries.len(),
        index.version,
        scan.matched
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
