use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowlistFile {
    #[serde(rename = "allow", default)]
    entries: Vec<AllowEntry>,
    #[serde(default)]
    tracks: Vec<TrackEntry>,
}

/// A declared relationship, not a suppression: `package` is exempt from the one-version rule because
/// it works with another project, and must match the version in that project's manifest instead
/// (`version-consistency`, design doc section 5). `manifest` is relative to the project root.
#[derive(Debug, Deserialize)]
pub(crate) struct TrackEntry {
    pub package: String,
    pub manifest: String,
    /// Required, like an allowlist entry's: every exception says why.
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AllowEntry {
    pub check: String,
    pub file: String,
    pub symbol: String,
    /// Required: a missing field fails TOML deserialization, which is what enforces "every
    /// suppression says why". Shown in the finding raised when the entry goes stale.
    pub reason: String,
}

pub struct Allowlist {
    entries: Vec<AllowEntry>,
    tracks: Vec<TrackEntry>,
}

impl Allowlist {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            tracks: Vec::new(),
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
        })
    }

    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
    }

    pub(crate) fn tracks(&self) -> &[TrackEntry] {
        &self.tracks
    }
}

#[cfg(test)]
mod tests {
    use super::Allowlist;

    fn allowed(al: &Allowlist, check: &str, file: &str, symbol: &str) -> bool {
        al.entries()
            .iter()
            .any(|e| e.check == check && e.file == file && e.symbol == symbol)
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
