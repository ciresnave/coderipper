// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runs `cargo check --all-targets --message-format=json` and parses rustc's diagnostics, keeping
//! the parts a check needs: the lint code, the primary span's line AND column, and the text of each
//! direct child note (where a `#[must_use = "..."]` reason string shows up).

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// Forward-slash path, relative to the directory cargo ran in.
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Diagnostic {
    pub code: Option<String>,
    pub level: String,
    pub message: String,
    /// `message` of each direct child (notes, helps) in order.
    pub notes: Vec<String>,
    pub spans: Vec<Span>,
}

impl Diagnostic {
    pub fn primary_span(&self) -> Option<&Span> {
        self.spans.iter().find(|s| s.is_primary)
    }

    /// An `error` that is not just a lint promoted by `#![deny(...)]`/`-D warnings`: rustc error
    /// codes look like `E0603`; lint names are lowercase snake_case; a syntax error has no code.
    pub fn is_hard_error(&self) -> bool {
        self.level == "error"
            && match &self.code {
                None => true,
                Some(code) => {
                    code.starts_with('E') && code[1..].chars().all(|c| c.is_ascii_digit())
                }
            }
    }
}

pub(crate) struct BuildOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub success: bool,
    /// The first and last of cargo's own stderr (plain text, not rustc JSON): where a failing build script says why.
    pub stderr_tail: String,
    /// The source file of every target cargo reported a `compiler-artifact` for in THIS package, with forward slashes
    /// and the path cargo printed (absolute). A target that is not in here was not built (e.g. a bin whose
    /// `required-features` are not enabled): nothing in it was compiled, so nothing in it can have been reported.
    pub built_roots: Vec<String>,
    /// The executable of every binary target of THIS package that cargo built (`cargo build`; empty after a `cargo check`).
    pub executables: Vec<Executable>,
    /// Every compilation unit this build reported, fresh or compiled (tests only: nothing in production reads it).
    #[cfg(test)]
    pub units: Vec<UnitReport>,
    /// The note `build_with` raised about the cache for THIS build, if any (tests only; production notes go to the
    /// process-global stats, which parallel tests share).
    #[cfg(test)]
    pub cache_note: Option<String>,
}

/// A binary target cargo built: its name and the file it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Executable {
    /// The target's name (`[[bin]] name`, or the package name for `src/main.rs`).
    pub name: String,
    /// The target's root source file as cargo printed it (absolute, forward slashes).
    pub src_path: String,
    pub path: PathBuf,
}

/// One `compiler-artifact` line of cargo's JSON stream. A lib and its test-mode unit share package id, target name
/// and kind and differ only in `test`, so the key includes it.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitReport {
    pub package_id: String,
    pub target_name: String,
    pub kind: Vec<String>,
    pub test: bool,
    pub fresh: bool,
}

/// How much of cargo's stderr is kept: the first [`STDERR_HEAD_BYTES`] and the last [`STDERR_TAIL_BYTES`]. A failing build script
/// prints `error: failed to run custom build command` first and its (possibly long) stdout before its stderr, so the head holds
/// the line that names the failure and the tail holds the end of a long compile.
const STDERR_HEAD_BYTES: usize = 4 * 1024;
const STDERR_TAIL_BYTES: usize = 12 * 1024;
/// The longest cause printed in an error message.
const CAUSE_CHARS: usize = 400;

/// The part of `stderr` worth keeping (see [`STDERR_HEAD_BYTES`]); the cut between the two parts is marked by a line.
fn keep_ends(stderr: &[u8]) -> String {
    if stderr.len() <= STDERR_HEAD_BYTES + STDERR_TAIL_BYTES {
        return String::from_utf8_lossy(stderr).into_owned();
    }
    let head = String::from_utf8_lossy(&stderr[..STDERR_HEAD_BYTES]);
    let tail = String::from_utf8_lossy(&stderr[stderr.len() - STDERR_TAIL_BYTES..]);
    format!("{head}\n[... cargo's stderr cut here ...]\n{tail}")
}

/// `c` is a control character, or one that reorders or hides the text around it (bidi overrides and isolates).
fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// `text` on one line, without control characters (it is third-party output that reaches a terminal), cut at [`CAUSE_CHARS`].
fn one_line(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if is_unsafe_char(c) { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    // Before the cut: a cut inside a secret would leave half of it, too short for any shape to recognise.
    let flat = crate::redact::scrub(&flat);
    if flat.chars().count() > CAUSE_CHARS {
        let cut: String = flat.chars().take(CAUSE_CHARS).collect();
        format!("{cut}...")
    } else {
        flat
    }
}

/// `text` without ANSI escape sequences: with `CARGO_TERM_COLOR=always` (CI sets it) cargo colours the word `error`, and the line
/// then no longer starts with `error:`.
fn without_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // parameters and intermediates, then one final byte in `@`..=`~`
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    out
}

