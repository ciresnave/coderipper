//! SUP-002 (dependencies free of known advisories) and SEC-006 (no known exploitable dependency vulnerabilities), delegated to
//! [osv-scanner](https://github.com/google/osv-scanner) (Apache-2.0).
//!
//! **What this judges:** osv-scanner reads the lockfiles (and the manifests and SBOMs) it knows under the project (measured: it skips
//! anything inside a `node_modules` directory, even with `--no-ignore`; `vendor/`, `third_party/` and `target/` are read), resolves each
//! package to an exact version, and asks the OSV database (osv.dev, a network call) which advisories cover that version. It sees
//! the ecosystems osv-scanner has an extractor for, at the version the lock pins, and the database as it is *today*: a clean result
//! today can be a finding tomorrow with no change to the project. A project osv-scanner finds no package sources in is a **gap**
//! (nothing was judged), never a clean rule.
//!
//! **What is read:** osv-scanner's JSON report, written to a file in a scratch directory. Its exit code is part of its answer
//! (`0` clean, `1` advisories found, `128` no package sources found); anything else is a failure of the tool. It names a lockfile
//! it could not read only on stderr (measured: exit 127 when nothing else was found, exit 1 beside advisories), so an empty report
//! with anything on stderr is a failure too (a defensive arm: exit 0 with stderr was not reproduced), and findings read beside an
//! unreadable lockfile carry a note saying so. A partial read must not look like a clean one.
//!
//! **The repository cannot silence the rule:** osv-scanner reads an `osv-scanner.toml` in every directory it scans, whose
//! `IgnoredVulns` drop advisories, and honours `.gitignore`. Both are overridden (`--config` with an empty file, `--no-ignore`);
//! the project's way to suppress a finding is the `.coderipper.toml` allowlist, which is visible.
//!
//! **The mapping record:** one finding per advisory group (the advisories osv-scanner treats as the same flaw, by alias) of one
//! package in one lockfile, at the lockfile, subject `ecosystem:name@version`. Severity follows the group's highest CVSS score
//! (9.0 and up critical, 7.0 high, 4.0 medium, below that low); an advisory with no score is reported as `High`, not hidden.
//! Confidence is `Medium` for SUP-002 (the version matches an advisory's affected range) and `Low` for SEC-006.
//!
//! **SEC-006 is narrower than SUP-002 and not "exploitable":** it reports the groups scored 7.0 or higher (or not scored). Whether
//! the vulnerable code is *reachable* is call analysis, which osv-scanner can do only for Go and Rust and builds the project's code
//! to do (a Rust build script runs), so it is not run. A SEC-006 finding says a serious advisory covers the pinned version, not
//! that the project is exploitable.

use super::{resolve, run_tool, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The catalog rules this evidences.
pub(super) const SUP: &str = "SUP-002";
pub(super) const SEC: &str = "SEC-006";
/// The tool's name in the lock.
pub(super) const TOOL: &str = "osv-scanner";

/// The score from which SEC-006 reports an advisory (CVSS "high").
const SEC_FROM: f32 = 7.0;
/// What osv-scanner exits with besides 0: advisories found, a general error (it could not read the project, or a lockfile in it)
/// and no package sources found. All three are read with its stderr rather than refused as a crash, so the message can say why.
const EXIT_FOUND: i32 = 1;
const EXIT_ERROR: i32 = 127;
const EXIT_NO_SOURCES: i32 = 128;

#[derive(Debug, Deserialize)]
struct Report {
    results: Vec<Source>,
}

#[derive(Debug, Deserialize)]
struct Source {
    source: SourceInfo,
    #[serde(default)]
    packages: Vec<Pkg>,
}

#[derive(Debug, Deserialize)]
struct SourceInfo {
    path: String,
}

#[derive(Debug, Deserialize)]
struct Pkg {
    package: PkgInfo,
    #[serde(default)]
    groups: Vec<Group>,
    #[serde(default)]
    vulnerabilities: Vec<Vuln>,
}

#[derive(Debug, Deserialize)]
struct PkgInfo {
    name: String,
    version: String,
    #[serde(default)]
    ecosystem: String,
}

#[derive(Debug, Deserialize)]
struct Group {
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    max_severity: String,
}

/// An advisory as reported (only its id and summary are read; the body and references are not).
#[derive(Debug, Deserialize)]
struct Vuln {
    id: String,
    #[serde(default)]
    summary: Option<String>,
}

/// One advisory group on one package in one lockfile.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    /// The lockfile, as osv-scanner printed it until [`relative`] makes it the project's.
    file: String,
    name: String,
    version: String,
    ecosystem: String,
    ids: Vec<String>,
    aliases: Vec<String>,
    /// The group's highest CVSS score; `None` when osv-scanner gave none.
    score: Option<f32>,
    summaries: Vec<String>,
    /// What osv-scanner could not read elsewhere in the project, when it said so: the finding carries it, because the rest of the
    /// project was not judged.
    unread: Option<String>,
}

