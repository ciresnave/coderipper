//! [`NeutralModule`]: the language-neutral rules, built in. They read the repository's tracked files (manifests, lockfiles, CI
//! workflows, documents), never a parser's output, so one implementation serves every language.
//!
//! The module answers the same protocol as [`super::RustModule`], in process. Every rule id it claims is a catalog id
//! (`SUP-001`), and each claim carries the conformance fixture that earns it (`conformance/<rule>/`, run by this repository's own
//! tests). What each rule judges is stated in its own file, with what it cannot see: these are narrow, deterministic readings
//! of broader rules, and the module says so rather than claiming the whole rule.
//!
//! Files are read from `HEAD` (`git ls-tree`, `git show`), as the Rust checks analyse `HEAD`, so a dirty working tree changes
//! nothing. A project that is not a git repository with a commit is an error for every rule, never a clean run.

mod doc005;
mod doc009;
mod sup001;
mod sup011;
mod wsp001;

use super::{
    Capabilities, ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleClaim,
    RuleRan, RuleResult,
};
use crate::finding::Finding;
#[cfg(test)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a rule concluded about the repository it was shown.
pub(crate) enum Verdict {
    /// It ran; these are its findings (none means the rule is clean).
    Findings(Vec<Finding>),
    /// The rule does not apply to this repository (the reason is shown). A coverage gap, never "clean".
    NotApplicable(String),
}

/// One rule: its catalog id and the function that judges a repository.
struct Rule {
    id: &'static str,
    judge: fn(&RepoView) -> Verdict,
}

const RULES: &[Rule] = &[
    Rule {
        id: "SUP-001",
        judge: sup001::judge,
    },
    Rule {
        id: "SUP-011",
        judge: sup011::judge,
    },
    Rule {
        id: "DOC-005",
        judge: doc005::judge,
    },
    Rule {
        id: "DOC-009",
        judge: doc009::judge,
    },
    Rule {
        id: "WSP-001",
        judge: wsp001::judge,
    },
];

/// The rules this module can check, in the order it reports them.
pub fn rule_ids() -> Vec<&'static str> {
    RULES.iter().map(|r| r.id).collect()
}

/// Where a [`RepoView`] gets its files.
enum Source {
    /// `HEAD` of the git repository at this directory.
    Git(PathBuf),
    /// Files held in memory (unit tests).
    #[cfg(test)]
    Memory(BTreeMap<String, String>),
}

/// The tracked files of a repository at `HEAD`: the list of paths, and their contents on demand.
pub(crate) struct RepoView {
    name: String,
    files: Vec<String>,
    source: Source,
}

