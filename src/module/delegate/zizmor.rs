//! SUP-008 (CI workflows pinned and least-privileged), delegated to [zizmor](https://github.com/zizmorcore/zizmor) (MIT).
//!
//! **What this judges:** zizmor is given the project's *tracked* GitHub Actions files (`git ls-files`, never a walk of the disk):
//! the workflows in `.github/workflows/` and every `action.yml`/`action.yaml` (composite actions). It runs **offline** (its online
//! audits need a GitHub token and the network; this rule is local-tier and asks for neither). Of what zizmor reports only the
//! three audits that are this rule's wording are kept: `unpinned-images` (a `container:`, `services:` or `docker://` image not
//! pinned to a digest), `unpinned-uses` (a third-party action, reusable workflow or image that is not
//! pinned to an immutable hash) and `excessive-permissions` (a workflow or job whose token has more permission than it declares
//! or needs). Every other audit zizmor has (template injection, credential persistence, dangerous triggers, ...) is a different
//! rule's business and is not reported here.
//!
//! **Why the `pedantic` persona:** measured against the real tool, the default (`regular`) persona does not report
//! `permissions: write-all` at all (it reports it from `pedantic` up), which is the clearest breach of "least-privileged" there is,
//! and it reports no `container:` image that is not pinned. `pedantic` finds both, and the `auditor` persona added nothing for the
//! two audits kept on the workflows probed. The cost is that a job that really needs `contents: write` is reported too: zizmor cannot know what a job
//! needs, so those findings carry its confidence and the catalog says the rule is about *declared* permissions. (Measured: zizmor
//! reports a workflow-level write scope, `write-all` and `read-all`, and a job with no `permissions:` block under a workflow with
//! none; it does **not** report a write scope declared on one job, which is the least-privilege pattern.)
//!
//! **Not judged is not clean:** zizmor exits 0 with `[]` having read only some of its inputs (measured: a workflow with a YAML
//! syntax error beside a valid one: it warns on stderr, skips the broken file and exits 0). Run quietly (`-q`: stderr then holds only
//! warnings and errors, so a long log cannot push a warning out of the part CodeRipper keeps), a warning that an input failed to
//! parse, validate or load beside an exit 0 is a
//! partial read: a clean result becomes a **coverage gap**, and findings carry a note. When *no* input could be read (exit 3,
//! "no inputs collected") the rule is a gap. A project with no tracked workflow or action is a gap too: nothing was there to judge.
//!
//! **The repository cannot silence the rule:** zizmor reads a `zizmor.yml` (or `.github/zizmor.yml`) found from the file it audits
//! upward (measured: a committed `rules: unpinned-uses: disable: true` drops the findings), and honours `# zizmor: ignore[...]`
//! comments. `--no-config` and `--no-ignores` turn both off (a finding that carries such a comment still comes back flagged
//! `ignored`, and says so), and `.gitignore` does not apply to files named on the command line (measured). The project's way to
//! suppress a finding is the `.coderipper.toml` allowlist, which is visible.
//!
//! **The mapping record:** one finding per zizmor finding, at the file and line of its primary location (zizmor counts lines from
//! 0). Severity and confidence are zizmor's own (`Informational` is `Info`). Subject: the `uses:` value for an unpinned reference,
//! the place in the file (`workflow`, `jobs.build`) for a permission. The text of the workflow is third-party text that reaches a
//! terminal: control characters are removed and the length is cut.