/// Reads osv-scanner's JSON report. A missing `results` list is unreadable; an empty one is a clean scan.
fn parse_report(text: &str) -> Result<Vec<Hit>, String> {
    let report: Report = serde_json::from_str(text).map_err(|e| {
        format!(
            "the report is not an osv-scanner result ({e}): {}",
            text.chars().take(200).collect::<String>()
        )
    })?;
    let mut hits = Vec::new();
    for source in report.results {
        for pkg in source.packages {
            for group in pkg.groups {
                let summaries = group
                    .ids
                    .iter()
                    .filter_map(|id| {
                        pkg.vulnerabilities
                            .iter()
                            .find(|v| &v.id == id)
                            .and_then(|v| v.summary.clone())
                    })
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                hits.push(Hit {
                    file: source.source.path.clone(),
                    name: pkg.package.name.clone(),
                    version: pkg.package.version.clone(),
                    ecosystem: pkg.package.ecosystem.clone(),
                    ids: group.ids,
                    aliases: group.aliases,
                    score: group
                        .max_severity
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|s| s.is_finite()),
                    summaries,
                    unread: None,
                });
            }
        }
    }
    Ok(hits)
}

/// The severity of a CVSS score. No score is not "low": an advisory the database did not score is reported as `High`.
fn severity_of(score: Option<f32>) -> Severity {
    match score {
        None => Severity::High,
        Some(s) if s >= 9.0 => Severity::Critical,
        Some(s) if s >= SEC_FROM => Severity::High,
        Some(s) if s >= 4.0 => Severity::Medium,
        Some(_) => Severity::Low,
    }
}

/// Whether `rule` reports this advisory group (see the module documentation for SEC-006's line).
fn applies(rule: &str, hit: &Hit) -> bool {
    match rule {
        SUP => true,
        SEC => hit.score.is_none_or(|s| s >= SEC_FROM),
        _ => false,
    }
}

/// Advisory text is third-party text that reaches a terminal: control characters become spaces and one line is cut short.
fn clean(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        format!("{}...", flat.chars().take(max).collect::<String>())
    } else {
        flat
    }
}

/// `items` joined, with how many more there were past `shown`.
fn list(items: &[String], shown: usize) -> String {
    let head: Vec<String> = items.iter().take(shown).map(|i| clean(i, 40)).collect();
    match items.len().saturating_sub(shown) {
        0 => head.join(", "),
        more => format!("{} and {more} more", head.join(", ")),
    }
}