impl RepoView {
    /// The repository at `root`, as committed at `HEAD`.
    pub(crate) fn from_git(root: &Path) -> anyhow::Result<Self> {
        let output = crate::conformance::git_command(root)
            .args(["ls-tree", "-r", "-z", "--name-only", "HEAD"])
            .output()
            .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
        if !output.status.success() {
            anyhow::bail!(
                "{} is not a git repository with a commit (the rules read HEAD): {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect();
        files.sort();
        Ok(Self {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            files,
            source: Source::Git(root.to_path_buf()),
        })
    }

    /// A repository made of these `(path, contents)` pairs.
    #[cfg(test)]
    pub(crate) fn from_files(files: &[(&str, &str)]) -> Self {
        let map: BTreeMap<String, String> = files
            .iter()
            .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
            .collect();
        Self {
            name: "repo".into(),
            files: map.keys().cloned().collect(),
            source: Source::Memory(map),
        }
    }

    /// The project's directory name (what a finding's `project` says).
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Every tracked path, sorted, with forward slashes.
    pub(crate) fn paths(&self) -> &[String] {
        &self.files
    }

    /// Whether `path` is tracked.
    pub(crate) fn has(&self, path: &str) -> bool {
        self.files
            .binary_search_by(|f| f.as_str().cmp(path))
            .is_ok()
    }

    /// The text of a tracked file (invalid UTF-8 is replaced); `None` when it is not tracked or cannot be read.
    pub(crate) fn read(&self, path: &str) -> Option<String> {
        if !self.has(path) {
            return None;
        }
        match &self.source {
            #[cfg(test)]
            Source::Memory(map) => map.get(path).cloned(),
            Source::Git(root) => {
                let output = crate::conformance::git_command(root)
                    .arg("show")
                    // `./`: relative to the project directory, which may be a subdirectory of the repository
                    .arg(format!("HEAD:./{path}"))
                    .output()
                    .ok()?;
                output.status.success().then(|| {
                    let text = String::from_utf8_lossy(&output.stdout);
                    text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
                })
            }
        }
    }
}

/// The directory part of a tracked path (`""` for a file at the root).
pub(crate) fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file-name part of a path.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `dir` and each of its ancestors, nearest first, ending with the root (`""`).
pub(crate) fn dir_and_ancestors(dir: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut current = dir;
    loop {
        found.push(current.to_string());
        if current.is_empty() {
            return found;
        }
        current = dir_of(current);
    }
}

/// `dir/name`, or just `name` for the root.
pub(crate) fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The language-neutral rules, answering the module protocol in process.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeutralModule;

impl NeutralModule {
    /// The module.
    pub fn new() -> Self {
        Self
    }
}

/// The rules whose claim is earned: each has a seeded-defect fixture and a clean twin under `conformance/`
/// (`tests/neutral_module.rs` fails if this list and the fixtures disagree).
/// Rules whose check reads the files of some languages only (the coverage report shows the others as gaps). `SUP-001` reads
/// Cargo manifests and lockfiles (`rust`) and npm-family manifests and lockfiles (`typescript`, which stands for the
/// JavaScript family); Python, Go and the rest are not read.
const READS_ONLY: &[(&str, &[&str])] = &[("SUP-001", &["rust", "typescript"])];

const PROVEN: &[&str] = &["SUP-001", "SUP-011", "DOC-005", "DOC-009", "WSP-001"];

impl Module for NeutralModule {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "neutral",
            env!("CARGO_PKG_VERSION"),
            vec!["any".into()],
            vec![],
            // reads files and runs nothing of the project's
            Capabilities::new(false, false, false, false),
            RULES
                .iter()
                .map(|r| {
                    let mut claim = RuleClaim::native(r.id);
                    if let Some((_, languages)) = READS_ONLY.iter().find(|(id, _)| *id == r.id) {
                        claim = claim.only_languages(languages);
                    }
                    if PROVEN.contains(&r.id) {
                        claim.proof = Some(format!("conformance/{}", r.id));
                    }
                    claim
                })
                .collect(),
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let view = match RepoView::from_git(&request.project_root) {
            Ok(view) => view,
            Err(e) => {
                // every requested rule gets the same error, so the host sees an error per rule and never a clean run
                let mut events: Vec<Event> = request
                    .rules
                    .iter()
                    .map(|rule| {
                        Event::RuleResult(RuleResult::error(
                            rule,
                            ErrorKind::Internal,
                            e.to_string(),
                        ))
                    })
                    .collect();
                events.push(Event::Summary(ModuleSummary {
                    rules_requested: request.rules.len(),
                    rules_errored: request.rules.len(),
                    ..ModuleSummary::default()
                }));
                return ModuleOutput {
                    in_process: true,
                    ..ModuleOutput::from_events(events)
                };
            }
        };
        let mut events = Vec::new();
        let mut summary = ModuleSummary {
            rules_requested: request.rules.len(),
            ..ModuleSummary::default()
        };
        for rule in &request.rules {
            let Some(known) = RULES.iter().find(|r| r.id == rule) else {
                events.push(Event::RuleResult(RuleResult::error(
                    rule,
                    ErrorKind::Internal,
                    "the neutral module has no such rule",
                )));
                summary.rules_errored += 1;
                continue;
            };
            match (known.judge)(&view) {
                Verdict::Findings(found) => {
                    summary.rules_ran += 1;
                    summary.findings += found.len();
                    let count = found.len();
                    events.extend(found.into_iter().map(|f| Event::Finding(Box::new(f))));
                    events.push(Event::RuleResult(RuleResult::ran(rule, count)));
                }
                Verdict::NotApplicable(why) => {
                    summary.rules_skipped += 1;
                    events.push(Event::RuleResult(RuleResult {
                        status: RuleRan::Skipped,
                        reason_code: Some("not_applicable_here".into()),
                        detail: Some(why),
                        ..RuleResult::ran(rule, 0)
                    }));
                }
            }
        }
        events.push(Event::Summary(summary));
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_split_into_directory_and_name() {
        assert_eq!(dir_of("a/b/c.md"), "a/b");
        assert_eq!(dir_of("c.md"), "");
        assert_eq!(file_name("a/b/c.md"), "c.md");
        assert_eq!(join("", "x"), "x");
        assert_eq!(join("a", "x"), "a/x");
        assert_eq!(dir_and_ancestors("a/b"), vec!["a/b", "a", ""]);
        assert_eq!(dir_and_ancestors(""), vec![""]);
    }

    #[test]
    fn the_repo_view_reads_only_tracked_files() {
        let view = RepoView::from_files(&[("a.txt", "A"), ("d/b.txt", "B")]);
        assert_eq!(view.read("a.txt").as_deref(), Some("A"));
        assert_eq!(view.read("d/b.txt").as_deref(), Some("B"));
        assert_eq!(view.read("nope.txt"), None);
        assert!(view.has("a.txt") && !view.has("nope.txt"));
    }

    #[test]
    fn the_module_claims_exactly_its_rules_all_proven() {
        let hello = NeutralModule::new().describe().unwrap();
        let claimed: Vec<&str> = hello.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(claimed, rule_ids());
        assert_eq!(rule_ids().len(), 5);
        assert!(hello.rules.iter().all(|r| r.proof.is_some()));
        assert!(hello.problem().is_none());
    }
}
