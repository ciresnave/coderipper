// SPDX-License-Identifier: MIT OR Apache-2.0
//! Size and coupling outliers (project scope, Rust): which files and types are far larger or more entangled than the rest of the
//! codebases we keep?
//!
//! Rule record: `MOD-005` in `rules/mod.toml` (the rule catalog). Background in `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.1.
//!
//! **What it measures** (read from `syn`, at `HEAD`, from the tracked `.rs` files under the package's `src/`; see `metrics`):
//! - per file: code lines (not blank, not a `//` line, not inside a `#[cfg(test)]`/`#[test]` item), the other modules of the crate the
//!   file names (fan-out), and how many other files name its module (fan-in);
//! - per type: its fields plus the methods of its inherent `impl` blocks (a type with no inherent method is a record and is not reported).
//!
//! **How the threshold is chosen:** the codebases we keep, pooled, not the project alone. A project-relative percentile would call a
//! 5,000-line file ordinary in the project that most needs the finding. The figures and the corpus they came from are in
//! [`thresholds`] and the design note; a finding states the value, the threshold, and where the threshold came from.
//!
//! **Known limitations / deliberate scope:**
//! - A size is a cue to inspect, never a verdict: a long file of one cohesive table is fine. The finding says "inspect", and the
//!   Low band needs size **and** high coupling.
//! - Syntactic only (see `metrics`): macro-generated items, glob-import targets, `#[path]` modules and a type name that two structs
//!   share are not seen; the last is counted in a note.
//! - Test code is a different population and is not measured: `#[cfg(test)]`/`#[test]` items, and files named `tests.rs`, `test.rs`,
//!   `*_test.rs`, `*_tests.rs`. This is a kind of code, not a path exclusion of the rule. `tests/`, `benches/`, `examples/` and
//!   `build.rs` are outside `src/` and are not read. A file under `src/bin/` is measured for size; its coupling is not resolved.
//! - Growth over time needs history and is not measured.
//! - Analysis is HEAD-only, one package at a time, as with the other checks.

mod metrics;
mod thresholds;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use metrics::{FileMetrics, SourceFile};
use std::path::Path;
use std::process::Command;
use thresholds::{Thresholds, CORPUS};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_005;

/// Reports files and types far above the corpus's typical size and coupling (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct SizeCouplingCheck;

impl SizeCouplingCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for SizeCouplingCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let (package, _) = crate::package::locate(&ctx.project_root)?;
        let files = read_sources(&package.dir)?;
        let (measured, notes) = metrics::measure(&files)?;
        let project = package.dir.file_name().map_or_else(
            || package.name.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut findings = judge(&measured, &CORPUS, &project);
        findings.sort_by(|a, b| {
            let key = |f: &Finding| {
                f.location
                    .as_ref()
                    .map(|l| (l.file.clone(), l.line))
                    .unwrap_or_default()
            };
            key(a).cmp(&key(b)).then_with(|| a.subject.cmp(&b.subject))
        });
        Ok((findings, notes))
    }
}