/// What cargo's own stderr says went wrong: its first `error:` line, then the first two lines of the `--- stderr` block that
/// FOLLOWS it (a failed build script: a `-sys` crate says there what it could not find) or, when there is none, the last line after
/// the final `Caused by:` (a registry or process failure's root cause). Falls back to the last line it wrote.
fn cargo_cause(stderr: &str) -> String {
    let plain = without_ansi(stderr);
    if crate::redact::is_on() {
        if let Some(sentence) = build_script_sentence(&plain) {
            return sentence;
        }
    }
    let lines: Vec<&str> = plain.lines().map(str::trim).collect();
    let mut parts: Vec<&str> = Vec::new();
    let error_at = lines.iter().position(|l| l.starts_with("error:"));
    if let Some(at) = error_at {
        parts.push(lines[at]);
    }
    // Under redaction the `--- stderr` block is the build script's own words and is not repeated.
    let block = lines
        .iter()
        .enumerate()
        .skip(error_at.unwrap_or(0))
        .find(|(_, l)| l.starts_with("--- stderr") && !crate::redact::is_on())
        .map(|(at, _)| at);
    if let Some(at) = block {
        parts.extend(
            lines[at + 1..]
                .iter()
                .filter(|l| !l.is_empty())
                .take(2)
                .copied(),
        );
    } else if let Some(at) = lines.iter().rposition(|l| l.starts_with("Caused by:")) {
        parts.extend(lines[at + 1..].iter().find(|l| !l.is_empty()).copied());
    }
    if parts.is_empty() && crate::redact::is_on() {
        return "cargo failed; its output is withheld".to_string();
    }
    if parts.is_empty() {
        if let Some(last) = lines.iter().rev().find(|l| !l.is_empty()) {
            parts.push(last);
        }
    }
    if parts.is_empty() {
        "cargo exited with an error and wrote nothing to say why".to_string()
    } else {
        one_line(&parts.join(" | "))
    }
}

/// Under redaction, what a failed build script is reported as: a sentence built only from what cargo says about it (the
/// package, its version and the exit status, each checked against a strict grammar and dropped when it does not fit), never from
/// what the script printed. `None` when the failure is not a build script's.
///
/// Only cargo's own line counts: one that starts, unindented, with `error: failed to run custom build command for `. A script's
/// stdout is printed indented under `--- stdout`, and a `cargo:warning=` is printed after `warning: `, so neither can pass for
/// it. A process that writes an unindented line to cargo's stderr can still choose the name and version (at most 128 characters
/// of `[A-Za-z0-9_.+-]`); the exit status is read only from the lines that follow it.
fn build_script_sentence(plain: &str) -> Option<String> {
    const MARK: &str = "error: failed to run custom build command for `";
    let lines: Vec<&str> = plain.lines().collect();
    let at = lines.iter().position(|l| l.starts_with(MARK))?;
    let subject = lines[at][MARK.len()..]
        .split('`')
        .next()
        .unwrap_or_default();
    let mut words = subject.split_whitespace();
    let name = words.next().filter(|n| is_package_name(n));
    let version = words
        .next()
        .and_then(|v| v.strip_prefix('v'))
        .filter(|v| is_version(v));
    // `(exit status: 101)` on Unix, `(exit code: 101)` on Windows; only the digits are kept, and only from the lines right after.
    let status = lines[at + 1..].iter().take(8).find_map(|line| {
        ["(exit status: ", "(exit code: "].iter().find_map(|label| {
            let from = line.find(label)? + label.len();
            let digits: String = line[from..]
                .chars()
                .take_while(char::is_ascii_digit)
                .take(6)
                .collect();
            (!digits.is_empty()).then_some(digits)
        })
    });
    let status = status.map_or(String::new(), |s| format!(" (exit status {s})"));
    Some(match (name, version) {
        (Some(name), Some(version)) => {
            format!("build script of {name} {version} failed{status}; its output is withheld")
        }
        _ => format!("a build script failed{status}; its output is withheld"),
    })
}

fn is_package_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_version(s: &str) -> bool {
    s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
}

impl BuildOutput {
    /// Why this build is broken, in one line for an error message: the first real compiler error (with its code and place), or
    /// what cargo itself reported (a build script that failed, a dependency that could not be built).
    pub fn failure_cause(&self) -> String {
        if let Some(d) = self
            .diagnostics
            .iter()
            .find(|d| d.is_hard_error() && !d.message.starts_with("aborting due to"))
        {
            let code = d
                .code
                .as_deref()
                .map(|c| format!("[{c}]"))
                .unwrap_or_default();
            let place = d
                .primary_span()
                .map(|s| format!(" ({}:{})", s.file, s.line))
                .unwrap_or_default();
            let note = d
                .notes
                .iter()
                .find(|n| !n.trim().is_empty())
                .map(|n| format!(" | {n}"))
                .unwrap_or_default();
            return one_line(&format!("error{code}: {}{place}{note}", d.message));
        }
        cargo_cause(&self.stderr_tail)
    }

    /// True when this build can't be trusted to have reported what a check looked for: a real
    /// compiler error, or a non-zero exit with no diagnostic at all (e.g. a panicking build.rs,
    /// which writes plain text to stderr rather than rustc JSON).
    pub fn is_broken(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_hard_error)
            || (!self.success && self.diagnostics.is_empty())
    }
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    /// The package being compiled (set on `compiler-message` lines).
    package_id: Option<String>,
    /// On `compiler-artifact` lines: true when cargo reused the unit instead of compiling it.
    fresh: Option<bool>,
    target: Option<RawTarget>,
    /// On `compiler-artifact` lines of a binary: the file cargo wrote.
    executable: Option<String>,
    #[cfg(test)]
    profile: Option<RawProfile>,
    message: Option<RawDiagnostic>,
}