use super::osv::{clean, relative, roots_of};
use super::{resolve, run_tool, tracked_listing, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::Path;

/// The catalog rule this evidences.
pub(super) const RULE: &str = "SUP-008";
/// The tool's name in the lock.
pub(super) const TOOL: &str = "zizmor";

/// The zizmor audits that are this rule's wording, and the only ones reported.
const AUDITS: &[&str] = &["unpinned-uses", "unpinned-images", "excessive-permissions"];
/// What zizmor exits with, with `--no-exit-codes`, when no input could be read at all (any other failure is 1 or 2).
const EXIT_NO_INPUTS: i32 = 3;
/// How many bytes of file names one zizmor run is given: a Windows command line is cut at 32 767 characters.
const ARGS_BUDGET: usize = 16_000;
/// Directories of other people's files, which are not the project's to fix: a tracked file below one is not read.
const SKIPPED_DIRS: &[&str] = &["node_modules", "vendor", "third_party", "target", ".git"];

/// Whether the project-relative path (with `/`) is a workflow or an action definition.
fn is_input(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    if segments
        .iter()
        .any(|s| SKIPPED_DIRS.iter().any(|d| s.eq_ignore_ascii_case(d)))
    {
        return false;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if !(ext == "yml" || ext == "yaml") {
        return false;
    }
    segments == [".github", "workflows"] || stem == "action"
}

/// The tracked workflows and actions of the repository at `root` that exist on disk, as absolute paths in the listing's order.
fn select_inputs(listing: &str, root: &Path) -> Vec<OsString> {
    listing
        .split('\0')
        .filter(|p| !p.is_empty() && is_input(p) && root.join(p).is_file())
        .map(|p| root.join(p).into_os_string())
        .collect()
}

/// `inputs` in groups whose names add up to at most [`ARGS_BUDGET`] bytes (a group has at least one).
fn chunks(inputs: &[OsString]) -> Vec<&[OsString]> {
    let mut out = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (i, input) in inputs.iter().enumerate() {
        let size = input.len() + 1;
        if i > start && used + size > ARGS_BUDGET {
            out.push(&inputs[start..i]);
            (start, used) = (i, 0);
        }
        used += size;
    }
    if start < inputs.len() {
        out.push(&inputs[start..]);
    }
    out
}

#[derive(Debug, Deserialize)]
struct Entry {
    ident: String,
    determinations: Determinations,
    locations: Vec<Place>,
    #[serde(default)]
    ignored: bool,
}

#[derive(Debug, Deserialize)]
struct Determinations {
    confidence: String,
    severity: String,
}

#[derive(Debug, Deserialize)]
struct Place {
    symbolic: Symbolic,
    concrete: Concrete,
}

#[derive(Debug, Deserialize)]
struct Symbolic {
    key: serde_json::Value,
    #[serde(default)]
    annotation: String,
    #[serde(default)]
    route: Route,
    kind: String,
}

#[derive(Debug, Default, Deserialize)]
struct Route {
    #[serde(default)]
    route: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Concrete {
    location: Span,
    #[serde(default)]
    feature: String,
}

#[derive(Debug, Deserialize)]
struct Span {
    start_point: Point,
}

#[derive(Debug, Deserialize)]
struct Point {
    row: u32,
}

/// One zizmor finding of an audit this rule reports.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    audit: String,
    severity: Severity,
    confidence: Confidence,
    /// The file as zizmor printed it (the path it was given) until [`relative`] makes it the project's.
    file: String,
    /// 1-based.
    line: u32,
    /// What zizmor said about the primary location.
    annotation: String,
    /// The text at the primary location (for an unpinned reference, the `uses:` value).
    feature: String,
    /// Where in the file: `workflow`, or the keys down to the job or step (`jobs.build`, `jobs.build.steps[0]`).
    place: String,
    /// The finding carries a `# zizmor: ignore` comment, which is not honoured.
    ignored: bool,
}

fn severity_of(text: &str) -> Result<Severity, String> {
    match text {
        "High" => Ok(Severity::High),
        "Medium" => Ok(Severity::Medium),
        "Low" => Ok(Severity::Low),
        "Informational" => Ok(Severity::Info),
        other => Err(format!("a severity it does not know: {}", clean(other, 40))),
    }
}

fn confidence_of(text: &str) -> Result<Confidence, String> {
    match text {
        "High" => Ok(Confidence::High),
        "Medium" => Ok(Confidence::Medium),
        "Low" => Ok(Confidence::Low),
        other => Err(format!(
            "a confidence it does not know: {}",
            clean(other, 40)
        )),
    }
}

/// `jobs.build.steps[0]` for a route of keys and indexes; `workflow` for the empty route (the whole file).
fn place_of(route: &Route) -> String {
    let mut out = String::new();
    for part in &route.route {
        if let Some(key) = part.get("Key").and_then(|k| k.as_str()) {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(key);
        } else if let Some(index) = part.get("Index").and_then(|i| i.as_u64()) {
            out.push_str(&format!("[{index}]"));
        }
    }
    if out.is_empty() {
        "workflow".into()
    } else {
        out
    }
}

/// Reads zizmor's JSON report (`--format json`): a list of findings. Only those of [`AUDITS`] are kept. A report that is not
/// zizmor's, or a finding that cannot be placed in a file, is unreadable (a partial list must not look like a complete one).
fn parse_report(text: &str) -> Result<Vec<Hit>, String> {
    let entries: Vec<Entry> = serde_json::from_str(text).map_err(|e| {
        format!(
            "the report is not a zizmor result ({e}): {}",
            text.chars().take(200).collect::<String>()
        )
    })?;
    let mut hits = Vec::new();
    for entry in entries {
        if !AUDITS.contains(&entry.ident.as_str()) {
            continue;
        }
        let primary = entry
            .locations
            .iter()
            .find(|p| p.symbolic.kind == "Primary")
            .ok_or_else(|| format!("a {} finding has no primary location", entry.ident))?;
        let file = primary
            .symbolic
            .key
            .get("Local")
            .and_then(|l| l.get("verbatim_path"))
            .and_then(|p| p.as_str())
            .ok_or_else(|| {
                format!(
                    "a {} finding is not in a local file (it names {})",
                    entry.ident,
                    clean(&primary.symbolic.key.to_string(), 100)
                )
            })?;
        hits.push(Hit {
            severity: severity_of(&entry.determinations.severity)?,
            confidence: confidence_of(&entry.determinations.confidence)?,
            file: file.to_string(),
            line: primary.concrete.location.start_point.row + 1,
            annotation: primary.symbolic.annotation.clone(),
            feature: primary.concrete.feature.clone(),
            place: place_of(&primary.symbolic.route),
            ignored: entry.ignored,
            audit: entry.ident,
        });
    }
    // zizmor reports by audit: the findings are in the order of the files
    hits.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    Ok(hits)
}

/// The finding for one zizmor finding. `roots` are the names the project root can be printed as.
fn to_finding(hit: &Hit, project: &str, roots: &[String], extra: Option<&str>) -> Finding {
    let file = clean(&relative(&hit.file, roots), 200);
    let annotation = clean(&hit.annotation, 160);
    let (summary, subject, advice) = if hit.audit == "unpinned-images" {
        let target = clean(hit.feature.trim(), 200);
        (
            format!("the image {target} is not pinned to a digest"),
            target,
            "A mutable tag lets whoever controls the image run other code in the job. Pin it to its sha256 digest (keep the tag in a comment).",
        )
    } else if hit.audit == "unpinned-uses" {
        let target = clean(&hit.feature, 200);
        (
            format!("{target} is not pinned to an immutable hash"),
            target,
            "A mutable tag or branch lets a compromised upstream run with the project's own credentials. Pin it to a full commit \
             hash (keep the version in a comment).",
        )
    } else {
        let place = clean(&hit.place, 200);
        (
            format!("{place}: {annotation}"),
            place,
            "Declare the permissions the workflow needs, at the workflow level (`permissions: {}` or `contents: read`) and raise them \
             for the one job that needs more.",
        )
    };
    let mut detail = format!(
        "zizmor ({}) reports {file} line {}: {annotation}. {advice}",
        hit.audit, hit.line
    );
    if hit.ignored {
        detail.push_str(
            " This finding carries a `# zizmor: ignore` comment, which CodeRipper does not honour (the project's way to suppress a \
             finding is the .coderipper.toml allowlist).",
        );
    }
    if let Some(extra) = extra {
        detail.push(' ');
        detail.push_str(extra);
    }
    let finding = Finding::new(RULE, hit.severity, hit.confidence, project, summary, detail)
        .location(Location::new(file.clone(), Some(hit.line)))
        .subject(subject.clone());
    // the summary quotes the workflow and zizmor's words, and could contain a whole word the host reads as an absence claim ("no
    // permissions: block"): such a finding carries a control
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "zizmor read {file} and reported {} at line {}",
            hit.audit, hit.line
        )),
    }
}

