//! Conformance: how a module earns its claim to cover a rule (multi-language design section 6.2).
//!
//! A claim to check a rule is only worth what proves it. For each rule a module claims, the fixtures directory holds a small
//! project with a seeded defect and a clean twin:
//!
//! ```text
//! <fixtures>/<rule>/defective/   a project that breaks the rule, plus expect.toml listing where it is reported
//! <fixtures>/<rule>/clean/       the same project without the defect, plus an expect.toml that expects nothing
//! ```
//!
//! `expect.toml` says what a run must report, so a defect without a line (a missing file, a CI setting) can be expressed:
//!
//! ```toml
//! needs_network = false      # optional: the rule needs the network; unproven unless the run allows it
//! [[expect]]
//! rule = "version-consistency"
//! file = "c/Cargo.toml"
//! line = 3                   # optional
//! ```
//!
//! [`run`] copies each fixture into a throwaway git repository (the checks read `HEAD`), asks the module for that one rule, and
//! judges the answer: every expectation must be met, nothing else may be reported in the defective project, and nothing at all
//! may be reported in the clean one. A rule that passes is [`Verdict::Proven`]. A rule that was claimed but has no complete pair
//! is [`Verdict::NoFixture`], and one that needs the network when the run does not allow it is [`Verdict::Unproven`]: neither is
//! coverage. A defect with no clean twin proves nothing, because a check that always reports would pass it.
//!
//! **Fixtures are code that runs.** The built-in checks build the project they are given, so a fixture's build script or
//! procedural macro executes with the caller's privileges and network. Run conformance only over fixtures you wrote or trust (a
//! module's own CI over its own repository); there is no sandbox here. The runner does refuse a fixture that carries its own `.git`
//! directory or a symbolic link, and it clears the git environment variables that would point its repository at the caller's.

use crate::check::{CheckContext, Tier};
use crate::module::{reconcile, Limits, Module, Request, RuleStatus};
use anyhow::{bail, Context};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// How one rule's claim stands after the fixtures have run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The seeded defect was reported where `expect.toml` says, and the clean twin reported nothing.
    Proven,
    /// A fixture contradicted the claim; each reason names the fixture and what was wrong.
    Failed(Vec<String>),
    /// The rule could not run on a fixture (the module errored, skipped it, or sent no verdict). That is not a contradiction of the
    /// claim, and nothing was proven either: the run is incomplete, like a check that cannot run.
    Errored(Vec<String>),
    /// Not judged, for the reason given (the fixture needs the network and the run did not allow it).
    Unproven(String),
    /// There is no complete pair (a defective project and a clean twin) for the rule.
    NoFixture,
}

/// One claimed rule and its verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RuleProof {
    /// The rule's id.
    pub rule: String,
    /// Whether its claim is earned.
    pub verdict: Verdict,
}

/// What [`run`] found, one entry per claimed rule (a rule the module marks not applicable is not judged).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Conformance {
    /// The rules, in the order the module claims them.
    pub rules: Vec<RuleProof>,
}

impl Conformance {
    /// The rules whose claim is earned.
    pub fn proven(&self) -> Vec<&str> {
        self.rules
            .iter()
            .filter(|r| r.verdict == Verdict::Proven)
            .map(|r| r.rule.as_str())
            .collect()
    }

    /// Whether any rule could not be run on its fixtures.
    pub fn any_errored(&self) -> bool {
        self.rules
            .iter()
            .any(|r| matches!(r.verdict, Verdict::Errored(_)))
    }

    /// Whether any fixture contradicted a claim.
    pub fn any_failed(&self) -> bool {
        self.rules
            .iter()
            .any(|r| matches!(r.verdict, Verdict::Failed(_)))
    }
}

/// What to run: where the fixtures are, which rule, and whether the network is allowed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    fixtures: PathBuf,
    rule: Option<String>,
    network: bool,
}

impl Options {
    /// Run every claimed rule against the fixtures in `fixtures`, without the network.
    pub fn new(fixtures: impl Into<PathBuf>) -> Self {
        Self {
            fixtures: fixtures.into(),
            rule: None,
            network: false,
        }
    }

