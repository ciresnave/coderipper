use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowlistFile {
    #[serde(rename = "allow", default)]
    entries: Vec<AllowEntry>,
    #[serde(default)]
    tracks: Vec<TrackEntry>,
    #[serde(default)]
    buf: Option<BufSettings>,
}

/// `[buf]`: how API-006 (`buf breaking`) picks the schema version it compares the working tree with.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BufSettings {
    /// A git ref (branch, tag or commit) naming the released schema. Without it the baseline is the merge-base of `HEAD` with
    /// `origin/HEAD`.
    baseline: Option<String>,
}

/// A declared relationship, not a suppression: `package` is exempt from the one-version rule because
/// it works with another project, and must match the version in that project's manifest instead
/// (`version-consistency`, design doc section 5). `manifest` is relative to the project root.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrackEntry {
    pub package: String,
    pub manifest: String,
    /// Required, like an allowlist entry's: every exception says why.
    pub reason: String,
}

/// `[[allow]]`: a waiver. Since 0.5.0 a finding fails the run unless an entry names it; a waived finding is still printed, with
/// `reason`. `check`, `file` and `reason` are required. `symbol` narrows the waiver to the finding about that one symbol, and
/// `lines` (`"12"` or `"10-20"`, inclusive) to findings reported at those lines; omit both to waive every finding of `check` in
/// `file`. A finding that names no line is never matched by an entry with `lines`.
#[derive(Debug, Deserialize)]
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
    buf_baseline: Option<String>,
}

impl Allowlist {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            tracks: Vec::new(),
            buf_baseline: None,
        }
    }

    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = project_root.join(".coderipper.toml");
        if !path.exists() {
            return Ok(Self::empty());
        }
        let text = std::fs::read_to_string(&path)?;
        let parsed: AllowlistFile = toml::from_str(&text)?;
        Ok(Self {
            entries: parsed.entries,
            tracks: parsed.tracks,
            buf_baseline: parsed.buf.and_then(|b| b.baseline),
        })
    }

    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
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
