//! Version-consistency check (project scope, local): do all of a project's packages share one version?
//!
//! CireSnave's standing rule: every crate in a project carries the same version number, so a consumer
//! can tell which ones go together. One narrow exception: a crate that exists to work with a version of
//! ANOTHER project (an emitter crate tracking the project it targets) keeps that project's version
//! instead. Mechanism, and nothing else: read every package's version with `cargo metadata`, decide
//! which version the project is at, and report each package that differs.
//!
//! - **The project's version** is `[workspace.package] version` when the workspace root defines one;
//!   otherwise the version most packages are at (a tie goes to the highest version).
//! - **A tracking exception** is declared in `.coderipper.toml` as a `[[tracks]]` entry: the package
//!   is left out of the uniformity comparison and is instead compared with the version in the other
//!   project's CURRENT manifest. An unreadable reference is an error, never a pass.
//! - A package may also be silenced with an ordinary `[[allow]]` entry (check `version-consistency`,
//!   file = its manifest relative to the workspace root, symbol = the package name), which the host
//!   applies and reports when it goes stale.
//!
//! Reads the working tree, not HEAD: this is a rule about what is about to be released.
//! Rust (cargo) manifests only.

use crate::allowlist::Allowlist;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::package::{metadata, Metadata};
use std::collections::BTreeMap;
use std::path::Path;

pub const CHECK_ID: &str = "version-consistency";

pub struct VersionConsistencyCheck;

impl Check for VersionConsistencyCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
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
        let packages: Vec<Pkg> = meta.packages_for_comparison(&workspace_root);

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let mut tracked: BTreeMap<String, Tracked> = BTreeMap::new();
        for entry in allowlist.tracks() {
            anyhow::ensure!(
                packages.iter().any(|p| p.name == entry.package),
                "[[tracks]] names package `{}`, which is not a package of this project (members: {})",
                entry.package,
                packages
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let manifest = ctx.project_root.join(&entry.manifest);
            tracked.insert(
                entry.package.clone(),
                Tracked {
                    version: version_in_manifest(&manifest)?,
                    manifest: entry.manifest.clone(),
                    reason: entry.reason.clone(),
                },
            );
        }

        let workspace_version = workspace_package_version(&workspace_root)?;
        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(evaluate(&packages, workspace_version.as_deref(), &tracked)
            .into_iter()
            .map(|p| p.into_finding(&project))
            .collect())
    }
}

/// One package as this check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pkg {
    pub name: String,
    pub version: String,
    /// Manifest path relative to the workspace root, forward slashes.
    pub manifest: String,
}

/// What a `[[tracks]]` entry resolved to: the version of the referenced manifest, read just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tracked {
    pub version: String,
    pub manifest: String,
    /// Why the exception exists, as the `[[tracks]]` entry says.
    pub reason: String,
}

impl Metadata {
    fn packages_for_comparison(&self, workspace_root: &Path) -> Vec<Pkg> {
        let mut out: Vec<Pkg> = self
            .packages
            .iter()
            .map(|p| Pkg {
                name: p.name.clone(),
                version: p.version.clone(),
                manifest: p
                    .manifest_path
                    .canonicalize()
                    .ok()
                    .and_then(|m| {
                        m.strip_prefix(workspace_root)
                            .ok()
                            .map(|r| r.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
                    })
                    .unwrap_or_else(|| p.manifest_path.to_string_lossy().replace('\\', "/")),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
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

/// The version a manifest declares: `[package] version`, or, when that is `{ workspace = true }`
/// (or the manifest is a virtual workspace root), `[workspace.package] version`.
pub(crate) fn version_in_manifest(path: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {} to find its version: {e}", path.display()))?;
    let table: toml::Table = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", path.display()))?;
    let direct = table
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str());
    let inherited = table
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str());
    direct
        .or(inherited)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{} declares no version", path.display()))
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
            if pkg.version != t.version {
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
        Some(v) => (v.to_string(), "`[workspace.package] version`".to_string()),
        None if uniform.len() < 2 => return problems,
        None => {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for p in &uniform {
                *counts.entry(p.version.as_str()).or_default() += 1;
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
        if pkg.version != expected {
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

/// `major.minor.patch[-pre]` ordering; anything unparsable sorts below everything parsable, and ties
/// fall back to plain text order so the result is deterministic.
pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(v: &str) -> Option<(u64, u64, u64, bool)> {
        let (core, pre) = match v.split_once('-') {
            Some((c, _)) => (c, true),
            None => (v.split_once('+').map_or(v, |(c, _)| c), false),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??, !pre))
    }
    match (key(a), key(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.cmp(b),
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
                let at_expected = all.iter().filter(|(_, v)| *v == expected).count();
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
                    "{} was read successfully and declares version {}",
                    tracked.manifest, tracked.version
                )),
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
