use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct AllowlistFile {
    #[serde(rename = "allow", default)]
    entries: Vec<AllowEntry>,
}

#[derive(Debug, Deserialize)]
struct AllowEntry {
    check: String,
    file: String,
    symbol: String,
    // Required so a missing field fails TOML deserialization -- that's the enforcement mechanism,
    // not programmatic use. rustc's dead_code lint deliberately does not count a derived Debug
    // impl's field read as real usage (confirmed via its own diagnostic note), so this is an
    // honest, compiler-suggested suppression for a genuinely validation-only field, not a
    // workaround for a real bug. Debug is still kept for future diagnostic output.
    #[allow(dead_code)]
    reason: String,
}

pub struct Allowlist {
    entries: Vec<AllowEntry>,
}

impl Allowlist {
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

    pub fn is_allowed(&self, check_id: &str, file: &str, symbol: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.check == check_id && e.file == file && e.symbol == symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::Allowlist;

    #[test]
    fn missing_allowlist_file_allows_nothing_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let al = Allowlist::load(tmp.path()).unwrap();
        assert!(!al.is_allowed("reachability", "src/lib.rs", "anything"));
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
        assert!(al.is_allowed("reachability", "src/api.rs", "public_entry_point"));
        // Review Focus: must match by identity, not loose path containment.
        assert!(!al.is_allowed("reachability", "src/api.rs", "some_other_fn"));
        assert!(!al.is_allowed("reachability", "src/api_v2.rs", "public_entry_point"));
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