/// The finding for one advisory group, under `rule`.
fn to_finding(hit: &Hit, project: &str, rule: &str) -> Finding {
    let file = hit.file.replace('\\', "/");
    let package = format!("{} {}", clean(&hit.name, 80), clean(&hit.version, 40));
    let ids = list(&hit.ids, 3);
    let confidence = if rule == SEC {
        Confidence::Low
    } else {
        Confidence::Medium
    };
    let aliases: Vec<String> = hit
        .aliases
        .iter()
        .filter(|a| !hit.ids.contains(a))
        .cloned()
        .collect();
    let mut detail = format!(
        "osv-scanner matched {} package {package}, as pinned in {file}, to the advisories {ids} in the OSV database",
        clean(&hit.ecosystem, 40),
    );
    if !aliases.is_empty() {
        detail.push_str(&format!(" (also {})", list(&aliases, 4)));
    }
    detail.push('.');
    if let Some(score) = hit.score {
        detail.push_str(&format!(" Highest score {score:.1}."));
    }
    for summary in hit.summaries.iter().take(3) {
        detail.push_str(&format!(" \"{}\"", clean(summary, 160)));
    }
    detail.push_str(" Upgrade to a release the advisory lists as fixed, or replace the package.");
    if let Some(unread) = &hit.unread {
        detail.push_str(&format!(
            " Note: osv-scanner could not read part of the project ({}); what it did not read was not judged.",
            clean(unread, 200)
        ));
    }
    if rule == SEC {
        detail.push_str(
            " Reachability is not analysed: this says an advisory of high or unknown severity covers the pinned version, not that the flawed code is called.",
        );
    }
    let first = hit.ids.first().map_or("an advisory", String::as_str);
    let extra = hit.ids.len().saturating_sub(1);
    // SEC-006 names what is known, never "exploitable": a serious advisory covers the version, and nothing says it is reachable
    let kind = match (rule, hit.score) {
        (SEC, Some(_)) => "known high-severity advisory",
        (SEC, None) => "known advisory with no severity score",
        _ => "known advisory",
    };
    let finding = Finding::new(
        rule,
        severity_of(hit.score),
        confidence,
        project,
        format!(
            "{package} has a {kind} ({}{})",
            clean(first, 40),
            if extra > 0 {
                format!(" and {extra} more")
            } else {
                String::new()
            }
        ),
        detail,
    )
    .location(Location::new(file.clone(), None))
    .subject(format!("{}:{}@{}", hit.ecosystem, hit.name, hit.version));
    // The summary is built from a package name, which could in principle be a whole word the host reads as an absence claim;
    // such a finding carries a control rather than being dropped.
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "osv-scanner read {file} and matched {package} to {ids}"
        )),
    }
}

/// `path` as the project names it: relative to one of `roots`, with `/` separators. A path outside every root is kept as printed.
/// (Windows prints drive letters and cases that differ from the path the tool was given, so the comparison ignores ASCII case.)
fn relative(path: &str, roots: &[String]) -> String {
    let path = path.replace('\\', "/");
    for root in roots {
        let root = root.replace('\\', "/");
        let root = root.trim_end_matches('/');
        let Some(head) = path.get(..root.len()) else {
            continue;
        };
        if !root.is_empty()
            && head.eq_ignore_ascii_case(root)
            && path[root.len()..].starts_with('/')
        {
            return path[root.len() + 1..].to_string();
        }
    }
    path
}

/// The configuration osv-scanner is run with, written to the scratch directory: an empty one, so no project's `osv-scanner.toml`
/// can ignore an advisory.
const CONFIG: &str = "";

/// The arguments of the scan of the directory `root`: JSON into `scratch`, errors only on stderr, the repository's own
/// `osv-scanner.toml` replaced and `.gitignore` not honoured. `root` is last and absolute (the tool runs in `scratch`). No call
/// analysis: that builds the project's code.
fn scan_args(root: &Path, scratch: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "scan",
        "source",
        "--format",
        "json",
        "--verbosity",
        "error",
        "--no-ignore",
        "--recursive",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    for (flag, file) in [
        ("--config", "config.toml"),
        ("--output-file", "report.json"),
    ] {
        args.push(flag.into());
        args.push(scratch.join(file).into());
    }
    args.push(root.into());
    args
}

/// What the scan came to.
#[derive(Debug, Clone)]
enum Scan {
    /// The advisory groups found (none: every package osv-scanner read is clean today).
    Hits(Vec<Hit>),
    /// Nothing was judged, and that is a gap, not a failure.
    Gap(String),
    Failed(ErrorKind, String),
}