#[derive(Deserialize)]
struct RawTarget {
    name: String,
    kind: Vec<String>,
    #[serde(default)]
    src_path: String,
}

#[cfg(test)]
#[derive(Deserialize)]
struct RawProfile {
    test: bool,
}

#[derive(Deserialize)]
struct RawDiagnostic {
    code: Option<RawCode>,
    level: String,
    message: String,
    spans: Vec<RawSpan>,
    children: Vec<RawChild>,
}

#[derive(Deserialize)]
struct RawCode {
    code: String,
}

#[derive(Deserialize)]
struct RawChild {
    message: String,
}

#[derive(Deserialize)]
struct RawSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
    /// Present when the span is inside macro-expanded code; `span` is the macro CALL site.
    expansion: Option<Box<RawExpansion>>,
}

#[derive(Deserialize)]
struct RawExpansion {
    span: RawSpan,
}

impl RawSpan {
    /// rustc reports a lint inside a `macro_rules!` body at the macro DEFINITION, once per
    /// invocation. Follow `expansion` out to the outermost call site, which is the only thing that
    /// tells two invocations apart (and is where `unused_must_use` already points).
    fn into_span(self) -> Span {
        let is_primary = self.is_primary;
        let mut cur = self;
        while let Some(exp) = cur.expansion.take() {
            cur = exp.span;
        }
        Span {
            file: cur.file_name.replace('\\', "/"),
            line: cur.line_start,
            column: cur.column_start,
            is_primary,
        }
    }
}

/// Caps every lint at `warn`. Every check that builds the crate wants this: a `#![deny(warnings)]`,
/// a `[lints]` table or `-D warnings` would otherwise turn a lint the check relies on into an error,
/// fail the build, and stop cargo compiling the crates that depend on the failed one.
pub(crate) const CAP_LINTS: &str = "--cap-lints=warn";

/// `existing` (the caller's own `RUSTFLAGS`) followed by `extra`.
pub(crate) fn compose_rustflags(existing: &str, extra: &str) -> String {
    format!("{existing} {extra}").trim().to_string()
}

/// Checks (`cargo check`: compiles, never links) every target of the package at `root` and parses cargo's JSON diagnostics. No link, because
/// the checks read lints, and a target whose native part is missing (a skipped nvcc under `DOCS_RS=1`) compiles but cannot link. `extra_rustflags`
/// is appended to the caller's `RUSTFLAGS`; `CARGO_ENCODED_RUSTFLAGS` is removed because cargo prefers
/// it and it would drop the flags.
pub(crate) fn build_all_targets(
    root: &Path,
    cache_source: Option<&Path>,
    extra_rustflags: &str,
    features: &crate::check::CargoFeatures,
) -> anyhow::Result<BuildOutput> {
    build_with(
        "check",
        root,
        cache_source,
        "--all-targets",
        extra_rustflags,
        features,
    )
}

/// `cargo build` of the package's binaries (the only build here that writes executables). `executables` names each one.
pub(crate) fn build_bins(
    root: &Path,
    cache_source: Option<&Path>,
    features: &crate::check::CargoFeatures,
) -> anyhow::Result<BuildOutput> {
    build_with("build", root, cache_source, "--bins", "", features)
}

/// Like [`build_all_targets`] but only the library target: bins, tests, examples and benches are not
/// compiled, so a change that breaks them (e.g. downgrading the lib's `pub` items) cannot fail it.
pub(crate) fn build_lib_only(
    root: &Path,
    cache_source: Option<&Path>,
    extra_rustflags: &str,
    features: &crate::check::CargoFeatures,
) -> anyhow::Result<BuildOutput> {
    build_with(
        "check",
        root,
        cache_source,
        "--lib",
        extra_rustflags,
        features,
    )
}

