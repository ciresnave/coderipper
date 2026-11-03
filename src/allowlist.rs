// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::Deserialize;
use std::path::Path;

/// `[buf]`: how API-006 (`buf breaking`) picks the schema version it compares the working tree with.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BufSettings {
    /// A git ref (branch, tag or commit) naming the released schema. Without it the baseline is the merge-base of `HEAD` with
    /// `origin/HEAD`.
    pub baseline: Option<String>,
}

/// A declared relationship, not a suppression: `package` is exempt from the one-version rule because
/// it works with another project, and must match the version in that project's manifest instead
/// (`version-consistency`, design doc section 5). `manifest` is relative to the project root.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrackEntry {
    pub package: String,
    pub manifest: String,
    /// Required, like an allowlist entry's: every exception says why.
    pub reason: String,
}

/// `[[skip_member]]`: a workspace member this repository's `--workspace` run does not build, and says so every run ("not judged
/// (declared skip)"). `name` is the cargo package name. `reason` and `owner` are required, like a waiver's. With `needs` the skip
/// holds only while that executable is absent from `PATH`: a runner that has the tool builds and judges the member.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkipMemberEntry {
    pub name: String,
    pub reason: String,
    pub owner: String,
    #[serde(default)]
    pub needs: Option<String>,
}

/// The probes of `OPS-011` a `[[cli_waiver]]` entry may name: the four standard flags as typed, and `unknown-flag` for the bad-flag probe.
pub(crate) const CLI_PROBE_NAMES: [&str; 5] = ["--version", "-V", "--help", "-h", "unknown-flag"];

/// `[[cli_waiver]]`: `OPS-011` does not hold the binary `bin` to the probes named in `probes` (a subset of [`CLI_PROBE_NAMES`]),
/// because its documented contract differs (for example a usage error that exits 1). `bin`, `probes` and a non-blank `reason` are
/// required; the waived probe is still reported, as `waived: <reason>`. The other probes of the binary are still judged.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CliWaiver {
    pub bin: String,
    pub probes: Vec<String>,
    pub reason: String,
}

impl CliWaiver {
    /// A waiver without a reason, without a probe, or naming a probe that does not exist is a configuration error, never a silent no-op.
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.reason.trim().is_empty(),
            "`[[cli_waiver]]` for `{}` has an empty `reason`: every waiver says why",
            self.bin
        );
        anyhow::ensure!(
            !self.bin.trim().is_empty(),
            "`[[cli_waiver]]` has an empty `bin`"
        );
        anyhow::ensure!(
            !self.probes.is_empty(),
            "`[[cli_waiver]]` for `{}` names no probe (one or more of {})",
            self.bin,
            CLI_PROBE_NAMES.join(", ")
        );
        for p in &self.probes {
            anyhow::ensure!(
                CLI_PROBE_NAMES.contains(&p.as_str()),
                "`[[cli_waiver]]` for `{}` names an unknown probe `{p}` (one of {})",
                self.bin,
                CLI_PROBE_NAMES.join(", ")
            );
        }
        Ok(())
    }
}

/// `[[allow]]`: a waiver. Since 0.5.0 a finding fails the run unless an entry names it; a waived finding is still printed, with
/// `reason`. `check`, `file` and `reason` are required. `symbol` narrows the waiver to the finding about that one symbol, and
/// `lines` (`"12"` or `"10-20"`, inclusive) to findings reported at those lines; omit both to waive every finding of `check` in
/// `file`. A finding that names no line is never matched by an entry with `lines`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AllowEntry {
    pub check: String,
    pub file: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default, deserialize_with = "line_range")]
    pub lines: Option<LineRange>,
    /// Required: a missing field fails TOML deserialization, which is what enforces "every
    /// suppression says why". Shown with the waived finding, and in the finding raised when the entry goes stale.
    pub reason: String,
    /// The wide configuration file this entry came from; `None` for the repository's own file. Not a key: it is set by the
    /// reader, and a file that writes it is an unknown-key error like any other.
    #[serde(skip)]
    pub source: Option<String>,
    /// Set by the reader for an entry that came from `[reachability] public_api`: `file` is then a glob.
    #[serde(skip)]
    pub declared_public: bool,
    /// Set by the reader for an entry that came from `[reachability] public_api_crates`: the cargo package whose findings it covers
    /// (`file` is then `**`). Matched against a finding's `member` in a `--workspace` run, else its `project`.
    #[serde(skip)]
    pub crate_scope: Option<String>,
    /// Set when a crate-level declaration of the workspace root was added to the run of the package it names: every finding of that
    /// run is then covered, whatever the finding's `project` (the directory name) is.
    #[serde(skip)]
    pub run_scoped: bool,
}