/// The first line of stderr that says something went wrong (at `--verbosity error` that is every line, bar the one that says
/// there was nothing to scan).
fn problem(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("No package sources found"))
}

/// What the exit code, stderr and report add up to. The exit code says whether advisories were found, stderr says whether
/// everything was read, and neither alone is the verdict.
fn judge(code: Option<i32>, stderr: &str, report: Result<Vec<Hit>, String>) -> Scan {
    let problem = problem(stderr);
    match code {
        Some(EXIT_NO_SOURCES) => match problem {
            Some(why) => Scan::Failed(
                ErrorKind::ToolFailed,
                format!("{TOOL} could not read what it found: {why}"),
            ),
            None => Scan::Gap(format!(
                "{TOOL} found no package sources (a lockfile, manifest or SBOM it knows) in the project, so there is nothing for this \
                 rule to judge; it says nothing about dependencies {TOOL} does not read"
            )),
        },
        Some(code @ (0 | EXIT_FOUND)) => match report {
            Err(why) => Scan::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}")),
            Ok(hits) if hits.is_empty() && code == EXIT_FOUND => Scan::Failed(
                ErrorKind::ToolOutputUnreadable,
                format!("{TOOL} exited {EXIT_FOUND} (advisories found) but its report holds none"),
            ),
            Ok(hits) if hits.is_empty() && problem.is_some() => Scan::Failed(
                ErrorKind::ToolFailed,
                format!(
                    "{TOOL} could not read part of the project ({}); an empty report says nothing about it",
                    problem.unwrap_or_default()
                ),
            ),
            Ok(hits) => Scan::Hits(hits),
        },
        Some(EXIT_ERROR) => Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} could not read the project (exit code {EXIT_ERROR}): {stderr}"),
        ),
        Some(code) => Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} exited with code {code}: {stderr}"),
        ),
        None => Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} ended without an exit code: {stderr}"),
        ),
    }
}