/// The first line of stderr that says an input was not read. Run with `-q` zizmor writes only warnings and errors there, but not
/// every warning is about an input: an audit warns too (measured: `couldn't determine shell type` for a `pull_request_target`
/// workflow on a self-hosted runner, with every file read), and such a warning is not a partial read.
fn problem(stderr: &str) -> Option<&str> {
    const UNREAD: &[&str] = &[
        "failed to parse input",
        "failed to validate",
        "failed to load",
    ];
    stderr
        .lines()
        .map(str::trim)
        .find(|l| UNREAD.iter().any(|k| l.contains(k)))
}

/// What one zizmor run came to.
#[derive(Debug, Clone)]
enum Chunk {
    /// Some input was read. `unread` is the first thing zizmor said it could not read, when it said so.
    Read {
        hits: Vec<Hit>,
        unread: Option<String>,
    },
    /// No input could be read: nothing was judged.
    NothingRead(String),
    Failed(ErrorKind, String),
}

/// What the exit code, stderr and report add up to. The exit code says whether the tool ran, stderr says whether everything was
/// read, and neither alone is the verdict.
fn judge(code: Option<i32>, stderr: &str, report: Result<Vec<Hit>, String>) -> Chunk {
    match code {
        Some(0) => match report {
            Ok(hits) => Chunk::Read {
                hits,
                unread: problem(stderr).map(str::to_string),
            },
            Err(why) => Chunk::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}")),
        },
        Some(EXIT_NO_INPUTS) if stderr.contains("no inputs collected") => Chunk::NothingRead(
            problem(stderr)
                .unwrap_or("it collected no input")
                .to_string(),
        ),
        Some(code) => Chunk::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} exited with code {code}: {stderr}"),
        ),
        None => Chunk::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} ended without an exit code: {stderr}"),
        ),
    }
}