/// `cache_source` is the SOURCE repository the throwaway checkout at `root` was made from: the build cache is
/// keyed by it. `None` means "no cache, build in the checkout's own `target/`", which is what the unit tests want.
fn build_with(
    subcommand: &str,
    root: &Path,
    cache_source: Option<&Path>,
    target_arg: &str,
    extra_rustflags: &str,
    features: &crate::check::CargoFeatures,
) -> anyhow::Result<BuildOutput> {
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    let (package, prefix) = crate::package::locate(root)?;
    // Held until the build ends: another CodeRipper run on this repository waits (boundedly) for it.
    // Inside a session for this repository the session already holds the cache lock through its own handle (a second
    // `try_lock` in this process would answer WouldBlock and cost the whole wait), owns the target directory, and
    // must not freshen (that would defeat the sibling reuse it exists for).
    let session = crate::session::session_for(cache_source);
    let mut choice = match (&session, cache_source) {
        (Some(_), _) | (None, None) => crate::build_cache::CacheChoice::Throwaway { why: None },
        (None, Some(source)) => crate::build_cache::acquire(source),
    };
    // The lock is held, so nothing can write an artifact from here on that predates this refresh. If the checkout
    // cannot be refreshed the cache is NOT used: replacing `choice` drops the `CacheDir` (releasing the lock) before
    // cargo runs, leaves CARGO_TARGET_DIR unset, and the match below raises the one note.
    if matches!(choice, crate::build_cache::CacheChoice::Shared(_)) {
        if let Err(e) = checkout_toplevel(root).and_then(|top| freshen_checkout(&top)) {
            choice = crate::build_cache::CacheChoice::Throwaway {
                why: Some(format!(
                    "build cache unusable ({e}): building without the cache"
                )),
            };
        }
    }
    let mut cargo = Command::new("cargo");
    cargo
        // `-p`: build THIS package even when the workspace's `default-members` name others
        .args([
            subcommand,
            "-p",
            &package.name,
            target_arg,
            "--message-format=json",
        ])
        .args(features.cargo_args())
        .current_dir(root)
        // plain text: the cause is read from cargo's stderr, which colour escapes would break
        .env("CARGO_TERM_COLOR", "never")
        .env("RUSTFLAGS", compose_rustflags(&existing, extra_rustflags))
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    let mut cache_dir = None;
    #[cfg(test)]
    let mut cache_note = None;
    if let Some(session) = &session {
        cargo.env("CARGO_TARGET_DIR", &session.target);
        cache_dir = session.cache_dir.clone();
    }
    match &choice {
        crate::build_cache::CacheChoice::Shared(dir) => {
            cargo.env("CARGO_TARGET_DIR", dir.path.join("target"));
            cache_dir = Some(dir.path.clone());
        }
        crate::build_cache::CacheChoice::Throwaway { why: Some(why) } => {
            crate::build_cache::note(why.clone());
            #[cfg(test)]
            {
                cache_note = Some(why.clone());
            }
        }
        crate::build_cache::CacheChoice::Throwaway { why: None } => {}
    }
    let output = cargo.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr_tail = keep_ends(&output.stderr);
    let (fresh, compiled) = count_units(&stdout);
    crate::build_cache::record(fresh, compiled, cache_dir.as_deref());
    drop(choice);
    let diagnostics = parse_messages_for(&stdout, Some(&package.id));
    Ok(BuildOutput {
        diagnostics: relative_to_package(diagnostics, &prefix),
        success: output.status.success(),
        stderr_tail,
        built_roots: built_roots(&stdout, &package.id),
        executables: executables(&stdout, &package.id),
        #[cfg(test)]
        units: unit_reports(&stdout),
        #[cfg(test)]
        cache_note,
    })
}