/// One item of `[reachability] public_api`: the project declares that what `file` (a path or a glob: `*` inside one directory,
/// `**` across directories, `?`) holds is public API consumed outside this repository, so `reachability` does not fail the run
/// on it. Unlike an `[[allow]]` entry it is printed as "declared public", and it warns when it matches nothing. `symbol`
/// narrows it to one item. `reason` is required.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicApiEntry {
    pub file: String,
    #[serde(default)]
    pub symbol: Option<String>,
    pub reason: String,
}

/// One item of `[reachability] public_api_crates`: the project declares that every public item of the cargo package `crate` is API
/// consumed outside this repository (an FFI binding crate, a published library), so `reachability` does not fail the run on any of
/// them. It is the crate-level form of `public_api`: one entry instead of one per symbol or glob, and it names the package, so it
/// survives a move of the directory and, in a `--workspace` run, does not depend on paths that are relative to each member.
/// `reason` is required.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicApiCrate {
    #[serde(rename = "crate")]
    pub name: String,
    pub reason: String,
}

/// `[reachability]`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReachabilitySettings {
    #[serde(default)]
    pub public_api: Vec<PublicApiEntry>,
    #[serde(default)]
    pub public_api_crates: Vec<PublicApiCrate>,
}

impl ReachabilitySettings {
    /// Every declaration of this table as the suppression entry it is applied as.
    fn entries(&self, source: Option<String>) -> impl Iterator<Item = AllowEntry> + '_ {
        self.public_api
            .iter()
            .map(PublicApiEntry::as_entry_with(source.clone()))
            .chain(
                self.public_api_crates
                    .iter()
                    .map(move |c| c.as_entry(source.clone())),
            )
    }
}

impl PublicApiCrate {
    fn as_entry(&self, source: Option<String>) -> AllowEntry {
        AllowEntry {
            check: "reachability".to_string(),
            file: "**".to_string(),
            symbol: None,
            lines: None,
            reason: self.reason.clone(),
            source,
            declared_public: true,
            crate_scope: Some(self.name.clone()),
            run_scoped: false,
        }
    }
}

impl PublicApiEntry {
    /// The suppression entry this declaration is applied as.
    fn as_entry(&self, source: Option<String>) -> AllowEntry {
        AllowEntry {
            check: "reachability".to_string(),
            file: self.file.clone(),
            symbol: self.symbol.clone(),
            lines: None,
            reason: self.reason.clone(),
            source,
            declared_public: true,
            crate_scope: None,
            run_scoped: false,
        }
    }

    fn as_entry_with(source: Option<String>) -> impl Fn(&PublicApiEntry) -> AllowEntry {
        move |p| p.as_entry(source.clone())
    }
}

/// The `public_api_crates` entries of the nearest enclosing workspace root's `.coderipper.toml` that name the cargo package at
/// `project_root`, marked as covering that whole run. Empty when the directory is not a workspace member, the root is the directory
/// itself (its own file is read already), or the root declares nothing for this package. Cheap on purpose: two small TOML reads.
fn workspace_declarations_for(project_root: &Path) -> Vec<AllowEntry> {
    let read_table = |dir: &Path| -> Option<toml::Table> {
        std::fs::read_to_string(dir.join("Cargo.toml"))
            .ok()?
            .parse()
            .ok()
    };
    let Some(package) = read_table(project_root)
        .and_then(|t| t.get("package")?.get("name")?.as_str().map(str::to_string))
    else {
        return Vec::new();
    };
    let Some(root) = project_root
        .ancestors()
        .skip(1)
        .find(|dir| read_table(dir).is_some_and(|t| t.contains_key("workspace")))
    else {
        return Vec::new();
    };
    let Ok(list) = Allowlist::load(root) else {
        return Vec::new();
    };
    list.crate_declarations()
        .entries
        .into_iter()
        .filter(|e| e.crate_scope.as_deref() == Some(package.as_str()))
        .map(|mut e| {
            e.run_scoped = true;
            e
        })
        .collect()
}