/// What the whole project came to.
#[derive(Debug, Clone)]
enum Scan {
    Read {
        hits: Vec<Hit>,
        unread: Option<String>,
    },
    /// Nothing was judged, and that is a gap, not a failure.
    Gap(String),
    Failed(ErrorKind, String),
}

/// The arguments of one run over `inputs` (absolute paths): offline, the repository's own configuration and ignore comments
/// replaced by none, the audits of the `pedantic` persona, JSON on stdout, quiet (warnings and errors only on stderr), and exit
/// codes that say only whether the tool worked. `--` ends the options: an input is never read as one.
fn check_args(inputs: &[OsString]) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--offline",
        "--no-config",
        "--no-ignores",
        "--no-exit-codes",
        "--no-progress",
        "-q",
        "--persona",
        "pedantic",
        "--format",
        "json",
        "--",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.extend(inputs.iter().cloned());
    args
}

/// Runs `program` with the arguments `args(inputs)` builds over the project's tracked workflows and actions and reads what it
/// prints. (The arguments are a parameter so a test can stand in a program that misbehaves; the tool is always run with
/// [`check_args`].)
fn scan_with(program: &Path, args: fn(&[OsString]) -> Vec<OsString>, request: &Request) -> Scan {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and check nothing.
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    if !root.is_dir() {
        return Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{} is not a directory", root.display()),
        );
    }
    let inputs = match tracked_listing(&root) {
        Ok(listing) => select_inputs(&listing, &root),
        Err(why) => return Scan::Failed(ErrorKind::ToolFailed, why),
    };
    if inputs.is_empty() {
        return Scan::Gap(format!(
            "the project has no tracked GitHub Actions workflow or action definition, so there is nothing for {TOOL} to judge"
        ));
    }
    let mut hits = Vec::new();
    let mut unread: Option<String> = None;
    let mut nothing_read: Option<String> = None;
    let mut read_any = false;
    for group in chunks(&inputs) {
        let run = match run_tool(
            program,
            |_scratch| args(group),
            &request.limits,
            &[EXIT_NO_INPUTS],
        ) {
            Ok(run) => run,
            Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
        };
        let report = parse_report(&run.stdout());
        match judge(run.exit_code(), run.log(), report) {
            Chunk::Read {
                hits: found,
                unread: u,
            } => {
                read_any = true;
                hits.extend(found);
                if unread.is_none() {
                    unread = u;
                }
            }
            Chunk::NothingRead(why) => {
                nothing_read.get_or_insert(why);
            }
            Chunk::Failed(kind, detail) => return Scan::Failed(kind, detail),
        }
    }
    if !read_any {
        return Scan::Gap(format!(
            "{TOOL} could not read any of the project's workflows or actions ({}), so nothing was judged",
            clean(
                nothing_read.as_deref().unwrap_or("it collected no input"),
                200
            )
        ));
    }
    if unread.is_none() {
        unread = nothing_read;
    }
    if hits.len() > request.limits.max_findings {
        return Scan::Failed(
            ErrorKind::LimitExceeded,
            format!(
                "{TOOL} reported {} findings, past the limit of {}",
                hits.len(),
                request.limits.max_findings
            ),
        );
    }
    hits.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    Scan::Read { hits, unread }
}

