//! Runs `cargo build --all-targets --message-format=json` and parses rustc's diagnostics, keeping
//! the parts a check needs: the lint code, the primary span's line AND column, and the text of each
//! direct child note (where a `#[must_use = "..."]` reason string shows up).

use serde::Deserialize;
use std::path::Path;
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
}

impl BuildOutput {
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
    message: Option<RawDiagnostic>,
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

/// Builds every target of the package at `root` and parses cargo's JSON diagnostics. `extra_rustflags`
/// is appended to the caller's `RUSTFLAGS`; `CARGO_ENCODED_RUSTFLAGS` is removed because cargo prefers
/// it and it would drop the flags.
pub(crate) fn build_all_targets(
    root: &Path,
    cache_source: Option<&Path>,
    extra_rustflags: &str,
) -> anyhow::Result<BuildOutput> {
    build_with(root, cache_source, "--all-targets", extra_rustflags)
}

/// Like [`build_all_targets`] but only the library target: bins, tests, examples and benches are not
/// compiled, so a change that breaks them (e.g. downgrading the lib's `pub` items) cannot fail it.
pub(crate) fn build_lib_only(
    root: &Path,
    cache_source: Option<&Path>,
    extra_rustflags: &str,
) -> anyhow::Result<BuildOutput> {
    build_with(root, cache_source, "--lib", extra_rustflags)
}

/// `cache_source` is the SOURCE repository the throwaway checkout at `root` was made from: the build cache is
/// keyed by it. `None` means "no cache, build in the checkout's own `target/`", which is what the unit tests want.
fn build_with(
    root: &Path,
    cache_source: Option<&Path>,
    target_arg: &str,
    extra_rustflags: &str,
) -> anyhow::Result<BuildOutput> {
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    let (package, prefix) = crate::package::locate(root)?;
    // Held until the build ends: another CodeRipper run on this repository waits (boundedly) for it.
    let choice = match cache_source {
        Some(source) => crate::build_cache::acquire(source),
        None => crate::build_cache::CacheChoice::Throwaway { why: None },
    };
    let mut cargo = Command::new("cargo");
    cargo
        // `-p`: build THIS package even when the workspace's `default-members` name others
        .args([
            "build",
            "-p",
            &package.name,
            target_arg,
            "--message-format=json",
        ])
        .current_dir(root)
        .env("RUSTFLAGS", compose_rustflags(&existing, extra_rustflags))
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    let mut cache_dir = None;
    match &choice {
        crate::build_cache::CacheChoice::Shared(dir) => {
            cargo.env("CARGO_TARGET_DIR", dir.path.join("target"));
            cache_dir = Some(dir.path.clone());
        }
        crate::build_cache::CacheChoice::Throwaway { why: Some(why) } => {
            crate::build_cache::note(why.clone())
        }
        crate::build_cache::CacheChoice::Throwaway { why: None } => {}
    }
    let output = cargo.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (fresh, compiled) = count_units(&stdout);
    crate::build_cache::record(fresh, compiled, cache_dir.as_deref());
    drop(choice);
    let diagnostics = parse_messages_for(&stdout, Some(&package.id));
    Ok(BuildOutput {
        diagnostics: relative_to_package(diagnostics, &prefix),
        success: output.status.success(),
    })
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
        let out = build_all_targets(&ws.path().join("b"), None, CAP_LINTS).unwrap();
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
        let out = build_all_targets(&ws.path().join("b"), None, CAP_LINTS).unwrap();
        assert!(out.is_broken(), "{:?}", out.diagnostics.len());
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
        };
        assert!(out.is_broken());
        let ok = BuildOutput {
            diagnostics: vec![],
            success: true,
        };
        assert!(!ok.is_broken());
    }
}