/// An inclusive range of 1-based line numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineRange {
    pub first: u32,
    pub last: u32,
}

impl LineRange {
    pub fn contains(self, line: u32) -> bool {
        (self.first..=self.last).contains(&line)
    }
}

impl std::fmt::Display for LineRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.first == self.last {
            write!(f, "{}", self.first)
        } else {
            write!(f, "{}-{}", self.first, self.last)
        }
    }
}

fn parse_line_range(text: &str) -> Result<LineRange, String> {
    let bad = || {
        format!("`lines = \"{text}\"` is not a line (\"12\") or an inclusive range (\"10-20\") of lines counted from 1")
    };
    // ASCII digits only: no sign, no spaces, no other script's digits
    let number = |part: &str| match part.parse::<u32>() {
        Ok(n) if n >= 1 && part.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
        _ => Err(bad()),
    };
    let (first, last) = match text.split_once('-') {
        Some((a, b)) => (number(a)?, number(b)?),
        None => {
            let n = number(text)?;
            (n, n)
        }
    };
    if first > last {
        return Err(bad());
    }
    Ok(LineRange { first, last })
}

fn line_range<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<LineRange>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|text| parse_line_range(&text).map_err(serde::de::Error::custom))
        .transpose()
}

pub struct Allowlist {
    entries: Vec<AllowEntry>,
    tracks: Vec<TrackEntry>,
    skip_members: Vec<SkipMemberEntry>,
    cli_waivers: Vec<CliWaiver>,
    buf_baseline: Option<String>,
}

impl Allowlist {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            tracks: Vec::new(),
            skip_members: Vec::new(),
            cli_waivers: Vec::new(),
            buf_baseline: None,
        }
    }

    /// The repository's `.coderipper.toml` (its `[[allow]]`, `[[tracks]]` and `[buf]`; other known keys are not read here:
    /// `crate::config` says which are skipped), then the entries of the wide files the command-line program installed, each
    /// marked with its file. A repository's own entry comes first, so its reason is the one shown for a finding both name.
    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = project_root.join(crate::config::FILE_NAME);
        let mut list = match crate::config::read_file(&path, false)? {
            Some(file) => {
                for w in &file.cli_waiver {
                    w.validate()?;
                }
                Self {
                    entries: file
                        .allow
                        .into_iter()
                        .chain(file.reachability.iter().flat_map(|r| r.entries(None)))
                        .collect(),
                    tracks: file.tracks,
                    skip_members: file.skip_member,
                    cli_waivers: file.cli_waiver,
                    buf_baseline: file.buf.and_then(|b| b.baseline),
                }
            }
            None => Self::empty(),
        };
        let own = std::fs::canonicalize(&path).ok();
        let mut wide_baseline = None;
        for layer in crate::config::wide_layers() {
            // a repository at the user's home directory: its file is the wide file, and is read once
            if own.is_some() && std::fs::canonicalize(&layer.path).ok() == own {
                continue;
            }
            let source = layer.path.display().to_string();
            list.entries
                .extend(layer.file.allow.iter().cloned().map(|mut e| {
                    e.source = Some(source.clone());
                    e
                }));
            list.entries.extend(
                layer
                    .file
                    .reachability
                    .iter()
                    .flat_map(|r| r.entries(Some(source.clone()))),
            );
            list.tracks.extend(layer.file.tracks.iter().cloned());
            if let Some(b) = layer.file.buf.as_ref().and_then(|b| b.baseline.clone()) {
                wide_baseline = Some(b);
            }
        }
        if list.buf_baseline.is_none() {
            list.buf_baseline = wide_baseline;
        }
        Ok(list)
    }

    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
    }

    /// Only the crate-level `[reachability] public_api_crates` declarations: the part of a workspace root's file that covers the
    /// findings of its members (a member's own run reads the member's file, not the root's).
    pub fn crate_declarations(mut self) -> Self {
        self.entries.retain(|e| e.crate_scope.is_some());
        self
    }

    /// [`Self::load`] for the package at `project_root`, plus the crate-level declarations that its **workspace root's** file makes for
    /// this package. The same run, whether `--workspace` made it or a lane ran `--project crates/x` by hand, so a local run agrees with CI.
    pub fn load_for_package(project_root: &Path) -> anyhow::Result<Self> {
        let mut list = Self::load(project_root)?;
        list.entries
            .extend(workspace_declarations_for(project_root));
        Ok(list)
    }

    /// The repository's own `[[skip_member]]` entries (a wide file's are not read: whether a member is built is the repository's
    /// to say, and reviewed in its pull request).
    pub(crate) fn skip_members(&self) -> &[SkipMemberEntry] {
        &self.skip_members
    }

    /// The repository's own `[[cli_waiver]]` entries (a wide file's are not read: what a binary's contract is belongs to the repository).
    pub(crate) fn cli_waivers(&self) -> &[CliWaiver] {
        &self.cli_waivers
    }

    pub(crate) fn tracks(&self) -> &[TrackEntry] {
        &self.tracks
    }

    /// The `[buf] baseline` git ref, when the project names one.
    pub(crate) fn buf_baseline(&self) -> Option<&str> {
        self.buf_baseline.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::Allowlist;

    fn allowed(al: &Allowlist, check: &str, file: &str, symbol: &str) -> bool {
        al.entries()
            .iter()
            .any(|e| e.check == check && e.file == file && e.symbol.as_deref() == Some(symbol))
    }

    #[test]
    fn missing_allowlist_file_allows_nothing_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let al = Allowlist::load(tmp.path()).unwrap();
        assert!(!allowed(&al, "reachability", "src/lib.rs", "anything"));
    }

    #[test]
    fn an_exact_file_and_symbol_match_is_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            r#"