/// What a result says about the files that were not read.
fn caveat(unread: Option<&str>) -> Option<String> {
    unread.map(|why| {
        format!(
            "Note: {TOOL} could not read part of the project ({}); what it did not read was not judged.",
            clean(why, 200)
        )
    })
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    let program = match resolve(env, TOOL) {
        Resolved::Ready(program) => program,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"))
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    match scan_with(&program, check_args, request) {
        Scan::Read { hits, unread } => {
            let note = caveat(unread.as_deref());
            if hits.is_empty() {
                if let Some(note) = note {
                    return Verdict::Unavailable(format!(
                        "{TOOL} found nothing to report, but it cannot say the workflows are fine. {note}"
                    ));
                }
                return Verdict::Findings(vec![]);
            }
            let root = std::path::absolute(&request.project_root)
                .unwrap_or_else(|_| request.project_root.clone());
            let project = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let roots = roots_of(&root);
            Verdict::Findings(
                hits.iter()
                    .map(|h| to_finding(h, &project, &roots, note.as_deref()))
                    .collect(),
            )
        }
        Scan::Gap(detail) => Verdict::Unavailable(detail),
        Scan::Failed(kind, detail) => Verdict::Failed(kind, detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What zizmor 1.30.1 wrote (`--format json --persona pedantic`, measured 2026-10-09) for a workflow with an unpinned
    /// action and no `permissions:` block, plus a finding of an audit this rule does not report (the unread fields and most
    /// related locations are left out; the shape of every field read is the real one).
    const REPORT: &str = r#"[
  { "ident": "artipacked", "desc": "x", "url": "https://docs.zizmor.sh/audits/#artipacked",
    "determinations": { "confidence": "Low", "severity": "Medium", "persona": "Regular" },
    "locations": [ { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } },
        "annotation": "does not set persist-credentials: false", "route": { "route": [ { "Key": "jobs" }, { "Key": "build" }, { "Key": "steps" }, { "Index": 0 } ] },
        "feature_kind": "Normal", "kind": "Primary" },
      "concrete": { "location": { "start_point": { "row": 6, "column": 8 }, "end_point": { "row": 6, "column": 33 }, "offset_span": { "start": 79, "end": 104 } },
        "feature": "uses: actions/checkout@v4", "comments": [] } } ], "ignored": false, "fixes": [] },
  { "ident": "excessive-permissions", "desc": "overly broad permissions", "url": "https://docs.zizmor.sh/audits/#excessive-permissions",
    "determinations": { "confidence": "Medium", "severity": "Medium", "persona": "Regular" },
    "locations": [
      { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } }, "annotation": "this job",
          "route": { "route": [ { "Key": "jobs" }, { "Key": "build" } ] }, "feature_kind": "Normal", "kind": "Related" },
        "concrete": { "location": { "start_point": { "row": 3, "column": 2 } }, "feature": "  build:\n" } },
      { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } }, "annotation": "default permissions used due to no permissions: block",
          "route": { "route": [ { "Key": "jobs" }, { "Key": "build" } ] }, "feature_kind": "Normal", "kind": "Primary" },
        "concrete": { "location": { "start_point": { "row": 3, "column": 2 } }, "feature": "  build:\n    runs-on: ubuntu-latest\n" } } ],
    "ignored": false, "fixes": [] },
  { "ident": "excessive-permissions", "desc": "overly broad permissions", "url": "u",
    "determinations": { "confidence": "Medium", "severity": "Medium", "persona": "Regular" },
    "locations": [ { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } },
        "annotation": "default permissions used due to no permissions: block", "route": { "route": [] }, "feature_kind": "Normal", "kind": "Primary" },
      "concrete": { "location": { "start_point": { "row": 0, "column": 0 } }, "feature": "name: ci\n" } } ], "ignored": false, "fixes": [] },
  { "ident": "unpinned-uses", "desc": "unpinned action reference", "url": "u",
    "determinations": { "confidence": "High", "severity": "High", "persona": "Regular" },
    "locations": [
      { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } }, "annotation": "this step",
          "route": { "route": [ { "Key": "jobs" }, { "Key": "build" }, { "Key": "steps" }, { "Index": 0 } ] }, "feature_kind": "Normal", "kind": "Hidden" },
        "concrete": { "location": { "start_point": { "row": 6, "column": 8 } }, "feature": "uses: actions/checkout@v4" } },
      { "symbolic": { "key": { "Local": { "verbatim_path": "C:\\work\\demo/.github/workflows/ci.yml" } },
          "annotation": "action is not pinned to a hash (required by blanket policy)",
          "route": { "route": [ { "Key": "jobs" }, { "Key": "build" }, { "Key": "steps" }, { "Index": 0 }, { "Key": "uses" } ] },
          "feature_kind": { "Subfeature": { "after": 0, "fragment": { "Raw": "actions/checkout@v4" } } }, "kind": "Primary" },
        "concrete": { "location": { "start_point": { "row": 6, "column": 14 } }, "feature": "actions/checkout@v4", "comments": [] } } ],
    "ignored": true, "fixes": [] }
]"#;

    fn roots() -> Vec<String> {
        vec!["C:/work/demo".to_string()]
    }

    #[test]
    fn the_real_report_shape_gives_the_two_audits_this_rule_is_and_drops_the_rest() {
        let hits = parse_report(REPORT).unwrap();
        let seen: Vec<_> = hits
            .iter()
            .map(|h| {
                (
                    h.audit.as_str(),
                    relative(&h.file, &roots()),
                    h.line,
                    h.place.as_str(),
                    h.severity,
                    h.confidence,
                    h.ignored,
                )
            })
            .collect();
        // artipacked is another rule's business; line 1 is zizmor's row 0; a finding of the whole file is "workflow"
        assert_eq!(
            seen,
            [
                (
                    "excessive-permissions",
                    ".github/workflows/ci.yml".to_string(),
                    1,
                    "workflow",
                    Severity::Medium,
                    Confidence::Medium,
                    false
                ),
                (
                    "excessive-permissions",
                    ".github/workflows/ci.yml".to_string(),
                    4,
                    "jobs.build",
                    Severity::Medium,
                    Confidence::Medium,
                    false
                ),
                (
                    "unpinned-uses",
                    ".github/workflows/ci.yml".to_string(),
                    7,
                    "jobs.build.steps[0].uses",
                    Severity::High,
                    Confidence::High,
                    true
                ),
            ]
        );
        assert_eq!(hits[2].feature, "actions/checkout@v4");
    }

    #[test]
    fn a_report_that_is_not_zizmors_or_cannot_be_placed_is_unreadable() {
        let entry = |ident: &str, kind: &str, key: &str, severity: &str| {
            format!(
                r#"[{{"ident":"{ident}","determinations":{{"confidence":"High","severity":"{severity}"}},"locations":[{{"symbolic":{{"key":{key},"kind":"{kind}"}},"concrete":{{"location":{{"start_point":{{"row":0}}}}}}}}]}}]"#
            )
        };
        let local = r#"{"Local":{"verbatim_path":"a.yml"}}"#;
        // the control: this is readable
        assert_eq!(
            parse_report(&entry("unpinned-uses", "Primary", local, "High"))
                .unwrap()
                .len(),
            1
        );
        for bad in [
            String::new(),
            "null".into(),
            "{}".into(),
            "not json".into(),
            // no primary location, not a local file, a severity this mapping does not know
            entry("unpinned-uses", "Related", local, "High"),
            entry("unpinned-uses", "Primary", r#"{"Remote":{}}"#, "High"),
            entry("unpinned-uses", "Primary", local, "Critical"),
        ] {
            assert!(parse_report(&bad).is_err(), "{bad}");
        }
        // an audit this rule does not report is not looked into: a finding it could not place is not this rule's problem
        assert!(parse_report(&entry("artipacked", "Related", local, "High"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn only_a_workflow_or_an_action_definition_outside_other_peoples_directories_is_an_input() {
        for (path, want) in [
            (".github/workflows/ci.yml", true),
            (".github/workflows/release.yaml", true),
            ("action.yml", true),
            ("action.yaml", true),
            (".github/actions/setup/action.yml", true),
            ("tools/ci/action.yml", true),
            // a workflow elsewhere is not run by GitHub; other YAML is not an action; other people's trees are not ours
            ("workflows/ci.yml", false),
            (".github/workflows/sub/ci.yml", false),
            (".github/workflows/README.md", false),
            (".github/dependabot.yml", false),
            ("docs/actions.yml", false),
            ("action.json", false),
            ("node_modules/x/action.yml", false),
            ("vendor/x/.github/workflows/ci.yml", false),
            ("third_party/x/action.yml", false),
        ] {
            assert_eq!(is_input(path), want, "{path}");
        }
        let root = std::env::temp_dir();
        let listing = ".github/workflows/ci.yml\0README.md\0.github/workflows/gone.yml\0";
        assert!(
            select_inputs(listing, &root).is_empty(),
            "a file that is not on disk is not an input"
        );
    }

    #[test]
    fn inputs_are_given_to_zizmor_in_groups_that_fit_a_command_line() {
        let name = |i: usize| OsString::from(format!("/p/{i:04}{}", "x".repeat(1000)));
        let inputs: Vec<_> = (0..40).map(name).collect();
        let groups = chunks(&inputs);
        assert!(groups.len() > 1);
        assert_eq!(groups.iter().map(|g| g.len()).sum::<usize>(), 40);
        assert!(groups
            .iter()
            .all(|g| g.iter().map(|i| i.len() + 1).sum::<usize>() <= ARGS_BUDGET));
        // a single name past the budget is still given, alone
        let long = vec![OsString::from("y".repeat(ARGS_BUDGET + 1))];
        assert_eq!(chunks(&long).len(), 1);
        assert!(chunks(&[]).is_empty());
    }

    #[test]
    fn the_arguments_turn_off_the_repositorys_configuration_and_ignore_comments_and_the_network() {
        let args = check_args(&[OsString::from("/p/a.yml")]);
        let args: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        for flag in [
            "--offline",
            "--no-config",
            "--no-ignores",
            "--no-exit-codes",
            "-q",
        ] {
            assert!(args.iter().any(|a| a == flag), "{flag} in {args:?}");
        }
        // the persona is what makes `permissions: write-all` visible
        let at = args.iter().position(|a| a == "--persona").unwrap();
        assert_eq!(args[at + 1], "pedantic");
        // the inputs come after `--`
        let dashes = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dashes + 1..], ["/p/a.yml"]);
    }

    fn hits(n: usize) -> Result<Vec<Hit>, String> {
        let template = parse_report(REPORT).unwrap()[2].clone();
        Ok(vec![template; n])
    }

    #[test]
    fn exit_0_is_findings_or_clean_and_stderr_beside_it_is_a_partial_read() {
        assert!(
            matches!(judge(Some(0), "", hits(2)), Chunk::Read { hits, unread: None } if hits.len() == 2)
        );
        assert!(
            matches!(judge(Some(0), "", hits(0)), Chunk::Read { hits, unread: None } if hits.is_empty())
        );
        // measured: a workflow with a syntax error beside a valid one is skipped with a warning and zizmor exits 0 with `[]`
        let warn = " WARN zizmor::registry::input: failed to parse input: did not find expected ',' or ']' at line 3 column 6";
        let chunk = judge(Some(0), warn, hits(0));
        assert!(
            matches!(&chunk, Chunk::Read { unread: Some(why), .. } if why.contains("failed to parse")),
            "{chunk:?}"
        );
    }

    #[test]
    fn exit_3_is_nothing_read_and_every_other_code_is_the_tool_failing() {
        let none =
            "WARN failed to parse input: x\nfatal: no audit was performed\nerror: no inputs collected";
        let chunk = judge(Some(3), none, Err("empty".into()));
        assert!(
            matches!(&chunk, Chunk::NothingRead(why) if why.contains("failed to parse")),
            "{chunk:?}"
        );
        // 3 without that message is not the answer it looks like
        assert!(matches!(
            judge(Some(3), "boom", Err("empty".into())),
            Chunk::Failed(ErrorKind::ToolFailed, _)
        ));
        // 1: an input that does not exist or cannot be read as text; 2: usage
        assert!(
            matches!(judge(Some(1), "invalid input: a.yml", Err("e".into())), Chunk::Failed(ErrorKind::ToolFailed, why) if why.contains("invalid input"))
        );
        assert!(matches!(
            judge(Some(2), "usage", Err("e".into())),
            Chunk::Failed(ErrorKind::ToolFailed, _)
        ));
        assert!(matches!(judge(None, "", hits(0)), Chunk::Failed(..)));
        // exit 0 with output that is not a report is unreadable, never clean
        assert!(matches!(
            judge(Some(0), "", Err("not json".into())),
            Chunk::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
    }

    #[test]
    fn the_finding_says_where_what_zizmor_said_and_that_an_ignore_comment_is_not_honoured() {
        let found = parse_report(REPORT).unwrap();
        let unpinned = to_finding(&found[2], "demo", &roots(), None);
        assert_eq!(unpinned.check_id, RULE);
        assert_eq!(
            (
                unpinned.severity,
                unpinned.confidence,
                unpinned.subject.as_deref()
            ),
            (
                Severity::High,
                Confidence::High,
                Some("actions/checkout@v4")
            )
        );
        let at = unpinned.location.as_ref().unwrap();
        assert_eq!(
            (at.file.as_str(), at.line),
            (".github/workflows/ci.yml", Some(7))
        );
        assert!(
            unpinned.summary.contains("not pinned"),
            "{}",
            unpinned.summary
        );
        assert!(
            unpinned.detail.contains("zizmor: ignore") && unpinned.detail.contains("allowlist"),
            "{}",
            unpinned.detail
        );
        // the permission finding quotes zizmor ("no permissions: block"), which reads as an absence claim: it carries its control
        let permission = to_finding(&found[1], "demo", &roots(), Some("Note: x."));
        assert_eq!(permission.subject.as_deref(), Some("jobs.build"));
        assert!(
            permission.detail.ends_with("Note: x."),
            "{}",
            permission.detail
        );
        assert!(permission.positive_control.is_some());
        assert!(
            !permission.detail.contains("ignore"),
            "{}",
            permission.detail
        );
        assert!(permission.validate().is_ok());
    }

    #[test]
    fn text_from_the_workflow_cannot_carry_control_characters_into_a_finding() {
        let mut hit = parse_report(REPORT).unwrap()[2].clone();
        hit.feature = "evil/action@v1\u{1b}[31m\nrm -rf".into();
        let f = to_finding(&hit, "demo", &roots(), None);
        let text = format!("{} {} {:?}", f.summary, f.detail, f.subject);
        assert!(!text.chars().any(char::is_control), "{text:?}");
    }

    #[test]
    fn a_read_problem_is_said_in_the_note_and_nothing_else_has_one() {
        assert_eq!(caveat(None), None);
        let note = caveat(Some(" WARN failed to parse input: x\u{1b}")).unwrap();
        assert!(
            note.contains("failed to parse") && note.contains("not judged"),
            "{note}"
        );
        assert!(!note.chars().any(char::is_control));
    }

    #[test]
    fn the_record_says_what_is_checked_today_and_what_is_not() {
        let catalog = crate::catalog::Catalog::builtin();
        let text = catalog
            .get(RULE)
            .unwrap()
            .checked_today()
            .expect("the record does not say what is checked today");
        assert!(text.contains("pinned"), "{text}");
        assert!(text.contains("permissions"), "{text}");
        assert!(text.contains("Not judged"), "{text}");
    }

    #[test]
    fn a_warning_from_an_audit_is_not_a_partial_read_but_a_skipped_input_is() {
        // measured: zizmor warns about a job's shell for a `pull_request_target` workflow on a self-hosted runner while reading everything
        let audit =
            " WARN zizmor::audit::github_env: couldn't determine shell type for job `build`";
        assert!(matches!(
            judge(Some(0), audit, hits(0)),
            Chunk::Read { unread: None, .. }
        ));
        let both = format!(
            "{audit}\n WARN zizmor::registry::input: failed to validate file://action.yml as action: input does not match"
        );
        assert!(
            matches!(judge(Some(0), &both, hits(0)), Chunk::Read { unread: Some(why), .. } if why.contains("failed to validate"))
        );
        assert_eq!(problem(audit), None);
    }

    #[test]
    fn an_unpinned_image_is_a_finding_about_the_image() {
        let mut hit = parse_report(REPORT).unwrap()[2].clone();
        hit.audit = "unpinned-images".into();
        hit.feature = "    container: node:18".into();
        let f = to_finding(&hit, "demo", &roots(), None);
        assert_eq!(f.subject.as_deref(), Some("container: node:18"));
        assert!(
            f.summary.contains("image") && f.summary.contains("digest"),
            "{}",
            f.summary
        );
        assert!(AUDITS.contains(&"unpinned-images"));
    }
}