/// The tracked Rust files under `src/` at `HEAD`, minus files of test code.
fn read_sources(dir: &Path) -> anyhow::Result<Vec<SourceFile>> {
    let listing = git(
        dir,
        &["ls-tree", "-r", "-z", "--name-only", "HEAD", "--", "src"],
    )?;
    let mut files = Vec::new();
    for path in String::from_utf8_lossy(&listing).split('\0') {
        if path.is_empty() || !path.ends_with(".rs") || metrics::is_test_file(path) {
            continue;
        }
        let blob = git(dir, &["show", &format!("HEAD:./{path}")])?;
        files.push(SourceFile {
            path: path.to_string(),
            text: String::from_utf8(blob)
                .map_err(|_| anyhow::anyhow!("{path} is not valid UTF-8"))?,
        });
    }
    Ok(files)
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// The findings for these measurements against these thresholds.
fn judge(files: &[FileMetrics], t: &Thresholds, project: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in files {
        let (fo, fi) = (f.fan_out.len(), f.fan_in);
        let coupled = fo >= t.fan_out_coupled || fi >= t.fan_in_coupled;
        if f.code_lines >= t.file_lines_high && coupled {
            out.push(file_finding(
                f,
                project,
                Severity::Low,
                "size and coupling",
                &[
                    (f.code_lines >= t.file_lines_high).then(|| {
                        format!(
                            "{} code lines (threshold {})",
                            f.code_lines, t.file_lines_high
                        )
                    }),
                    (fo >= t.fan_out_coupled).then(|| {
                        format!("names {fo} other modules (threshold {})", t.fan_out_coupled)
                    }),
                    (fi >= t.fan_in_coupled).then(|| {
                        format!(
                            "is named by {fi} other files (threshold {})",
                            t.fan_in_coupled
                        )
                    }),
                ],
                t,
            ));
        } else if f.code_lines >= t.file_lines {
            out.push(file_finding(
                f,
                project,
                Severity::Info,
                "size",
                &[Some(format!(
                    "{} code lines (threshold {})",
                    f.code_lines, t.file_lines
                ))],
                t,
            ));
        } else if fo >= t.fan_out || fi >= t.fan_in {
            out.push(file_finding(
                f,
                project,
                Severity::Info,
                "coupling",
                &[
                    (fo >= t.fan_out)
                        .then(|| format!("names {fo} other modules (threshold {})", t.fan_out)),
                    (fi >= t.fan_in)
                        .then(|| format!("is named by {fi} other files (threshold {})", t.fan_in)),
                ],
                t,
            ));
        }
        for ty in &f.types {
            let members = ty.fields + ty.methods;
            // a record of fields with no behaviour of its own (a wire format, a key) has no responsibilities to separate
            if members >= t.type_members && ty.methods > 0 {
                out.push(Finding {
                    check_id: CHECK_ID.into(),
                    claim: Claim::Presence,
                    severity: Severity::Info,
                    confidence: Confidence::Medium,
                    project: project.into(),
                    location: Some(Location { file: f.path.clone(), line: Some(ty.line as u32) }),
                    subject: Some(ty.name.clone()),
                    // "the `X`-named type", not "type `X`": the host reads a bare `None`, `No` or `Zero` in a summary as an absence claim
                    summary: format!(
                        "the `{}`-named type has {members} members (threshold {})",
                        ty.name, t.type_members
                    ),
                    detail: format!(
                        "Measured: {} fields and {} methods in inherent `impl` blocks (trait-impl methods are dictated by the \
                         trait and are not counted). {} A large type is a cue to inspect, not a verdict: if its fields and \
                         methods fall into groups that never touch each other, split it along them; if they all move together, \
                         leave it and say why in an allowlist reason. Syntactic: methods added by a macro or by an `impl` in \
                         another crate are not seen.",
                        ty.fields,
                        ty.methods,
                        t.basis(),
                    ),
                    positive_control: None,
                    member: None,
                });
            }
        }
    }
    out
}

fn file_finding(
    f: &FileMetrics,
    project: &str,
    severity: Severity,
    what: &str,
    over: &[Option<String>],
    t: &Thresholds,
) -> Finding {
    // The summary names only the figures that crossed a threshold (so none is zero, which the host would read as an absence claim);
    // the detail gives all three.
    let over: Vec<&str> = over.iter().flatten().map(String::as_str).collect();
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity,
        confidence: Confidence::Medium,
        project: project.into(),
        location: Some(Location { file: f.path.clone(), line: Some(1) }),
        subject: Some(f.path.clone()),
        summary: format!("`{}` is a {what} outlier: {}", f.path, over.join(", ")),
        detail: format!(
            "Measured: {} code lines (excluding blank lines, `//` lines and test items), {} other modules named, {} other files naming this module. {}              Size is a cue to inspect, never a verdict: a long file of one cohesive table is fine. Look for parts that change for              different reasons or that use different state, and move each to its own module; if the file is cohesive, leave it and              record why in an allowlist reason. Syntactic: macro-generated code, glob-import targets and `#[path]` modules are not              seen, so coupling is a lower bound.",
            f.code_lines,
            f.fan_out.len(),
            f.fan_in,
            t.basis()
        ),
        positive_control: None,
        member: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const T: Thresholds = Thresholds {
        file_lines: 10,
        file_lines_high: 20,
        fan_out: 5,
        fan_in: 5,
        fan_out_coupled: 3,
        fan_in_coupled: 3,
        type_members: 4,
        source: "test thresholds",
    };

    fn file(lines: usize, fan_out: usize, fan_in: usize) -> FileMetrics {
        FileMetrics {
            path: "src/x.rs".into(),
            code_lines: lines,
            fan_out: (0..fan_out)
                .map(|i| format!("m{i}"))
                .collect::<BTreeSet<_>>(),
            fan_in,
            types: vec![],
        }
    }

    #[test]
    fn a_file_under_every_threshold_has_no_finding() {
        assert!(judge(&[file(9, 2, 2)], &T, "p").is_empty());
    }

    #[test]
    fn size_alone_is_info_and_size_with_coupling_is_low() {
        let at = |f: FileMetrics| judge(&[f], &T, "p").first().map(|f| f.severity);
        assert_eq!(at(file(10, 0, 0)), Some(Severity::Info));
        assert_eq!(
            at(file(25, 0, 0)),
            Some(Severity::Info),
            "huge but uncoupled stays Info"
        );
        assert_eq!(at(file(25, 3, 0)), Some(Severity::Low));
        assert_eq!(at(file(25, 0, 3)), Some(Severity::Low));
        assert_eq!(
            at(file(12, 3, 3)),
            Some(Severity::Info),
            "coupled but only mildly large stays Info"
        );
    }

    #[test]
    fn no_finding_reads_as_an_absence_claim_to_the_host() {
        // a zero figure in a summary is a whole-word "0", which `Finding::validate` takes for an absence claim (seen on the corpus)
        let mut m = file(25, 3, 0);
        m.types = vec![metrics::TypeMetrics {
            name: "Big".into(),
            line: 3,
            fields: 0,
            methods: 5,
        }];
        let found = judge(&[m, file(2, 5, 0), file(25, 0, 3)], &T, "p");
        assert_eq!(found.len(), 4);
        for f in found {
            let summary = f.summary.clone();
            f.validate().unwrap_or_else(|e| panic!("{summary}: {e}"));
        }
    }

    #[test]
    fn a_type_named_like_an_absence_word_still_validates() {
        for name in ["None", "No", "Zero", "Missing"] {
            let mut m = file(1, 0, 0);
            m.types = vec![metrics::TypeMetrics {
                name: name.into(),
                line: 3,
                fields: 0,
                methods: 5,
            }];
            let f = judge(&[m], &T, "p");
            assert_eq!(f.len(), 1, "{name}");
            let summary = f[0].summary.clone();
            f.into_iter()
                .next()
                .unwrap()
                .validate()
                .unwrap_or_else(|e| panic!("{summary}: {e}"));
        }
    }

    #[test]
    fn coupling_alone_is_info() {
        let f = judge(&[file(2, 5, 0)], &T, "p");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].summary.contains("coupling outlier"));
    }

    #[test]
    fn a_finding_states_the_value_the_threshold_and_the_basis() {
        let f = &judge(&[file(25, 0, 0)], &T, "p")[0];
        assert!(
            f.summary.contains("25 code lines (threshold 10)"),
            "{}",
            f.summary
        );
        assert!(f.detail.contains("test thresholds"), "{}", f.detail);
    }

    #[test]
    fn a_record_with_no_methods_is_not_reported_however_many_fields() {
        let mut m = file(1, 0, 0);
        m.types = vec![metrics::TypeMetrics {
            name: "Record".into(),
            line: 3,
            fields: 40,
            methods: 0,
        }];
        assert!(judge(&[m], &T, "p").is_empty());
    }

    #[test]
    fn a_type_at_the_member_threshold_is_reported_and_one_below_is_not() {
        let mut m = file(1, 0, 0);
        m.types = vec![
            metrics::TypeMetrics {
                name: "Big".into(),
                line: 3,
                fields: 2,
                methods: 2,
            },
            metrics::TypeMetrics {
                name: "Small".into(),
                line: 9,
                fields: 2,
                methods: 1,
            },
        ];
        let f = judge(&[m], &T, "p");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject.as_deref(), Some("Big"));
    }
}