[[allow]]
check = "reachability"
file = "src/api.rs"
symbol = "public_entry_point"
reason = "published crate API, consumed outside this portfolio"
"#,
        )
        .unwrap();

        let al = Allowlist::load(tmp.path()).unwrap();
        assert!(allowed(
            &al,
            "reachability",
            "src/api.rs",
            "public_entry_point"
        ));
        // Review Focus: must match by identity, not loose path containment.
        assert!(!allowed(&al, "reachability", "src/api.rs", "some_other_fn"));
        assert!(!allowed(
            &al,
            "reachability",
            "src/api_v2.rs",
            "public_entry_point"
        ));
    }

    #[test]
    fn a_misspelled_table_header_is_an_error_not_an_empty_allowlist() {
        // Review finding: `[[allows]]` parsed to zero entries with no error, so every suppression in
        // the file silently stopped applying.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            "[[allows]]\ncheck = \"a\"\nfile = \"f\"\nsymbol = \"s\"\nreason = \"r\"\n",
        )
        .unwrap();
        let err = Allowlist::load(tmp.path())
            .err()
            .expect("must fail")
            .to_string();
        assert!(err.contains("allows"), "{err}");
    }

    fn load_waiver(body: &str) -> anyhow::Result<Allowlist> {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            format!(
                "[[cli_waiver]]
{body}"
            ),
        )
        .unwrap();
        Allowlist::load(tmp.path())
    }

    #[test]
    fn a_cli_waiver_loads_with_bin_probes_and_reason() {
        let al = load_waiver(
            "bin = \"lane-restart\"
probes = [\"unknown-flag\", \"-h\"]
reason = \"usage error exits 1\"
",
        )
        .unwrap();
        assert_eq!(al.cli_waivers().len(), 1);
        assert_eq!(al.cli_waivers()[0].bin, "lane-restart");
        assert_eq!(al.cli_waivers()[0].probes, ["unknown-flag", "-h"]);
    }

    #[test]
    fn a_cli_waiver_without_a_reason_is_a_config_error() {
        let err = load_waiver(
            "bin = \"x\"
probes = [\"-h\"]
",
        )
        .err()
        .expect("a missing reason must fail")
        .to_string();
        assert!(err.contains("reason"), "{err}");
    }

    #[test]
    fn a_cli_waiver_with_a_blank_reason_no_probe_or_a_made_up_probe_is_a_config_error() {
        for body in [
            "bin = \"x\"
probes = [\"-h\"]
reason = \"  \"
",
            "bin = \"x\"
probes = []
reason = \"r\"
",
            "bin = \"x\"
probes = [\"--nope\"]
reason = \"r\"
",
        ] {
            assert!(load_waiver(body).is_err(), "{body}");
        }
    }

    #[test]
    fn tracks_entries_load_with_package_manifest_and_reason() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\nreason = \"works with other\"\n",
        )
        .unwrap();
        let al = Allowlist::load(tmp.path()).unwrap();
        assert_eq!(al.tracks().len(), 1);
        assert_eq!(al.tracks()[0].package, "emit");
        assert_eq!(al.tracks()[0].manifest, "../other/Cargo.toml");
        assert!(
            al.entries().is_empty(),
            "a [[tracks]] entry is not a suppression"
        );
    }

    #[test]
    fn a_tracks_entry_with_no_reason_fails_to_load() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            "[[tracks]]\npackage = \"emit\"\nmanifest = \"../other/Cargo.toml\"\n",
        )
        .unwrap();
        assert!(
            Allowlist::load(tmp.path()).is_err(),
            "every exception says why"
        );
    }

    fn load(entry: &str) -> anyhow::Result<Allowlist> {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            format!(
                "[[allow]]
check = \"a\"
file = \"src/x.rs\"
reason = \"r\"
{entry}"
            ),
        )
        .unwrap();
        Allowlist::load(tmp.path())
    }

    #[test]
    fn an_entry_needs_no_symbol_and_no_lines() {
        let al = load("").unwrap();
        assert_eq!(al.entries()[0].symbol, None);
        assert_eq!(al.entries()[0].lines, None);
    }

    #[test]
    fn lines_is_one_line_or_an_inclusive_range() {
        let one = load(
            "lines = \"12\"
",
        )
        .unwrap();
        assert_eq!(
            one.entries()[0].lines,
            Some(super::LineRange {
                first: 12,
                last: 12
            })
        );
        let range = load(
            "lines = \"10-20\"
",
        )
        .unwrap();
        let r = range.entries()[0].lines.unwrap();
        assert!(r.contains(10) && r.contains(20) && !r.contains(9) && !r.contains(21));
        assert_eq!(r.to_string(), "10-20");
    }

    #[test]
    fn a_malformed_lines_value_is_an_error_not_a_waiver_of_everything() {
        for bad in [
            "",
            "0",
            "abc",
            "20-10",
            "1-",
            "-5",
            "1-2-3",
            "-1",
            "+5",
            " 5",
            "5 ",
            "5 - 7",
            "٣",
            "4294967296",
        ] {
            let err = load(&format!(
                "lines = \"{bad}\"
"
            ));
            assert!(err.is_err(), "lines = {bad:?} must not load");
        }
    }

    #[test]
    fn an_entry_with_an_unknown_key_no_longer_loads() {
        // Before 0.5.0 an `[[allow]]` entry ignored keys it did not know; it is now an error (stated under Breaking in the CHANGELOG).
        let err = load("note = \"was ignored before\"\n")
            .err()
            .expect("must fail");
        assert!(err.to_string().contains("note"), "{err}");
    }

    #[test]
    fn a_misspelled_entry_key_is_an_error() {
        // `line = 5` (singular) would otherwise be ignored and waive the whole file.
        assert!(load(
            "line = 5
"
        )
        .is_err());
    }

    #[test]
    fn an_entry_with_no_reason_fails_to_load() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            r#"
[[allow]]
check = "reachability"
file = "src/api.rs"
symbol = "x"
"#,
        )
        .unwrap();

        assert!(
            Allowlist::load(tmp.path()).is_err(),
            "reason is required, per design doc §4"
        );
    }
}