    /// Judge only this rule.
    pub fn rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    /// Allow fixtures that need the network to run.
    pub fn network(mut self, allowed: bool) -> Self {
        self.network = allowed;
        self
    }

    /// The run tier the fixtures are checked at: `Fast` unless the network is allowed.
    pub fn tier(&self) -> Tier {
        if self.network {
            Tier::Sweep
        } else {
            Tier::Fast
        }
    }
}

/// One finding a fixture must produce.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    rule: String,
    file: String,
    #[serde(default)]
    line: Option<u32>,
}

/// The parsed `expect.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectFile {
    #[serde(default)]
    needs_network: bool,
    #[serde(default)]
    expect: Vec<Expect>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Defective,
    Clean,
}

impl Kind {
    fn dir(self) -> &'static str {
        match self {
            Kind::Defective => "defective",
            Kind::Clean => "clean",
        }
    }
}

/// Runs the fixtures for the rules `module` claims and judges each claim.
///
/// # Errors
/// If the module's hello is unusable, `options` names a rule the module does not claim, or a fixture's `expect.toml` is
/// malformed or inconsistent (a defective project that expects nothing, a clean twin that expects something, an expectation
/// for another rule, a missing `expect.toml`, a `.git` directory or a symbolic link inside a fixture): that is an authoring
/// error in the fixtures, so it stops the whole run rather than hiding behind one rule's verdict. A fixture that fails is not an
/// error: it is a [`Verdict::Failed`]. `Options::tier` is only the run tier handed to the module; the built-in module runs
/// exactly the rule it is asked for whatever the tier.
pub fn run(module: &dyn Module, options: &Options) -> anyhow::Result<Conformance> {
    let hello = module.describe()?;
    if let Some(problem) = hello.problem() {
        bail!("the module's hello: {problem}");
    }
    if let Some(bad) = hello.rules.iter().find(|r| {
        !matches!(
            r.status.as_str(),
            "implemented-native" | "delegated" | "not-applicable"
        )
    }) {
        bail!(
            "the module claims the rule \"{}\" with the status \"{}\", which the protocol does not define",
            bad.id,
            bad.status
        );
    }
    let claimed: Vec<&str> = hello
        .rules
        .iter()
        .filter(|r| r.status != "not-applicable")
        .map(|r| r.id.as_str())
        .collect();
    if let Some(wanted) = &options.rule {
        if !claimed.contains(&wanted.as_str()) {
            bail!(
                "the module does not claim the rule \"{wanted}\" (it claims: {})",
                claimed.join(", ")
            );
        }
    }
    let mut rules = Vec::new();
    for rule in claimed {
        if options.rule.as_deref().is_some_and(|wanted| wanted != rule) {
            continue;
        }
        let verdict = judge(module, options, rule)?;
        rules.push(RuleProof {
            rule: rule.to_string(),
            verdict,
        });
    }
    Ok(Conformance { rules })
}

fn judge(module: &dyn Module, options: &Options, rule: &str) -> anyhow::Result<Verdict> {
    let base = options.fixtures.join(rule);
    let defective = base.join(Kind::Defective.dir());
    let clean = base.join(Kind::Clean.dir());
    for dir in [&defective, &clean] {
        reject_symlink(dir)?;
    }
    if !defective.is_dir() || !clean.is_dir() {
        return Ok(Verdict::NoFixture);
    }
    let defective_expect = read_expect(&defective, rule, Kind::Defective)?;
    let clean_expect = read_expect(&clean, rule, Kind::Clean)?;
    if (defective_expect.needs_network || clean_expect.needs_network) && !options.network {
        return Ok(Verdict::Unproven(
            "its fixture needs the network, and this run does not allow it".into(),
        ));
    }
    let (mut reasons, mut errors) = (Vec::new(), Vec::new());
    for (dir, expect, kind) in [
        (&defective, &defective_expect, Kind::Defective),
        (&clean, &clean_expect, Kind::Clean),
    ] {
        let (contradictions, problems) = judge_one(module, options, rule, dir, expect, kind)?;
        reasons.extend(contradictions);
        errors.extend(problems);
    }
    Ok(if !errors.is_empty() {
        Verdict::Errored(errors)
    } else if reasons.is_empty() {
        Verdict::Proven
    } else {
        Verdict::Failed(reasons)
    })
}

