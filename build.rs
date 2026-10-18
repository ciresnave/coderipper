//! Fails the build when the shipped `src/tools/tools.lock` breaks the lock rules (a bad hash, an unsafe name, a source scheme
//! the installer refuses, a duplicate entry). The rules are `src/tools/lock_rules.rs`, the same file the run-time parser uses.

#[path = "src/tools/lock_rules.rs"]
mod lock_rules;

fn main() {
    println!("cargo:rerun-if-changed=src/tools/tools.lock");
    println!("cargo:rerun-if-changed=src/tools/lock_rules.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let text = std::fs::read_to_string("src/tools/tools.lock").expect("read src/tools/tools.lock");
    if let Err(problem) = check_lock(&text) {
        panic!("src/tools/tools.lock is invalid: {problem}");
    }
}

fn check_lock(text: &str) -> Result<(), String> {
    let table: toml::Table = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut seen: Vec<(String, String)> = Vec::new();
    for (key, value) in &table {
        if key != "tool" {
            return Err(format!("unknown top-level key {key:?}"));
        }
        let entries = value.as_array().ok_or("`tool` is not an array of tables")?;
        for entry in entries {
            let entry = entry.as_table().ok_or("a `tool` entry is not a table")?;
            let text = |k: &str| entry.get(k).and_then(toml::Value::as_str);
            let need = |k: &str| text(k).ok_or(format!("an entry lacks the string `{k}`"));
            let (name, version, platform) = (need("name")?, need("version")?, need("platform")?);
            let label = format!("{name} {version}");
            lock_rules::check(&lock_rules::Fields {
                name,
                version,
                platform,
                source: need("source")?,
                sha256: need("sha256")?,
                licence: need("licence")?,
                file: text("file"),
                archive: text("archive"),
                member: text("member"),
                file_sha256: text("file_sha256"),
            })
            .map_err(|what| format!("{label}: {what}"))?;
            let id = (name.to_ascii_lowercase(), platform.to_string());
            if seen.contains(&id) {
                return Err(format!("{label}: two entries for {platform}"));
            }
            seen.push(id);
        }
    }
    Ok(())
}