/// The names `root` can be printed as: as given, and with symbolic links resolved (minus Windows' `\\?\` prefix).
fn roots_of(root: &Path) -> Vec<String> {
    let mut roots = vec![root.display().to_string()];
    if let Ok(real) = root.canonicalize() {
        let real = real.display().to_string();
        roots.push(real.strip_prefix(r"\\?\").unwrap_or(&real).to_string());
    }
    roots
}

/// Runs `program` with the arguments `args(root, scratch)` builds and reads the report it was told to write at
/// `scratch/report.json`. (The arguments are a parameter so a test can stand in a program that misbehaves; the tool is always run
/// with [`scan_args`].)
fn scan_with(program: &Path, args: fn(&Path, &Path) -> Vec<OsString>, request: &Request) -> Scan {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and scan nothing.
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    if !root.is_dir() {
        return Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{} is not a directory", root.display()),
        );
    }
    let run = match run_tool(
        program,
        |scratch| {
            // a failure to write this surfaces as the tool failing to start with it
            let _ = std::fs::write(scratch.join("config.toml"), CONFIG);
            args(&root, scratch)
        },
        &request.limits,
        &[EXIT_FOUND, EXIT_ERROR, EXIT_NO_SOURCES],
    ) {
        Ok(run) => run,
        Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
    };
    let report = std::fs::read(run.scratch().join("report.json"))
        .map_err(|e| format!("it wrote no report ({e})"))
        .and_then(|bytes| parse_report(&String::from_utf8_lossy(&bytes)));
    match judge(run.exit_code(), run.log(), report) {
        Scan::Hits(mut hits) => {
            if hits.len() > request.limits.max_findings {
                return Scan::Failed(
                    ErrorKind::LimitExceeded,
                    format!(
                        "{TOOL} reported {} advisory groups, past the limit of {}",
                        hits.len(),
                        request.limits.max_findings
                    ),
                );
            }
            let roots = roots_of(&root);
            let unread = problem(run.log()).map(str::to_string);
            for hit in &mut hits {
                hit.file = relative(&hit.file, &roots);
                hit.unread.clone_from(&unread);
            }
            Scan::Hits(hits)
        }
        other => other,
    }
}

/// The last scan, so SUP-002 and SEC-006 asked for in one run share one (osv-scanner is a network call, and the two rules read the
/// same report). Keyed by the run and the project, so a later run never reads an earlier answer.
static LAST: Mutex<Option<(String, PathBuf, Scan)>> = Mutex::new(None);

fn scan(env: &ToolEnv, request: &Request) -> Scan {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((run, project, scan)) = last.as_ref() {
        if *run == request.run_id && *project == request.project_root {
            return scan.clone();
        }
    }
    let scan = if request.network {
        match resolve(env, TOOL) {
            Resolved::Ready(program) => scan_with(&program, scan_args, request),
            Resolved::Unavailable(why) => Scan::Gap(format!("{TOOL} is not available: {why}")),
            Resolved::Failed(kind, why) => Scan::Failed(kind, why),
        }
    } else {
        Scan::Gap(format!(
            "{TOOL} asks the OSV database over the network and this run does not permit the network (a sweep does)"
        ))
    };
    *last = Some((
        request.run_id.clone(),
        request.project_root.clone(),
        scan.clone(),
    ));
    scan
}

fn run_rule(rule: &str, env: &ToolEnv, request: &Request) -> Verdict {
    match scan(env, request) {
        Scan::Hits(hits) => {
            let project = std::path::absolute(&request.project_root)
                .unwrap_or_else(|_| request.project_root.clone())
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Verdict::Findings(
                hits.iter()
                    .filter(|h| applies(rule, h))
                    .map(|h| to_finding(h, &project, rule))
                    .collect(),
            )
        }
        Scan::Gap(detail) => Verdict::Unavailable(detail),
        Scan::Failed(kind, detail) => Verdict::Failed(kind, detail),
    }
}

pub(super) fn run_sup(env: &ToolEnv, request: &Request) -> Verdict {
    run_rule(SUP, env, request)
}

pub(super) fn run_sec(env: &ToolEnv, request: &Request) -> Verdict {
    run_rule(SEC, env, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What osv-scanner 2.6.0 wrote for a lockfile pinning lodash 4.17.15 (measured 2026-10-09 with `--format json`; the advisory
    /// bodies, `affected` ranges and references are left out, they are not read).
    const REPORT: &str = r#"{
  "results": [
    {
      "source": {
        "path": "C:/work/demo/package-lock.json",
        "type": "lockfile"
      },
      "packages": [
        {
          "package": { "name": "lodash", "version": "4.17.15", "ecosystem": "npm" },
          "groups": [
            { "ids": ["GHSA-29mw-wpgm-hmr9"], "aliases": ["CVE-2020-28500", "GHSA-29mw-wpgm-hmr9"], "max_severity": "5.3" },
            { "ids": ["GHSA-35jh-r3h4-6jhm", "GHSA-r5fr-rjxr-66jc"], "aliases": ["CVE-2021-23337", "CVE-2026-4800", "GHSA-35jh-r3h4-6jhm", "GHSA-r5fr-rjxr-66jc"], "max_severity": "8.1" },
            { "ids": ["GHSA-p6mc-m468-83gw"], "aliases": ["CVE-2020-8203", "GHSA-p6mc-m468-83gw"], "max_severity": "" }
          ],
          "vulnerabilities": [
            { "id": "GHSA-29mw-wpgm-hmr9", "summary": "Regular Expression Denial of Service (ReDoS) in lodash", "aliases": ["CVE-2020-28500"] },
            { "id": "GHSA-35jh-r3h4-6jhm", "summary": "Command Injection in lodash", "aliases": ["CVE-2021-23337"] },
            { "id": "GHSA-r5fr-rjxr-66jc", "summary": "lodash vulnerable to Code Injection via `_.template` imports key names", "aliases": [] },
            { "id": "GHSA-p6mc-m468-83gw", "summary": "Prototype Pollution in lodash", "aliases": ["CVE-2020-8203"] }
          ]
        }
      ]
    }
  ],
  "experimental_config": { "licenses": { "summary": false, "allowlist": null } }
}"#;

    #[test]
    fn the_real_report_shape_gives_one_hit_per_advisory_group() {
        let hits = parse_report(REPORT).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[1].name, "lodash");
        assert_eq!(hits[1].version, "4.17.15");
        assert_eq!(hits[1].ecosystem, "npm");
        assert_eq!(hits[1].file, "C:/work/demo/package-lock.json");
        assert_eq!(hits[1].ids, ["GHSA-35jh-r3h4-6jhm", "GHSA-r5fr-rjxr-66jc"]);
        assert_eq!(hits[1].score, Some(8.1));
        assert_eq!(hits[2].score, None, "an empty score is unknown, not zero");
        assert!(hits[1]
            .summaries
            .contains(&"Command Injection in lodash".to_string()));
    }

    #[test]
    fn an_empty_result_list_is_clean_and_anything_else_is_unreadable() {
        assert_eq!(parse_report(r#"{"results": []}"#).unwrap().len(), 0);
        for bad in [
            "",
            "null",
            "[]",
            "{}",
            "not json",
            r#"{"results": 1}"#,
            r#"{"results": [{}]}"#,
        ] {
            assert!(
                parse_report(bad).is_err(),
                "{bad:?} must be unreadable, not clean"
            );
        }
    }

    #[test]
    fn a_score_maps_to_a_severity_and_an_unknown_score_is_not_hidden() {
        assert_eq!(severity_of(Some(9.8)), Severity::Critical);
        assert_eq!(severity_of(Some(9.0)), Severity::Critical);
        assert_eq!(severity_of(Some(8.1)), Severity::High);
        assert_eq!(severity_of(Some(7.0)), Severity::High);
        assert_eq!(severity_of(Some(5.3)), Severity::Medium);
        assert_eq!(severity_of(Some(4.0)), Severity::Medium);
        assert_eq!(severity_of(Some(3.9)), Severity::Low);
        assert_eq!(severity_of(Some(0.0)), Severity::Low);
        assert_eq!(
            severity_of(None),
            Severity::High,
            "an advisory with no score is not reported as minor"
        );
    }

    #[test]
    fn sup_002_reports_every_advisory_and_sec_006_only_those_scored_high_or_unknown() {
        let hits = parse_report(REPORT).unwrap();
        let sup: Vec<_> = hits.iter().filter(|h| applies(SUP, h)).collect();
        let sec: Vec<_> = hits.iter().filter(|h| applies(SEC, h)).collect();
        assert_eq!(sup.len(), 3);
        assert_eq!(
            sec.len(),
            2,
            "5.3 is under the line, 8.1 and the unscored one are not"
        );
        assert!(sec.iter().all(|h| h.score != Some(5.3)));
    }

    #[test]
    fn a_finding_names_the_lockfile_the_package_and_the_advisories_and_is_valid() {
        let mut hits = parse_report(REPORT).unwrap();
        hits[1].file = "package-lock.json".into();
        let f = to_finding(&hits[1], "demo", SUP);
        assert_eq!(f.check_id, "SUP-002");
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.confidence, Confidence::Medium);
        let location = f.location.as_ref().unwrap();
        assert_eq!(
            (location.file.as_str(), location.line),
            ("package-lock.json", None)
        );
        assert_eq!(f.subject.as_deref(), Some("npm:lodash@4.17.15"));
        assert!(f.summary.contains("lodash 4.17.15"), "{}", f.summary);
        assert!(
            f.detail.contains("GHSA-35jh-r3h4-6jhm") && f.detail.contains("CVE-2021-23337"),
            "{}",
            f.detail
        );
        assert!(
            f.detail.contains("Command Injection in lodash"),
            "{}",
            f.detail
        );
        assert!(f.clone().validate().is_ok());
        let sec = to_finding(&hits[1], "demo", SEC);
        assert_eq!(sec.check_id, "SEC-006");
        assert_eq!(
            sec.confidence,
            Confidence::Low,
            "nothing here shows the flaw is reachable"
        );
        assert!(sec.detail.contains("not analysed"), "{}", sec.detail);
    }

    #[test]
    fn advisory_text_cannot_carry_terminal_control_codes_or_run_on() {
        let mut hits = parse_report(REPORT).unwrap();
        hits[1].summaries = vec![format!("evil\u{1b}[31m red{}", "x".repeat(500))];
        let f = to_finding(&hits[1], "demo", SUP);
        assert!(!f.detail.contains('\u{1b}'), "{:?}", f.detail);
        assert!(f.detail.len() < 1500, "{}", f.detail.len());
    }

    #[test]
    fn a_package_called_none_still_makes_a_valid_finding() {
        let mut hits = parse_report(REPORT).unwrap();
        hits[1].name = "none".into(); // a whole word: the host would read the summary as an absence claim
        let f = to_finding(&hits[1], "demo", SUP);
        assert!(f.positive_control.is_some());
        assert!(f.validate().is_ok());
    }

    #[test]
    fn paths_are_made_relative_to_the_project_and_windows_separators_become_slashes() {
        let roots = vec!["C:/work/demo".to_string()];
        assert_eq!(
            relative("C:/work/demo/package-lock.json", &roots),
            "package-lock.json"
        );
        assert_eq!(
            relative(r"C:\work\demo\web\package-lock.json", &roots),
            "web/package-lock.json"
        );
        assert_eq!(
            relative("c:/WORK/demo/a/b.lock", &roots),
            "a/b.lock",
            "drive letters and case differ on Windows"
        );
        assert_eq!(
            relative("/elsewhere/x.lock", &roots),
            "/elsewhere/x.lock",
            "an unrelated path is kept as printed"
        );
        assert_eq!(
            relative("C:/work/demo2/x.lock", &roots),
            "C:/work/demo2/x.lock",
            "a sibling is not inside the project"
        );
    }

    #[test]
    fn the_scan_ignores_whatever_the_repository_says_about_itself() {
        let args: Vec<String> = scan_args(Path::new("/abs/repo"), Path::new("/scratch"))
            .iter()
            .map(|a| a.to_string_lossy().replace('\\', "/"))
            .collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(&args[..2], ["scan", "source"]);
        assert!(
            has("--no-ignore"),
            "a .gitignore must not hide a lockfile: {args:?}"
        );
        assert!(has("--recursive"), "{args:?}");
        assert_eq!(after("--format"), "json");
        assert_eq!(after("--verbosity"), "error");
        assert_eq!(
            after("--config"),
            "/scratch/config.toml",
            "the repository's osv-scanner.toml is never read"
        );
        assert_eq!(after("--output-file"), "/scratch/report.json");
        assert_eq!(args.last().map(String::as_str), Some("/abs/repo"));
        assert!(
            !has("--call-analysis"),
            "call analysis builds the project's code"
        );
    }

    fn hits() -> Result<Vec<Hit>, String> {
        parse_report(REPORT)
    }

    #[test]
    fn exit_1_with_a_report_is_findings_and_exit_0_with_none_is_clean() {
        assert!(matches!(judge(Some(1), "", hits()), Scan::Hits(h) if h.len() == 3));
        assert!(matches!(judge(Some(0), "", Ok(vec![])), Scan::Hits(h) if h.is_empty()));
    }

    #[test]
    fn exit_128_is_a_gap_because_nothing_was_there_to_judge() {
        let scan = judge(
            Some(128),
            "No package sources found, --help for usage information.",
            Err("no report".into()),
        );
        assert!(
            matches!(&scan, Scan::Gap(why) if why.contains("no package")),
            "{scan:?}"
        );
    }

    #[test]
    fn a_lockfile_the_tool_could_not_read_makes_a_clean_result_a_failure() {
        // measured: a broken lockfile beside clean ones exits 127; beside a vulnerable one it exits 1; stderr is the only place it is named
        let err = "Error during extraction: (extracting as javascript/packagelockjson) bad/package-lock.json: could not extract: invalid character";
        assert!(
            matches!(judge(Some(0), err, Ok(vec![])), Scan::Failed(ErrorKind::ToolFailed, why) if why.contains("could not extract"))
        );
        // an unreadable lockfile alone: 128, with the same message, is not "nothing to judge"
        assert!(matches!(
            judge(Some(128), err, Err("none".into())),
            Scan::Failed(ErrorKind::ToolFailed, _)
        ));
        // findings that were read are still true
        assert!(matches!(judge(Some(1), err, hits()), Scan::Hits(h) if h.len() == 3));
    }

    #[test]
    fn exit_1_without_findings_and_other_exits_are_failures() {
        assert!(matches!(
            judge(Some(1), "", Ok(vec![])),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
        assert!(
            matches!(judge(Some(127), "failed to resolve path", Err("none".into())), Scan::Failed(ErrorKind::ToolFailed, why) if why.contains("failed to resolve") && why.contains("could not read"))
        );
        assert!(
            matches!(judge(Some(2), "boom", Err("none".into())), Scan::Failed(ErrorKind::ToolFailed, why) if why.contains("boom"))
        );
        assert!(matches!(judge(None, "", hits()), Scan::Failed(..)));
        assert!(matches!(
            judge(Some(0), "", Err("not json".into())),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
    }

    #[test]
    fn findings_read_beside_an_unreadable_lockfile_say_so() {
        let mut hits = parse_report(REPORT).unwrap();
        assert!(!to_finding(&hits[1], "demo", SUP)
            .detail
            .contains("could not read"));
        hits[1].unread = Some("Error during extraction: bad/package-lock.json".into());
        let f = to_finding(&hits[1], "demo", SUP);
        assert!(
            f.detail.contains("could not read part") && f.detail.contains("bad/package-lock.json"),
            "{}",
            f.detail
        );
        assert!(f.validate().is_ok());
    }

    #[test]
    fn no_message_calls_a_finding_exploitable_and_sec_006_says_what_it_knows() {
        for hit in parse_report(REPORT).unwrap() {
            for rule in [SUP, SEC] {
                let f = to_finding(&hit, "demo", rule);
                let text = format!("{} {}", f.summary, f.detail).to_lowercase();
                assert!(!text.contains("exploitable"), "{text}");
            }
        }
        let hits = parse_report(REPORT).unwrap();
        let scored = to_finding(&hits[1], "demo", SEC);
        assert!(
            scored.summary.contains("known high-severity advisory"),
            "{}",
            scored.summary
        );
        assert!(
            scored.detail.contains("Reachability is not analysed"),
            "{}",
            scored.detail
        );
        let unscored = to_finding(&hits[2], "demo", SEC);
        assert!(
            unscored.summary.contains("no severity score"),
            "{}",
            unscored.summary
        );
        assert!(
            !unscored.summary.contains("high-severity"),
            "{}",
            unscored.summary
        );
    }

    #[test]
    fn the_records_say_what_is_checked_today_and_sec_006_states_its_narrowing() {
        let catalog = crate::catalog::Catalog::builtin();
        for rule in [SUP, SEC] {
            let text = catalog
                .get(rule)
                .unwrap()
                .checked_today()
                .unwrap_or_else(|| panic!("{rule}: the record does not say what is checked today"));
            assert!(text.contains("OSV"), "{rule}: {text}");
        }
        let sec = catalog.get(SEC).unwrap().checked_today().unwrap();
        assert!(sec.contains("7.0") && sec.contains("reachab"), "{sec}");
        assert!(!sec.to_lowercase().contains("is exploitable"), "{sec}");
    }

    #[test]
    fn any_stderr_on_a_clean_run_is_a_partial_read() {
        assert!(matches!(
            judge(Some(0), "warning: something", Ok(vec![])),
            Scan::Failed(ErrorKind::ToolFailed, _)
        ));
    }
}