/// A fixture is read from inside its own directory only: a link could lead anywhere (or to a device that never ends).
fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!(
            "{}: a fixture must not be, or contain, a symbolic link",
            path.display()
        );
    }
    Ok(())
}

fn read_expect(dir: &Path, rule: &str, kind: Kind) -> anyhow::Result<ExpectFile> {
    let path = dir.join("expect.toml");
    reject_symlink(&path)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("{}: cannot read the fixture's expectations", path.display()))?;
    let parsed: ExpectFile =
        toml::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    match kind {
        Kind::Defective if parsed.expect.is_empty() => bail!(
            "{}: a defective fixture must expect at least one finding",
            path.display()
        ),
        Kind::Clean if !parsed.expect.is_empty() => {
            bail!("{}: a clean twin must expect nothing", path.display())
        }
        _ => {}
    }
    if let Some(other) = parsed.expect.iter().find(|e| e.rule != rule) {
        bail!(
            "{}: expects a finding for \"{}\" but the fixture is for \"{rule}\"",
            path.display(),
            other.rule
        );
    }
    Ok(parsed)
}

/// Runs one fixture and returns the reasons it contradicts the claim (empty when it does not).
fn judge_one(
    module: &dyn Module,
    options: &Options,
    rule: &str,
    dir: &Path,
    expect: &ExpectFile,
    kind: Kind,
) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let name = format!("{rule}/{}", kind.dir());
    let project = tempfile::tempdir()?;
    copy_fixture(dir, project.path())?;
    git_commit_all(project.path())
        .with_context(|| format!("{name}: could not make the fixture a git repository"))?;
    let ctx = CheckContext::new(project.path().to_path_buf());
    let request = Request::new(
        &ctx,
        None,
        options.tier(),
        vec![rule.to_string()],
        Limits::new(1800, 64 << 20),
    );
    let reconciled = reconcile(&request, module.check(&request));
    let mut reasons: Vec<String> = Vec::new();
    let mut errors: Vec<String> = reconciled
        .run_errors
        .iter()
        .map(|e| format!("{name}: the run had a problem: {e}"))
        .collect();
    let Some(outcome) = reconciled.rules.into_iter().find(|o| o.rule == rule) else {
        errors.push(format!("{name}: the module gave no verdict for the rule"));
        return Ok((reasons, errors));
    };
    match outcome.status {
        RuleStatus::Ran => {}
        RuleStatus::Skipped { detail } => {
            errors.push(format!("{name}: the rule was skipped, not run: {detail}"));
            return Ok((reasons, errors));
        }
        RuleStatus::Error { kind: why, detail } => {
            errors.push(format!(
                "{name}: the rule could not run ({why:?}): {detail}"
            ));
            return Ok((reasons, errors));
        }
    }
    let mut found: Vec<(String, Option<u32>)> = Vec::new();
    for f in &outcome.findings {
        if f.check_id != rule {
            reasons.push(format!(
                "{name}: a finding came back under another rule, \"{}\", and does not count",
                f.check_id
            ));
            continue;
        }
        found.push(
            f.location
                .as_ref()
                .map_or((String::new(), None), |l| (l.file.clone(), l.line)),
        );
    }
    match kind {
        Kind::Clean => {
            for (file, line) in &found {
                reasons.push(format!(
                    "{name}: the clean twin was reported at {}",
                    place(file, *line)
                ));
            }
        }
        Kind::Defective => {
            // Each expectation is met by exactly one finding, and every finding must meet one: a duplicate, or a second
            // finding in the same file, is not explained by a single expectation.
            let assigned = assign(&expect.expect, &found);
            for (wanted, hit) in expect.expect.iter().zip(&assigned) {
                if hit.is_none() {
                    reasons.push(format!(
                        "{name}: the seeded defect was not reported at {}",
                        place(&wanted.file, wanted.line)
                    ));
                }
            }
            for (i, (file, line)) in found.iter().enumerate() {
                if !assigned.contains(&Some(i)) {
                    reasons.push(format!(
                        "{name}: a finding at {} is not in expect.toml",
                        place(file, *line)
                    ));
                }
            }
        }
    }
    Ok((reasons, errors))
}

