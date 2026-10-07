//! Version-consistency check (project scope, local): do all of a project's packages share one version?
//!
//! CireSnave's standing rule: every crate in a project carries the same version number, so a consumer
//! can tell which ones go together. One narrow exception: a crate that exists to work with a version of
//! ANOTHER project (an emitter crate tracking the project it targets) keeps that project's version
//! instead. Mechanism, and nothing else: read every package's version with `cargo metadata`, decide
//! which version the project is at, and report each package that differs.
//!
//! - **The project is a whole workspace**, so the check is run with `--project` on the WORKSPACE ROOT
//!   (a virtual workspace root is fine). From a member directory it says so (one `Info` finding) rather
//!   than guessing: `.coderipper.toml` is read from the directory named, and a verdict that depended on
//!   which member was named would be wrong.
//! - **The project's version** is `[workspace.package] version` when the workspace root defines one (even
//!   a stale one is authoritative: that is what the workspace says it is at); otherwise the version most
//!   packages are at (a tie goes to the highest). Build metadata (`+build.5`) is not part of the number.
//! - **A package with no `version` key** (allowed since Cargo 1.75; cargo reports `0.0.0`) has no version
//!   to compare and is left out.
//! - **A tracking exception** is declared in `.coderipper.toml` as a `[[tracks]]` entry: the package is
//!   left out of the uniformity comparison and is instead compared with the other project's CURRENT
//!   version. The reference is a manifest: a package's manifest (its version as cargo resolves it,
//!   inheritance included) or a virtual workspace manifest (that workspace's one version). An
//!   unreadable reference, one with no single version, an unknown package, or two entries for one
//!   package is an error, never a pass.
//! - A package may also be silenced with an ordinary `[[allow]]` entry (check `version-consistency`,
//!   file = its manifest relative to the workspace root, symbol = the package name), which the host
//!   applies and reports when it goes stale.
//!
//! Reads the working tree, not HEAD: this is a rule about what is about to be released.
//! Rust (cargo) manifests only.

use crate::allowlist::Allowlist;
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::package::{metadata, Metadata};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = "version-consistency";

/// Reports a package whose version differs from the rest of its project (see the module docs).
pub struct VersionConsistencyCheck;

impl Check for VersionConsistencyCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    /// It reads the whole workspace's versions (from a member's directory it only says "run me on the root"), so a
    /// workspace run takes it once, not once per member.
    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let meta = metadata(&ctx.project_root)?;
        anyhow::ensure!(
            !meta.packages.is_empty(),
            "cargo metadata found no packages in {}, so there is nothing to compare",
            ctx.project_root.display()
        );
        let workspace_root = meta.workspace_root.canonicalize()?;
        let project_dir = ctx.project_root.canonicalize()?;
        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if project_dir != workspace_root {
            return Ok(vec![not_the_workspace_root(
                &project,
                &project_dir,
                &workspace_root,
            )]);
        }

        let packages = versioned_packages(&meta, &workspace_root)?;
        let allowlist = Allowlist::load(&ctx.project_root)?;
        let mut tracked: BTreeMap<String, Tracked> = BTreeMap::new();
        for entry in allowlist.tracks() {
            anyhow::ensure!(
                packages.iter().any(|p| p.name == entry.package)
                    || meta.packages.iter().any(|p| p.name == entry.package),
                "[[tracks]] names package `{}`, which is not a package of this project (members: {})",
                entry.package,
                meta.packages
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let manifest = ctx.project_root.join(&entry.manifest);
            let previous = tracked.insert(
                entry.package.clone(),
                Tracked {
                    version: reference_version(&manifest)?,
                    manifest: entry.manifest.clone(),
                    reason: entry.reason.clone(),
                },
            );
            anyhow::ensure!(
                previous.is_none(),
                "[[tracks]] names package `{}` more than once; one reference per package",
                entry.package
            );
        }

        let workspace_version = workspace_package_version(&workspace_root)?;
        Ok(evaluate(&packages, workspace_version.as_deref(), &tracked)
            .into_iter()
            .map(|p| p.into_finding(&project))
            .collect())
    }
}

/// A path as the user should read it: Windows' verbatim prefix (`\\?\C:/...`) that
/// `canonicalize` adds is noise.
fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

fn not_the_workspace_root(project: &str, project_dir: &Path, workspace_root: &Path) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        severity: Severity::Info,
        confidence: Confidence::High,
        project: project.to_string(),
        location: None,
        subject: None,
        summary: format!(
            "version-consistency compares a whole workspace; `{project}` is inside the workspace at {}, \
             so run it with --project on the workspace root",
            shown(workspace_root)
        ),
        detail: format!(
            "Every package of the workspace must share one version, and `.coderipper.toml` (the \
             [[tracks]] and [[allow]] entries) is read from the directory named, so a verdict from a \
             member's directory would depend on which member was named. {} is not the workspace root.",
            shown(project_dir)
        ),
        positive_control: Some(format!(
            "cargo metadata reports the workspace root as {}, which is not {}",
            shown(workspace_root),
            shown(project_dir)
        )),
        member: None,
    }
}