/// The `src_path` of every target of `package_id` that cargo reported a `compiler-artifact` for, slashes forward, sorted.
fn built_roots(stdout: &str, package_id: &str) -> Vec<String> {
    let mut roots: Vec<String> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-artifact" && m.package_id.as_deref() == Some(package_id))
        .filter_map(|m| m.target)
        .map(|t| t.src_path.replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// The binary targets of `package_id` that cargo wrote an executable for, sorted by name.
fn executables(stdout: &str, package_id: &str) -> Vec<Executable> {
    let mut found: Vec<Executable> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-artifact" && m.package_id.as_deref() == Some(package_id))
        .filter_map(|m| {
            let target = m.target?;
            let path = m.executable.filter(|e| !e.is_empty())?;
            target.kind.iter().any(|k| k == "bin").then(|| Executable {
                name: target.name,
                src_path: target.src_path.replace('\\', "/"),
                path: PathBuf::from(path),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found.dedup();
    found
}

/// Every `compiler-artifact` line as a [`UnitReport`].
#[cfg(test)]
fn unit_reports(stdout: &str) -> Vec<UnitReport> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-artifact")
        .filter_map(|m| {
            let target = m.target?;
            Some(UnitReport {
                package_id: m.package_id.unwrap_or_default(),
                target_name: target.name,
                kind: target.kind,
                test: m.profile.is_some_and(|p| p.test),
                fresh: m.fresh == Some(true),
            })
        })
        .collect()
}

/// Sets the mtime of every file of the checkout at `toplevel` (everything except `.git`) to now.
///
/// Cargo calls a unit fresh when its source files are not newer than the artifact in the target directory, and a unit's
/// hash does not depend on the checkout's path. A checkout made before ANOTHER run finished building into the same
/// shared cache therefore has files older than that run's artifact and would be served its compile (and its rewrite).
/// Called right after the cache lock is taken, so no later artifact can predate it. A fresh checkout already has
/// every in-repo sibling rebuilt per run, so nothing reusable is lost.
pub(crate) fn freshen_checkout(toplevel: &Path) -> anyhow::Result<()> {
    let now = std::time::SystemTime::now();
    freshen_with(toplevel, &|path| {
        #[cfg(test)]
        if FAIL_ON.with(|f| {
            f.borrow()
                .as_deref()
                .is_some_and(|name| path.file_name().is_some_and(|n| n == name))
        }) {
            return Err(std::io::Error::other("injected failure"));
        }
        stamp(path, now)
    })
}

/// Stamps `path` with `now`, clearing and restoring a read-only attribute when that is what stands in the way.
fn stamp(path: &Path, now: std::time::SystemTime) -> std::io::Result<()> {
    let open = || std::fs::OpenOptions::new().write(true).open(path);
    match open() {
        Ok(file) => file.set_modified(now),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let mut perms = std::fs::metadata(path)?.permissions();
            if !perms.readonly() {
                return Err(e);
            }
            #[allow(
                clippy::permissions_set_readonly_false,
                reason = "the lint is about Unix mode bits; this clears the Windows read-only attribute so the mtime can be set"
            )]
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms.clone())?;
            let result = open().and_then(|file| file.set_modified(now));
            perms.set_readonly(true);
            std::fs::set_permissions(path, perms)?;
            result
        }
        Err(e) => Err(e),
    }
}

/// [`freshen_checkout`] with the stamping injected. Recursive; skips `.git` (directory or file); never follows a
/// symlink (a link is skipped, so it cannot stamp a file outside the checkout). The first file that cannot be
/// stamped is an error naming it.
fn freshen_with(
    toplevel: &Path,
    stamp: &dyn Fn(&Path) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    let mut dirs = vec![toplevel.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() {
                stamp(&path)
                    .map_err(|e| anyhow::anyhow!("cannot refresh {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

/// The checkout's top level, from the package directory `build_with` is given.
fn checkout_toplevel(root: &Path) -> anyhow::Result<PathBuf> {
    let output = crate::github::git_command(root, &["rev-parse", "--show-toplevel"]).output()?;
    anyhow::ensure!(
        output.status.success(),
        "cannot find the checkout's top level from {}",
        root.display()
    );
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

// Tests only: make `freshen_checkout` fail for a file with this name.
#[cfg(test)]
thread_local! {
    static FAIL_ON: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// `(fresh, compiled)`: how many compilation units cargo reused and how many it built, from the
/// `compiler-artifact` lines of its JSON stream.
fn count_units(stdout: &str) -> (u64, u64) {
    let (mut fresh, mut compiled) = (0, 0);
    for message in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-artifact")
    {
        if message.fresh == Some(true) {
            fresh += 1;
        } else {
            compiled += 1;
        }
    }
    (fresh, compiled)
}

/// cargo names files relative to the WORKSPACE root (`b/src/lib.rs` for a member `b`). Checks think in
/// package-relative paths (`src/lib.rs`), so strip the member's prefix.
fn relative_to_package(mut diagnostics: Vec<Diagnostic>, prefix: &str) -> Vec<Diagnostic> {
    for d in &mut diagnostics {
        for span in &mut d.spans {
            if let Some(rest) = span.file.strip_prefix(prefix) {
                span.file = rest.to_string();
            }
        }
    }
    diagnostics
}

/// Parses cargo's JSON stream (one object per line), keeping only `compiler-message` entries.
#[cfg(test)]
pub(crate) fn parse_messages(stdout: &str) -> Vec<Diagnostic> {
    parse_messages_for(stdout, None)
}

/// Like [`parse_messages`], but with `Some(package_id)` keeps only what that package's own compilation
/// said — plus every ERROR, wherever it came from, because an error anywhere breaks the build.
/// (A sibling or path-dependency member that cargo builds along the way warns too; those warnings
/// are not this package's, whatever directory their files sit in.)
pub(crate) fn parse_messages_for(stdout: &str, package_id: Option<&str>) -> Vec<Diagnostic> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-message")
        .filter(|m| {
            let is_error = m.message.as_ref().is_some_and(|d| d.level == "error");
            is_error || package_id.is_none_or(|id| m.package_id.as_deref() == Some(id))
        })
        .filter_map(|m| m.message)
        .map(|raw| Diagnostic {
            code: raw.code.map(|c| c.code),
            level: raw.level,
            message: raw.message,
            notes: raw.children.into_iter().map(|c| c.message).collect(),
            spans: raw.spans.into_iter().map(RawSpan::into_span).collect(),
        })
        .collect()
}

#[cfg(test)]
mod cross_run_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"reason":"compiler-message","message":{"code":{"code":"unused_must_use","explanation":null},"level":"warning","message":"unused return value of `f` that must be used","spans":[{"file_name":"src\\lib.rs","line_start":7,"column_start":5,"is_primary":true}],"children":[{"message":"CR:3"},{"message":"use `let _ = ...` to ignore the resulting value"}]}}"#;

    #[test]
    fn a_compiler_message_keeps_code_span_column_and_child_notes() {
        let parsed = parse_messages(LINE);
        assert_eq!(parsed.len(), 1);
        let d = &parsed[0];
        assert_eq!(d.code.as_deref(), Some("unused_must_use"));
        assert_eq!(d.notes[0], "CR:3");
        let span = d.primary_span().unwrap();
        assert_eq!(
            (span.file.as_str(), span.line, span.column),
            ("src/lib.rs", 7, 5)
        );
    }

    #[test]
    fn a_span_inside_a_macro_expansion_resolves_to_the_invocation_site() {
        let line = r#"{"reason":"compiler-message","message":{"code":{"code":"deprecated","explanation":null},"level":"warning","message":"use of deprecated function `f`: CR:1","spans":[{"file_name":"src/main.rs","line_start":3,"column_start":29,"is_primary":true,"expansion":{"span":{"file_name":"src/main.rs","line_start":4,"column_start":22,"is_primary":false,"expansion":null},"macro_decl_name":"call!"}}],"children":[]}}"#;
        let span = parse_messages(line)[0].primary_span().unwrap().clone();
        assert_eq!((span.line, span.column), (4, 22));
        assert!(span.is_primary);
    }

    #[test]
    fn rustflags_are_appended_to_the_callers_without_a_stray_space() {
        assert_eq!(
            compose_rustflags("", "--cap-lints=warn"),
            "--cap-lints=warn"
        );
        assert_eq!(
            compose_rustflags("-C target-cpu=native", "--cap-lints=warn"),
            "-C target-cpu=native --cap-lints=warn"
        );
    }

    fn workspace(a_lib: &str, b_lib: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let files = [
            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n"),
            ("a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            ("a/src/lib.rs", a_lib),
            (
                "b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\" }\n",
            ),
            ("b/src/lib.rs", b_lib),
        ];
        for (name, contents) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        tmp
    }

    #[test]
    fn a_member_build_reports_package_relative_paths_and_only_its_own_diagnostics() {
        // `a` warns too (it is built as b's dependency); that warning is not b's.
        let ws = workspace(
            "pub fn a_warn(x: i32) {}\n",
            "pub fn b_warn(y: i32) { a::a_warn(1); }\n",
        );
        let out =
            build_all_targets(&ws.path().join("b"), None, CAP_LINTS, &Default::default()).unwrap();
        let unused: Vec<_> = out
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("unused_variables"))
            .map(|d| {
                let span = d.primary_span().unwrap();
                (span.file.clone(), d.message.clone())
            })
            .collect();
        // (`--all-targets` compiles the lib twice, so the same warning can appear twice)
        assert!(!unused.is_empty(), "b's own warning must be reported");
        assert!(
            unused
                .iter()
                .all(|(file, message)| file == "src/lib.rs" && message.contains('y')),
            "{unused:?}"
        );
    }

    #[test]
    fn a_dependency_that_does_not_compile_still_breaks_the_build() {
        // Only WARNINGS from outside the package are dropped; an error anywhere must still count.
        let ws = workspace(
            "pub fn a_broken() { let x: i32 = \"no\"; }\n",
            "pub fn b() {}\n",
        );
        let out =
            build_all_targets(&ws.path().join("b"), None, CAP_LINTS, &Default::default()).unwrap();
        assert!(out.is_broken(), "{:?}", out.diagnostics.len());
    }

    #[test]
    fn a_target_that_cannot_link_does_not_break_the_analysis() {
        // A bin whose native symbol is never defined (what a skipped nvcc/build script leaves under `DOCS_RS=1`) compiles but
        // cannot link. The checks read lints, so they must not need a link.
        let tmp = tempfile::tempdir().unwrap();
        let files = [
            (
                "Cargo.toml",
                "[package]
name = \"p\"
version = \"0.1.0\"
edition = \"2021\"
",
            ),
            (
                "src/lib.rs",
                "pub fn warn(x: i32) {}
",
            ),
            (
                "src/main.rs",
                "extern \"C\" { fn coderipper_undefined_native_symbol(); }
fn main() { unsafe { coderipper_undefined_native_symbol() } }
",
            ),
        ];
        for (name, contents) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let out = build_all_targets(tmp.path(), None, CAP_LINTS, &Default::default()).unwrap();
        assert!(!out.is_broken(), "{}", out.stderr_tail);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("unused_variables")),
            "the lint must still be reported"
        );
    }

    #[test]
    fn non_message_lines_and_garbage_are_skipped() {
        let stdout = format!("{{\"reason\":\"compiler-artifact\"}}\nnot json\n{LINE}\n");
        assert_eq!(parse_messages(&stdout).len(), 1);
    }

    fn diag(level: &str, code: Option<&str>) -> Diagnostic {
        Diagnostic {
            code: code.map(str::to_string),
            level: level.into(),
            message: String::new(),
            notes: vec![],
            spans: vec![],
        }
    }

    #[test]
    fn a_denied_lint_is_not_a_hard_error_but_a_real_error_code_is() {
        assert!(!diag("error", Some("deprecated")).is_hard_error());
        assert!(!diag("warning", None).is_hard_error());
        assert!(diag("error", Some("E0603")).is_hard_error());
        assert!(diag("error", None).is_hard_error());
    }

    #[test]
    fn units_are_counted_fresh_or_compiled_from_artifact_lines() {
        let stdout = concat!(
            r#"{"reason":"compiler-artifact","package_id":"a 0.1.0","fresh":true}"#,
            "\n",
            r#"{"reason":"compiler-artifact","package_id":"b 0.1.0","fresh":false}"#,
            "\n",
            r#"{"reason":"compiler-artifact","package_id":"c 0.1.0","fresh":false}"#,
            "\n",
            r#"{"reason":"build-script-executed","package_id":"c 0.1.0"}"#,
            "\n",
            "not json at all\n",
        );
        assert_eq!(count_units(stdout), (1, 2));
    }

    #[test]
    fn a_failed_build_with_no_diagnostics_is_broken() {
        let out = BuildOutput {
            diagnostics: vec![],
            success: false,
            stderr_tail: String::new(),
            units: vec![],
            built_roots: vec![],
            executables: vec![],
            cache_note: None,
        };
        assert!(out.is_broken());
        let ok = BuildOutput {
            diagnostics: vec![],
            success: true,
            stderr_tail: String::new(),
            units: vec![],
            built_roots: vec![],
            executables: vec![],
            cache_note: None,
        };
        assert!(!ok.is_broken());
    }

    #[test]
    fn a_failed_build_script_is_named_with_what_it_printed_to_stderr() {
        let stderr = "   Compiling openssl-sys v0.9.117\n\
            error: failed to run custom build command for `openssl-sys v0.9.117`\n\n\
            Caused by:\n  process didn't exit successfully: `build-script-build` (exit code: 101)\n  --- stdout\n  cargo:rerun-if-env-changed=X\n\n  --- stderr\n  thread 'main' panicked at build.rs:3:\n  Could not find directory of OpenSSL installation\n";
        let cause = cargo_cause(stderr);
        assert!(
            cause.starts_with("error: failed to run custom build command"),
            "{cause}"
        );
        assert!(
            cause.contains("Could not find directory of OpenSSL installation"),
            "{cause}"
        );
        assert!(!cause.contains('\n'));
    }

    #[test]
    fn a_coloured_stderr_is_read_like_a_plain_one() {
        // CARGO_TERM_COLOR=always (CI): the word `error` is wrapped in escapes
        let stderr = "\u{1b}[1m\u{1b}[31merror\u{1b}[0m\u{1b}[1m: failed to run custom build command for `x v1`\u{1b}[0m\n  --- stderr\n  \u{1b}[31mthe real reason\u{1b}[0m\n";
        let cause = cargo_cause(stderr);
        assert!(
            cause.starts_with("error: failed to run custom build command"),
            "{cause:?}"
        );
        assert!(
            cause.contains("the real reason") && !cause.contains('\u{1b}'),
            "{cause:?}"
        );
    }

    #[test]
    fn the_stderr_block_that_belongs_to_the_first_error_is_the_one_used() {
        // an earlier warning block of another crate must not lend its text
        let stderr = "--- stderr\nunrelated earlier noise\nerror: failed to run custom build command for `a v1`\n  --- stderr\n  a's own reason\n";
        let cause = cargo_cause(stderr);
        assert!(cause.contains("a's own reason"), "{cause}");
        assert!(!cause.contains("unrelated earlier noise"), "{cause}");
    }

    #[test]
    fn a_failure_with_no_stderr_block_gives_its_root_cause() {
        let stderr = "error: failed to get `serde` as a dependency\n\nCaused by:\n  failed to load source\n\nCaused by:\n  network is unreachable\n";
        let cause = cargo_cause(stderr);
        assert!(
            cause.contains("failed to get `serde`") && cause.contains("network is unreachable"),
            "{cause}"
        );
    }

    #[test]
    fn nothing_written_is_said_as_such_and_the_last_line_is_the_fallback() {
        assert!(cargo_cause("").contains("wrote nothing"));
        assert_eq!(
            cargo_cause("warning: x\nlinker blew up\n"),
            "linker blew up"
        );
    }

    #[test]
    fn a_cause_is_one_line_without_control_or_bidi_characters_and_is_cut() {
        let text = format!("error: a\u{1b}[31mb\n\u{202e}c{}", "x".repeat(600));
        let cause = one_line(&text);
        assert!(
            !cause.chars().any(|c| c.is_control() || c == '\u{202e}'),
            "{cause:?}"
        );
        assert!(cause.ends_with("..."));
        assert!(cause.chars().count() <= CAUSE_CHARS + 3);
    }

    #[test]
    fn a_long_stderr_keeps_its_first_error_line_and_its_end() {
        let mut big = b"error: failed to run custom build command for `x v1`\n".to_vec();
        big.extend(std::iter::repeat_n(b'.', 40 * 1024));
        big.extend_from_slice(b"\n  --- stderr\n  the end says why\n");
        let kept = keep_ends(&big);
        assert!(kept.starts_with("error: failed to run custom build command"));
        assert!(kept.contains("the end says why"));
        assert!(kept.len() < 20 * 1024);
    }

    #[test]
    fn rustcs_own_trailer_is_not_the_cause_and_the_first_note_is_carried() {
        let mut trailer = diag("error", None);
        trailer.message = "aborting due to 1 previous error".into();
        let mut real = diag("error", Some("E0425"));
        real.message = "cannot find value `x` in this scope".into();
        real.notes = vec!["proc macro panicked: boom".into()];
        let out = BuildOutput {
            diagnostics: vec![trailer, real],
            success: false,
            stderr_tail: String::new(),
            units: vec![],
            built_roots: vec![],
            executables: vec![],
            cache_note: None,
        };
        let cause = out.failure_cause();
        assert!(
            cause.contains("E0425") && cause.contains("proc macro panicked: boom"),
            "{cause}"
        );
        assert!(!cause.contains("aborting due to"), "{cause}");
    }

    fn secret_token() -> String {
        ["ghp_", &"aB3dE5fG7h".repeat(4)].concat()
    }

    #[test]
    fn a_cause_is_redacted_before_it_is_cut_and_only_when_the_policy_is_on() {
        use crate::redact::{with_policy, RedactionPolicy};
        let token = secret_token();
        // the token straddles the cut: its first characters sit before CAUSE_CHARS and the rest after
        let filler = "x".repeat(CAUSE_CHARS - 10);
        let text = format!("{filler} {token} tail");
        with_policy(RedactionPolicy::Off, || {
            assert!(one_line(&text).contains("ghp_aB3"), "off: shown as it came");
        });
        with_policy(RedactionPolicy::On, || {
            let cause = one_line(&text);
            assert!(!cause.contains("ghp_"), "{cause}");
            // the cut falls inside the placeholder, which is fine: no part of the token is left
            assert!(cause.contains("<redacted"), "{cause}");
        });
    }

    #[test]
    fn an_escape_sequence_inside_a_token_does_not_hide_it() {
        use crate::redact::{with_policy, RedactionPolicy};
        let token = secret_token();
        let (head, rest) = token.split_at(5);
        let stderr = format!("error: build failed: key {head}\u{1b}[0m{rest} end\n");
        with_policy(RedactionPolicy::On, || {
            let cause = cargo_cause(&stderr);
            assert!(!cause.contains(rest), "{cause}");
            assert!(cause.starts_with("error: build failed"), "{cause}");
        });
    }

    #[test]
    fn a_failed_build_script_is_a_fixed_sentence_under_redaction() {
        use crate::redact::{with_policy, RedactionPolicy};
        let stderr = format!(
            "error: failed to run custom build command for `foo-sys v0.1.0 (/tmp/x/foo-sys)`\n\nCaused by:\n  process didn't \
             exit successfully: `/tmp/build-script-build` (exit status: 101)\n  --- stdout\n  cargo:warning=TOKEN={}\n  --- stderr\n  \
             thread 'main' panicked at build.rs:1: {}\n",
            "aB3dE5fG7h".repeat(2),
            secret_token()
        );
        with_policy(RedactionPolicy::Off, || {
            assert!(
                cargo_cause(&stderr).contains("panicked at build.rs"),
                "off: unchanged"
            );
        });
        with_policy(RedactionPolicy::On, || {
            assert_eq!(
                cargo_cause(&stderr),
                "build script of foo-sys 0.1.0 failed (exit status 101); its output is withheld"
            );
        });
    }

    #[test]
    fn a_build_script_name_that_does_not_fit_the_grammar_is_dropped_not_echoed() {
        use crate::redact::{with_policy, RedactionPolicy};
        let stderr =
            "error: failed to run custom build command for `ev\u{202e}il v1.0`\n  (exit code: 1)\n";
        with_policy(RedactionPolicy::On, || {
            assert_eq!(
                cargo_cause(stderr),
                "a build script failed (exit status 1); its output is withheld"
            );
        });
        let stderr = "error: failed to run custom build command for `pkg v1.0;rm`";
        with_policy(RedactionPolicy::On, || {
            assert_eq!(
                cargo_cause(stderr),
                "a build script failed; its output is withheld"
            );
        });
    }

    #[test]
    fn a_forged_marker_in_a_scripts_output_is_not_cargos_line() {
        use crate::redact::{with_policy, RedactionPolicy};
        let forged = "AAAA_forged_payload_0123456789 v9.9.9-forged";
        // a warning a successful dependency's script printed, then cargo's own line
        let stderr = format!(
            "warning: dep@1.0.0: failed to run custom build command for `{forged}`\n\
             error: failed to run custom build command for `real v1.2.3 (/x)`\n\nCaused by:\n  process didn't exit successfully: \
             `b` (exit status: 4)\n"
        );
        with_policy(RedactionPolicy::On, || {
            let cause = cargo_cause(&stderr);
            assert_eq!(
                cause,
                "build script of real 1.2.3 failed (exit status 4); its output is withheld"
            );
        });
        // cargo's own line was in the part cut out: the script's indented words are all that is left
        let stderr = format!(
            "  cargo:warning=error: failed to run custom build command for `{forged}`\n  --- stderr\n  secret words here\n"
        );
        with_policy(RedactionPolicy::On, || {
            let cause = cargo_cause(&stderr);
            assert_eq!(cause, "cargo failed; its output is withheld");
        });
    }

    #[test]
    fn a_cargo_failure_that_is_not_a_build_script_is_kept_under_redaction() {
        use crate::redact::{with_policy, RedactionPolicy};
        let stderr = "error: failed to select a version for `serde`\n";
        with_policy(RedactionPolicy::On, || {
            assert_eq!(
                cargo_cause(stderr),
                "error: failed to select a version for `serde`"
            );
        });
    }
}
