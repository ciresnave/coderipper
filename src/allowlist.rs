use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct AllowlistFile {
    #[serde(rename = "allow", default)]
    entries: Vec<AllowEntry>,
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
}

impl Allowlist {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = project_root.join(".coderipper.toml");
        if !path.exists() {
            return Ok(Self {
                entries: Vec::new(),
            });
        }
        let text = std::fs::read_to_string(&path)?;
        let parsed: AllowlistFile = toml::from_str(&text)?;
        Ok(Self {
            entries: parsed.entries,
        })
    }

    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
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