/// One package as this check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pkg {
    pub name: String,
    pub version: String,
    /// Manifest path relative to the workspace root, forward slashes (`../x/Cargo.toml` for a member
    /// outside it).
    pub manifest: String,
}

/// What a `[[tracks]]` entry resolved to: the version of the reference, read just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tracked {
    pub version: String,
    pub manifest: String,
    /// Why the exception exists, as the `[[tracks]]` entry says.
    pub reason: String,
}

/// The packages that declare a version of their own (or inherit one). A manifest with no `version`
/// key gets `0.0.0` from cargo, which would otherwise outvote the real crates.
fn versioned_packages(meta: &Metadata, workspace_root: &Path) -> anyhow::Result<Vec<Pkg>> {
    let mut out = Vec::new();
    for p in &meta.packages {
        let text = std::fs::read_to_string(&p.manifest_path)?;
        let table: toml::Table = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", p.manifest_path.display()))?;
        let declares = table
            .get("package")
            .and_then(|pkg| pkg.get("version"))
            .is_some();
        if declares {
            out.push(Pkg {
                name: p.name.clone(),
                version: p.version.clone(),
                manifest: relative_manifest(workspace_root, &p.manifest_path),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `manifest` relative to `workspace_root`, with `..` where it is outside. Both are made canonical so
/// the answer does not depend on how the path was spelled.
fn relative_manifest(workspace_root: &Path, manifest: &Path) -> String {
    let manifest = manifest
        .canonicalize()
        .unwrap_or_else(|_| manifest.to_path_buf());
    if let Ok(inside) = manifest.strip_prefix(workspace_root) {
        return forward(inside);
    }
    let root: Vec<_> = workspace_root.components().collect();
    let target: Vec<_> = manifest.components().collect();
    let common = root.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..root.len() {
        out.push("..");
    }
    for part in &target[common..] {
        out.push(part);
    }
    forward(&out)
}

/// `[workspace.package] version` of the manifest at `workspace_root`, when there is one.
fn workspace_package_version(workspace_root: &Path) -> anyhow::Result<Option<String>> {
    let text = std::fs::read_to_string(workspace_root.join("Cargo.toml"))?;
    let table: toml::Table = toml::from_str(&text)?;
    Ok(table
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// The version a `[[tracks]]` reference stands for. A package's manifest: the version cargo
/// resolves for it, `version.workspace = true` included. A virtual workspace manifest: that
/// workspace's one version (`[workspace.package] version`, or the version all its packages share).
fn reference_version(manifest: &Path) -> anyhow::Result<String> {
    anyhow::ensure!(
        manifest.is_file(),
        "cannot read {} to find its version: not a file",
        manifest.display()
    );
    let dir = manifest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no directory", manifest.display()))?;
    let meta = metadata(dir).map_err(|e| {
        anyhow::anyhow!(
            "cannot read {} to find its version: {e}",
            manifest.display()
        )
    })?;
    let wanted = manifest.canonicalize()?;
    if let Some(package) = meta
        .packages
        .iter()
        .find(|p| p.manifest_path.canonicalize().ok().as_ref() == Some(&wanted))
    {
        return Ok(package.version.clone());
    }
    // not a package's manifest: a virtual workspace, whose version is the one its packages share
    let root = meta.workspace_root.canonicalize()?;
    if let Some(v) = workspace_package_version(&root)? {
        return Ok(v);
    }
    let packages = versioned_packages(&meta, &root)?;
    let first = packages
        .first()
        .ok_or_else(|| anyhow::anyhow!("{} declares no version", manifest.display()))?;
    anyhow::ensure!(
        packages
            .iter()
            .all(|p| strip_build(&p.version) == strip_build(&first.version)),
        "{} has no single version to track: its packages are at {}",
        manifest.display(),
        packages
            .iter()
            .map(|p| format!("{} {}", p.name, p.version))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(first.version.clone())
}

/// How a version problem arose.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The package differs from the version the rest of the project is at.
    Outlier {
        pkg: Pkg,
        expected: String,
        basis: String,
        all: Vec<(String, String)>,
    },
    /// A package that tracks another project does not match that project's manifest.
    TrackMismatch { pkg: Pkg, tracked: Tracked },
}

pub(crate) fn evaluate(
    packages: &[Pkg],
    workspace_version: Option<&str>,
    tracked: &BTreeMap<String, Tracked>,
) -> Vec<Problem> {
    let mut problems = Vec::new();

    for pkg in packages {
        if let Some(t) = tracked.get(&pkg.name) {
            if strip_build(&pkg.version) != strip_build(&t.version) {
                problems.push(Problem::TrackMismatch {
                    pkg: pkg.clone(),
                    tracked: t.clone(),
                });
            }
        }
    }

    let uniform: Vec<&Pkg> = packages
        .iter()
        .filter(|p| !tracked.contains_key(&p.name))
        .collect();
    let all: Vec<(String, String)> = uniform
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();
    let (expected, basis) = match workspace_version {
        Some(v) => (
            strip_build(v).to_string(),
            "`[workspace.package] version`".to_string(),
        ),
        None if uniform.len() < 2 => return problems,
        None => {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for p in &uniform {
                *counts.entry(strip_build(&p.version)).or_default() += 1;
            }
            let best = counts
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| compare_versions(a.0, b.0)))
                .map(|(v, n)| (v.to_string(), *n))
                .expect("at least two packages");
            let basis = format!(
                "the version most packages are at ({} of {}; a tie goes to the highest)",
                best.1,
                uniform.len()
            );
            (best.0, basis)
        }
    };
    for pkg in uniform {
        if strip_build(&pkg.version) != expected {
            problems.push(Problem::Outlier {
                pkg: pkg.clone(),
                expected: expected.clone(),
                basis: basis.clone(),
                all: all.clone(),
            });
        }
    }
    problems
}

/// Build metadata (`+build.5`) is not part of a version's precedence or of "the same number".
fn strip_build(v: &str) -> &str {
    v.split_once('+').map_or(v, |(core, _)| core)
}

#[derive(PartialEq, Eq)]
enum Ident {
    Num(u64),
    Alpha(String),
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Ident::Num(a), Ident::Num(b)) => a.cmp(b),
            (Ident::Alpha(a), Ident::Alpha(b)) => a.cmp(b),
            (Ident::Num(_), Ident::Alpha(_)) => Ordering::Less,
            (Ident::Alpha(_), Ident::Num(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse(v: &str) -> Option<((u64, u64, u64), Vec<Ident>)> {
    let (core, pre) = match strip_build(v).split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (strip_build(v), None),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let numbers = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    let pre = pre
        .map(|p| {
            p.split('.')
                .map(|id| {
                    id.parse::<u64>()
                        .map_or_else(|_| Ident::Alpha(id.to_string()), Ident::Num)
                })
                .collect()
        })
        .unwrap_or_default();
    Some((numbers, pre))
}

/// Semver precedence: numeric `major.minor.patch`; a release beats its own pre-release; pre-release
/// identifiers compare one by one (numbers numerically and below words, a shorter list below a longer
/// one with the same start); build metadata is ignored. Anything unparsable sorts below everything
/// parsable, and falls back to plain text order so the result is deterministic.
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    match (parse(a), parse(b)) {
        (Some((ca, pa)), Some((cb, pb))) => {
            ca.cmp(&cb)
                .then_with(|| match (pa.is_empty(), pb.is_empty()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    (false, false) => pa.cmp(&pb),
                })
        }
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => strip_build(a).cmp(strip_build(b)),
    }
}

impl Problem {
    fn into_finding(self, project: &str) -> Finding {
        match self {
            Problem::Outlier {
                pkg,
                expected,
                basis,
                all,
            } => {
                let listing = all
                    .iter()
                    .map(|(n, v)| format!("{n} {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let at_expected = all
                    .iter()
                    .filter(|(_, v)| strip_build(v) == expected)
                    .count();
                Finding {
                    check_id: CHECK_ID.into(),
                    severity: Severity::High,
                    confidence: Confidence::High,
                    project: project.to_string(),
                    location: Some(Location {
                        file: pkg.manifest.clone(),
                        line: None,
                    }),
                    subject: Some(pkg.name.clone()),
                    summary: format!(
                        "`{}` is at version {} but this project is at {expected}",
                        pkg.name, pkg.version
                    ),
                    detail: format!(
                        "Every package of a project carries one version so a consumer knows which go \
                         together. The project's version is {basis}. Packages compared: {listing}. A \
                         package that exists to work with another project's version is declared in \
                         `.coderipper.toml` with a `[[tracks]]` entry."
                    ),
                    positive_control: Some(format!(
                        "read {} package manifest(s) with `cargo metadata`; {at_expected} of them are \
                         at {expected}, so the comparison could see agreement as well as difference",
                        all.len()
                    )),
                    member: None,
                }
            }
            Problem::TrackMismatch { pkg, tracked } => Finding {
                check_id: CHECK_ID.into(),
                severity: Severity::High,
                confidence: Confidence::High,
                project: project.to_string(),
                location: Some(Location {
                    file: pkg.manifest.clone(),
                    line: None,
                }),
                subject: Some(pkg.name.clone()),
                summary: format!(
                    "`{}` is declared to track {} but is at version {} while that manifest is at {}",
                    pkg.name, tracked.manifest, pkg.version, tracked.version
                ),
                detail: format!(
                    "`{}` is exempt from the one-version rule because it works with another \
                     project ({}), so it must match THAT project's current version, read from {} \
                     just now.",
                    pkg.name, tracked.reason, tracked.manifest
                ),
                positive_control: Some(format!(
                    "{} was read successfully and stands for version {}",
                    tracked.manifest, tracked.version
                )),
                member: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> Pkg {
        Pkg {
            name: name.into(),
            version: version.into(),
            manifest: format!("{name}/Cargo.toml"),
        }
    }

    fn outliers(problems: &[Problem]) -> Vec<&str> {
        problems
            .iter()
            .filter_map(|p| match p {
                Problem::Outlier { pkg, .. } => Some(pkg.name.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn packages_that_agree_produce_nothing() {
        let pkgs = [pkg("a", "1.2.3"), pkg("b", "1.2.3"), pkg("c", "1.2.3")];
        assert!(evaluate(&pkgs, None, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn the_minority_is_the_outlier_when_there_is_no_workspace_version() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("c", "0.2.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["c"]
        );
    }

    #[test]
    fn a_tie_goes_to_the_highest_version() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.2.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["a"]
        );
        let pkgs = [pkg("a", "0.9.0"), pkg("b", "0.10.0")];
        assert_eq!(
            outliers(&evaluate(&pkgs, None, &BTreeMap::new())),
            vec!["a"]
        );
    }

    #[test]
    fn the_workspace_package_version_is_authoritative_even_against_a_majority() {
        let pkgs = [pkg("a", "2.0.0"), pkg("b", "2.0.0"), pkg("c", "1.0.0")];
        let found = evaluate(&pkgs, Some("1.0.0"), &BTreeMap::new());
        assert_eq!(outliers(&found), vec!["a", "b"]);
    }

    #[test]
    fn a_single_package_has_nothing_to_be_consistent_with() {
        assert!(evaluate(&[pkg("only", "0.1.0")], None, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn a_tracking_package_is_compared_with_its_reference_and_left_out_of_the_majority() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("emit", "7.0.0")];
        let mut tracked = BTreeMap::new();
        tracked.insert(
            "emit".to_string(),
            Tracked {
                version: "7.0.0".into(),
                manifest: "../other/Cargo.toml".into(),
                reason: "emits for other".into(),
            },
        );
        assert!(evaluate(&pkgs, None, &tracked).is_empty());

        tracked.get_mut("emit").unwrap().version = "7.1.0".into();
        let found = evaluate(&pkgs, None, &tracked);
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0], Problem::TrackMismatch { pkg, .. } if pkg.name == "emit"));
    }

    #[test]
    fn a_windows_verbatim_path_prefix_is_not_shown_to_the_user() {
        assert_eq!(shown(Path::new(r"\\?\C:/work/ws")), "C:/work/ws");
        assert_eq!(shown(Path::new("/home/me/ws")), "/home/me/ws");
    }

    #[test]
    fn prerelease_identifiers_compare_numerically_and_build_metadata_is_ignored() {
        use std::cmp::Ordering::*;
        // Review findings: `rc.2` < `rc.10`, and `+build-5` must not be split at its own hyphen.
        assert_eq!(compare_versions("1.0.0-rc.10", "1.0.0-rc.2"), Greater);
        assert_eq!(compare_versions("1.0.0-alpha", "1.0.0-alpha.1"), Less);
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), Less);
        assert_eq!(compare_versions("1.0.0+build-5", "0.1.0"), Greater);
        assert_eq!(compare_versions("1.0.0+a", "1.0.0+b"), Equal);
    }

    #[test]
    fn the_same_version_with_different_build_metadata_is_not_an_outlier() {
        let pkgs = [pkg("a", "1.0.0"), pkg("b", "1.0.0+build.5")];
        assert!(evaluate(&pkgs, None, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn versions_order_numerically_and_a_release_beats_its_prerelease() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0-rc.1"), Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("1.0.0", "not-a-version"), Greater);
    }

    #[test]
    fn a_finding_validates_and_names_the_package_and_both_versions() {
        let pkgs = [pkg("a", "0.1.0"), pkg("b", "0.1.0"), pkg("c", "0.2.0")];
        let problem = evaluate(&pkgs, None, &BTreeMap::new()).remove(0);
        let finding = problem.into_finding("proj");
        assert_eq!(finding.subject.as_deref(), Some("c"));
        assert!(finding.summary.contains("0.2.0") && finding.summary.contains("0.1.0"));
        assert_eq!(finding.location.as_ref().unwrap().file, "c/Cargo.toml");
        assert!(finding.validate().is_ok());
    }
}