/// Pairs each expectation with a distinct finding that satisfies it, maximising the pairs (augmenting paths), so a broad
/// expectation listed first cannot take the finding a narrower one needs. `result[i]` is the finding index for expectation `i`.
fn assign(expect: &[Expect], found: &[(String, Option<u32>)]) -> Vec<Option<usize>> {
    fn augment(
        e: usize,
        expect: &[Expect],
        found: &[(String, Option<u32>)],
        owner: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for (f, (file, line)) in found.iter().enumerate() {
            if seen[f] || !matches(&expect[e], file, *line) {
                continue;
            }
            seen[f] = true;
            let free = match owner[f] {
                None => true,
                Some(other) => augment(other, expect, found, owner, seen),
            };
            if free {
                owner[f] = Some(e);
                return true;
            }
        }
        false
    }
    let mut owner: Vec<Option<usize>> = vec![None; found.len()];
    for e in 0..expect.len() {
        let mut seen = vec![false; found.len()];
        augment(e, expect, found, &mut owner, &mut seen);
    }
    let mut result = vec![None; expect.len()];
    for (f, e) in owner.iter().enumerate() {
        if let Some(e) = e {
            result[*e] = Some(f);
        }
    }
    result
}

fn matches(wanted: &Expect, file: &str, line: Option<u32>) -> bool {
    wanted.file == file && wanted.line.is_none_or(|l| Some(l) == line)
}

fn place(file: &str, line: Option<u32>) -> String {
    match (file.is_empty(), line) {
        (true, _) => "(no location)".into(),
        (false, None) => file.to_string(),
        (false, Some(n)) => format!("{file}:{n}"),
    }
}

/// Copies a fixture into `to`, leaving out its `expect.toml` (the module under test must not see its own answer key).
fn copy_fixture(from: &Path, to: &Path) -> anyhow::Result<()> {
    fn copy(from: &Path, to: &Path, top: bool) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let name = entry.file_name();
            if top && name == "expect.toml" {
                continue;
            }
            let target = to.join(&name);
            if name == ".git" {
                bail!(
                    "{}: a fixture must not carry a .git directory (the runner makes its own repository)",
                    from.display()
                );
            }
            if entry.file_type()?.is_symlink() {
                bail!(
                    "{}: a fixture must not contain a symbolic link",
                    entry.path().display()
                );
            }
            if entry.file_type()?.is_dir() {
                std::fs::create_dir_all(&target)?;
                copy(&entry.path(), &target, false)?;
            } else {
                std::fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    }
    copy(from, to, true)
}

/// `git` in `dir` with the variables that redirect a repository cleared: a caller inside a git hook has them set, and inherited
/// they would aim the fixture's `init`, `add` and `commit` at the caller's own repository.
pub(crate) fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(var);
    }
    command
}

fn git_commit_all(dir: &Path) -> anyhow::Result<()> {
    let git = |args: &[&str]| -> anyhow::Result<()> {
        let output = git_command(dir).args(args).output()?;
        if !output.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    };
    git(&["init", "-q"])?;
    // `-f`: the fixture is committed as written, whatever the caller's own ignore rules say.
    git(&["add", "-A", "-f"])?;
    git(&[
        "-c",
        "user.email=conformance@coderipper.invalid",
        "-c",
        "user.name=conformance",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "-m",
        "fixture",
    ])
}

#[cfg(test)]
mod tests {
    use super::git_command;
    use std::collections::HashSet;

    #[test]
    fn git_runs_with_no_inherited_repository_redirection() {
        // A caller inside a git hook has GIT_DIR and friends set; inherited, they would point the fixture's `git init`, `add` and
        // `commit` at the caller's own repository.
        let command = git_command(std::path::Path::new("."));
        let cleared: HashSet<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_COMMON_DIR",
        ] {
            assert!(
                cleared.contains(var),
                "{var} must be cleared, cleared: {cleared:?}"
            );
        }
    }
}
