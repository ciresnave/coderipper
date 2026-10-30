// SPDX-License-Identifier: MIT OR Apache-2.0
//! Configuration layers (`docs/superpowers/plans/2026-10-13-configuration-layers.md`).
//!
//! One file name, `.coderipper.toml`, is read from up to four places and combined, weakest first:
//!
//! 1. built-in defaults;
//! 2. the **wide files**: `/etc/coderipper/coderipper.toml` (dotless), `/etc/coderipper/.coderipper.toml`, `/etc/.coderipper.toml`, then `~/.coderipper.toml` (on Windows
//!    `%ProgramData%\coderipper\`, `%ProgramData%\` and `%USERPROFILE%`). Every file found is read; a later one overrides an
//!    earlier one. They may set everything;
//! 3. the **repository's file**, which may set only [`REPOSITORY_MAY_SET`]: a known key outside that list warns and is
//!    skipped (fail closed: the repository is untrusted input in the hosted service), an unknown key is an error;
//! 4. the environment (`CODERIPPER_CACHE`, ...), which wins over every file;
//! 5. command-line flags, which win over everything.
//!
//! **Limits are the one exception**: the effective value of a `[limits]` key is the *minimum* every layer gave, so a later layer
//! can lower a ceiling and never raise it. CodeRipper reports the ceilings it was given; the process that runs it enforces them
//! (a limit read from a file inside the job's own container is a request, not a boundary).
//!
//! The wide files are read once by the command-line program and installed for the process ([`install`]). A library caller
//! that never calls it sees no wide file at all, so a library run is exactly as hermetic as before.

use crate::allowlist::{
    AllowEntry, BufSettings, CliWaiver, ReachabilitySettings, SkipMemberEntry, TrackEntry,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The one file name, in every place it is searched.
pub(crate) const FILE_NAME: &str = ".coderipper.toml";
/// The dotless spelling accepted in `/etc/coderipper/` only (a system directory has no reason to hide its file).
#[cfg(any(feature = "cli", test))]
const SYSTEM_FILE_NAME: &str = "coderipper.toml";

#[cfg(feature = "cli")]
/// What a repository's own file may set. Everything else is skipped with a warning. Adding a key here is a deliberate act and a
/// CHANGELOG line: a key wrongly added cannot be taken back.
pub(crate) const REPOSITORY_MAY_SET: &str =
    "[[allow]], [[tracks]], [[skip_member]], [[cli_waiver]], [buf], [reachability] and profile";

/// How findings are written to stdout (`--message-format`, and the key of the same name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MessageFormat {
    /// Human-readable lines (the default).
    Human,
    /// One JSON object per line, each with a `"reason"`: `coderipper-finding` per finding, then one
    /// `coderipper-summary` with the finding count, the errors and the exit code.
    Json,
    /// One line per finding, `file:line: severity: summary [check id]` (a finding with no place leads with its package), and
    /// nothing else on stdout: no waivers, coverage or summary. Errors and notes stay on stderr.
    Short,
}

/// Which rules run and what is reported (`--profile`, and the key of the same name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Profile {
    /// The five original checks, plus `conflict-marker` (the one rule every profile takes), and the output they have always had (the default).
    Classic,
    /// `classic`, plus the coverage report and the language-neutral rules (SUP-001, SUP-011, DOC-005, DOC-009, WSP-001, lint-suppression, conflict-marker); the
    /// rules of the language modules join it as they arrive.
    Extended,
    /// What a repository's pull-request check takes: the five original checks and every language-neutral rule, but none of the
    /// rules delegated to a tool (no network, no tool to install), judged against a per-rule blocking floor instead of one `--deny`:
    /// a rule that finds a mechanical violation fails the run, an expectation-class rule only reports (see `portfolio_floor`).
    Portfolio,
}

/// The severity at which a finding fails the run, or `none` for never (`--deny`, and the key of the same name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Deny {
    None,
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[cfg(any(feature = "cli", test))]
/// The limit keys, in the order they are reported.
const LIMIT_NAMES: &[&str] = &[
    "max_runtime_minutes",
    "max_memory_mb",
    "max_disk_mb",
    "max_processes",
];

/// One `.coderipper.toml`, as written. `deny_unknown_fields`: a misspelt key is an error, never a silent absence.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigFile {
    #[serde(rename = "allow", default)]
    pub allow: Vec<AllowEntry>,
    #[serde(default)]
    pub tracks: Vec<TrackEntry>,
    /// `[[skip_member]]`: workspace members this repository does not build (see [`SkipMemberEntry`]).
    #[serde(default)]
    pub skip_member: Vec<SkipMemberEntry>,
    #[serde(default)]
    pub buf: Option<BufSettings>,
    /// `[[cli_waiver]]`: probes of `OPS-011` a binary is not held to (see [`crate::allowlist::CliWaiver`]).
    #[serde(default)]
    pub cli_waiver: Vec<CliWaiver>,
    /// `[reachability] public_api`: declared public API (see [`crate::allowlist::PublicApiEntry`]).
    #[serde(default)]
    pub reachability: Option<ReachabilitySettings>,
    /// A default for `--profile`.
    #[cfg_attr(
        not(any(feature = "cli", test)),
        expect(
            dead_code,
            reason = "parsed so a wrong value is an error in every build; only the command-line program uses it"
        )
    )]
    pub profile: Option<Profile>,
    /// `blocking = [..]`: rules this repository has made blocking under `--profile portfolio` (see `portfolio_floor`), by rule id; every finding of
    /// such a rule at Low or above fails the run. A repository that is clean for a rule flips it here; it never lowers a default floor.
    #[cfg_attr(
        not(any(feature = "cli", test)),
        expect(
            dead_code,
            reason = "parsed so a wrong value is an error in every build; only the command-line program uses it"
        )
    )]
    #[serde(default)]
    pub blocking: Vec<String>,
    /// A default for `--deny`.
    #[cfg_attr(
        not(any(feature = "cli", test)),
        expect(
            dead_code,
            reason = "parsed so a wrong value is an error in every build; only the command-line program uses it"
        )
    )]
    pub deny: Option<Deny>,
    /// A default for `--message-format`.
    #[serde(rename = "message-format")]
    #[cfg_attr(
        not(any(feature = "cli", test)),
        expect(
            dead_code,
            reason = "parsed so a wrong value is an error in every build; only the command-line program uses it"
        )
    )]
    pub message_format: Option<MessageFormat>,
    #[cfg(any(feature = "cli", test))]
    /// A default for `--redact`.
    pub redact: Option<bool>,
    #[cfg(any(feature = "cli", test))]
    /// A default for `--install-tools`.
    #[serde(rename = "install-tools")]
    pub install_tools: Option<bool>,
    #[cfg(any(feature = "cli", test))]
    pub cache: Option<CacheSection>,
    #[cfg(any(feature = "cli", test))]
    pub tools: Option<ToolsSection>,
    #[cfg(any(feature = "cli", test))]
    pub limits: Option<LimitsSection>,
    /// Without the command-line program the keys only it reads are accepted, whatever their value, and not looked at; any other
    /// key is still an error. `tests/config_cli_only_keys.rs` pins both builds.
    #[cfg(not(any(feature = "cli", test)))]
    #[serde(rename = "redact")]
    _redact: Option<toml::Value>,
    #[cfg(not(any(feature = "cli", test)))]
    #[serde(rename = "install-tools")]
    _install_tools: Option<toml::Value>,
    #[cfg(not(any(feature = "cli", test)))]
    #[serde(rename = "cache")]
    _cache: Option<toml::Value>,
    #[cfg(not(any(feature = "cli", test)))]
    #[serde(rename = "tools")]
    _tools: Option<toml::Value>,
    #[cfg(not(any(feature = "cli", test)))]
    #[serde(rename = "limits")]
    _limits: Option<toml::Value>,
}

#[cfg(any(feature = "cli", test))]
/// `[cache]`: what `CODERIPPER_CACHE`, `CODERIPPER_CACHE_DIR`, `CODERIPPER_CACHE_MAX_GB` and `CODERIPPER_CACHE_WAIT_SECS` set.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CacheSection {
    pub enabled: Option<bool>,
    pub dir: Option<String>,
    pub max_gb: Option<f64>,
    pub wait_secs: Option<f64>,
}

#[cfg(any(feature = "cli", test))]
/// `[tools]`: what `CODERIPPER_TOOLS_DIR` sets.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolsSection {
    pub dir: Option<String>,
}

#[cfg(any(feature = "cli", test))]
/// `[limits]`: ceilings (see the module documentation).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LimitsSection {
    pub max_runtime_minutes: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub max_disk_mb: Option<u64>,
    pub max_processes: Option<u64>,
}

#[cfg(any(feature = "cli", test))]
impl LimitsSection {
    fn get(&self, name: &str) -> Option<u64> {
        match name {
            "max_runtime_minutes" => self.max_runtime_minutes,
            "max_memory_mb" => self.max_memory_mb,
            "max_disk_mb" => self.max_disk_mb,
            "max_processes" => self.max_processes,
            _ => None,
        }
    }
}

impl ConfigFile {
    /// Parses `text`, and checks the enumerated values so that a wrong value is an error naming the key and the file.
    pub(crate) fn parse(text: &str, path: &Path) -> anyhow::Result<Self> {
        let file: ConfigFile =
            toml::from_str(text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        Ok(file)
    }

    #[cfg(any(feature = "cli", test))]
    /// The keys this file sets that a repository's file may not (see [`REPOSITORY_MAY_SET`]), by their written names.
    pub(crate) fn skipped_for_a_repository(&self) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        let mut note = |set: bool, key: &str| {
            if set {
                keys.push(key.to_string());
            }
        };
        note(self.deny.is_some(), "deny");
        note(self.message_format.is_some(), "message-format");
        note(self.redact.is_some(), "redact");
        note(self.install_tools.is_some(), "install-tools");
        if let Some(c) = &self.cache {
            note(c.enabled.is_some(), "cache.enabled");
            note(c.dir.is_some(), "cache.dir");
            note(c.max_gb.is_some(), "cache.max_gb");
            note(c.wait_secs.is_some(), "cache.wait_secs");
        }
        if let Some(t) = &self.tools {
            note(t.dir.is_some(), "tools.dir");
        }
        if let Some(l) = &self.limits {
            for name in LIMIT_NAMES {
                note(l.get(name).is_some(), &format!("limits.{name}"));
            }
        }
        keys
    }
}

/// A file that was read, with where it came from.
#[derive(Debug)]
pub(crate) struct Layer {
    pub path: PathBuf,
    pub file: ConfigFile,
}

/// Reads `path` as a configuration file. `Ok(None)` when it is not there (and `required` is false).
pub(crate) fn read_file(path: &Path, required: bool) -> anyhow::Result<Option<ConfigFile>> {
    match std::fs::read_to_string(path) {
        Ok(text) => ConfigFile::parse(&text, path).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => Ok(None),
        Err(e) => Err(anyhow::anyhow!("cannot read {}: {e}", path.display())),
    }
}

#[cfg(any(feature = "cli", test))]
/// The places a wide file is looked for, weakest first. `get` is the environment lookup (tests pass their own).
pub(crate) fn search_paths(get: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let present = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let mut paths = Vec::new();
    if cfg!(windows) {
        if let Some(base) = present("ProgramData") {
            let base = PathBuf::from(base);
            paths.push(base.join("coderipper").join(FILE_NAME));
            paths.push(base.join(FILE_NAME));
        }
    } else {
        paths.push(PathBuf::from("/etc/coderipper").join(SYSTEM_FILE_NAME));
        paths.push(PathBuf::from("/etc/coderipper").join(FILE_NAME));
        paths.push(PathBuf::from("/etc").join(FILE_NAME));
    }
    if let Some(home) = present("HOME").or_else(|| present("USERPROFILE")) {
        paths.push(PathBuf::from(home).join(FILE_NAME));
    }
    paths
}

#[cfg(any(feature = "cli", test))]
/// Reads the wide files. `explicit` (`--config`) is read in place of the search, and must exist; `no_config` reads none.
pub(crate) fn load_wide(
    get: &dyn Fn(&str) -> Option<String>,
    explicit: Option<&Path>,
    no_config: bool,
) -> anyhow::Result<Vec<Layer>> {
    if no_config {
        return Ok(Vec::new());
    }
    let candidates: Vec<(PathBuf, bool)> = match explicit {
        Some(path) => vec![(path.to_path_buf(), true)],
        None => search_paths(get).into_iter().map(|p| (p, false)).collect(),
    };
    let mut layers: Vec<Layer> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for (path, required) in candidates {
        // two spellings of one file (HOME=/etc) are one layer: its entries and its warning are not doubled
        let identity = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&identity) {
            continue;
        }
        if let Some(file) = read_file(&path, required)? {
            seen.push(identity);
            layers.push(Layer { path, file });
        }
    }
    Ok(layers)
}

static WIDE: OnceLock<Vec<Layer>> = OnceLock::new();

#[cfg(feature = "cli")]
/// Installs the wide files for this process. When some were installed already the first stay: the command-line program calls
/// this once, before anything reads them.
pub(crate) fn install(layers: Vec<Layer>) {
    WIDE.get_or_init(|| layers);
}

/// The wide files of this process, weakest first; none unless the command-line program installed some.
pub(crate) fn wide_layers() -> &'static [Layer] {
    WIDE.get().map_or(&[], Vec::as_slice)
}

#[cfg(any(feature = "cli", test))]
/// One effective ceiling and the file or variable that gave the lowest value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Limit {
    pub name: &'static str,
    pub value: u64,
    pub source: String,
}

#[cfg(any(feature = "cli", test))]
/// The settings the wide files and the environment give, before flags and the repository's file are applied.
#[derive(Debug, Default, Clone)]
pub(crate) struct Settings {
    pub profile: Option<Profile>,
    pub deny: Option<Deny>,
    pub message_format: Option<MessageFormat>,
    pub redact: Option<bool>,
    pub install_tools: Option<bool>,
    pub cache_enabled: Option<bool>,
    pub cache_dir: Option<String>,
    pub cache_max_gb: Option<f64>,
    pub cache_wait_secs: Option<f64>,
    pub tools_dir: Option<String>,
    pub limits: Vec<Limit>,
}

#[cfg(any(feature = "cli", test))]
impl Settings {
    /// Combines `layers`, weakest first: a later layer's value replaces an earlier one's; limits take the minimum.
    pub(crate) fn from_layers(layers: &[Layer]) -> Self {
        let mut s = Settings::default();
        for layer in layers {
            let f = &layer.file;
            let take = |slot: &mut Option<String>, v: &Option<String>| {
                if v.is_some() {
                    slot.clone_from(v);
                }
            };
            s.profile = f.profile.or(s.profile);
            s.deny = f.deny.or(s.deny);
            s.message_format = f.message_format.or(s.message_format);
            if f.redact.is_some() {
                s.redact = f.redact;
            }
            if f.install_tools.is_some() {
                s.install_tools = f.install_tools;
            }
            if let Some(c) = &f.cache {
                if c.enabled.is_some() {
                    s.cache_enabled = c.enabled;
                }
                take(&mut s.cache_dir, &c.dir);
                if c.max_gb.is_some() {
                    s.cache_max_gb = c.max_gb;
                }
                if c.wait_secs.is_some() {
                    s.cache_wait_secs = c.wait_secs;
                }
            }
            if let Some(t) = &f.tools {
                take(&mut s.tools_dir, &t.dir);
            }
            if let Some(l) = &f.limits {
                for name in LIMIT_NAMES {
                    if let Some(v) = l.get(name) {
                        s.lower(name, v, layer.path.display().to_string());
                    }
                }
            }
        }
        s
    }

    /// Takes a ceiling from the environment (`CODERIPPER_MAX_RUNTIME_MINUTES`, ...): it can only lower what the files gave.
    pub(crate) fn apply_env_limits(&mut self, get: &dyn Fn(&str) -> Option<String>) {
        for name in LIMIT_NAMES {
            let var = format!("CODERIPPER_{}", name.to_ascii_uppercase());
            if let Some(v) = get(&var).and_then(|v| v.trim().parse::<u64>().ok()) {
                self.lower(name, v, var);
            }
        }
    }

    fn lower(&mut self, name: &'static str, value: u64, source: String) {
        match self.limits.iter_mut().find(|l| l.name == name) {
            Some(existing) if existing.value <= value => {}
            Some(existing) => {
                existing.value = value;
                existing.source = source;
            }
            None => self.limits.push(Limit {
                name,
                value,
                source,
            }),
        }
    }

    /// What a file gave for an environment variable the program already reads, so that variable's own reader needs no second
    /// code path: the environment is asked first, and this answers when it is silent.
    pub(crate) fn env_default(&self, key: &str) -> Option<String> {
        match key {
            "CODERIPPER_CACHE" => (self.cache_enabled == Some(false)).then(|| "off".to_string()),
            "CODERIPPER_CACHE_DIR" => self.cache_dir.clone(),
            "CODERIPPER_CACHE_MAX_GB" => self.cache_max_gb.map(|v| v.to_string()),
            "CODERIPPER_CACHE_WAIT_SECS" => self.cache_wait_secs.map(|v| v.to_string()),
            "CODERIPPER_TOOLS_DIR" => self.tools_dir.clone(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(path: &str, toml: &str) -> Layer {
        Layer {
            path: PathBuf::from(path),
            file: ConfigFile::parse(toml, Path::new(path)).unwrap(),
        }
    }

    #[test]
    fn a_later_layer_overrides_an_earlier_one_for_a_setting() {
        let s = Settings::from_layers(&[
            layer(
                "/etc/coderipper/.coderipper.toml",
                "deny = \"high\"\nprofile = \"extended\"\n",
            ),
            layer("/home/u/.coderipper.toml", "deny = \"low\"\n"),
        ]);
        assert_eq!(s.deny, Some(Deny::Low));
        assert_eq!(
            s.profile,
            Some(Profile::Extended),
            "a key the later file does not set keeps the earlier value"
        );
    }

    #[test]
    fn a_limit_is_the_minimum_of_every_layer_and_never_raised() {
        let mut s = Settings::from_layers(&[
            layer(
                "/etc/c.toml",
                "[limits]\nmax_runtime_minutes = 30\nmax_memory_mb = 4096\n",
            ),
            layer(
                "/home/u/c.toml",
                "[limits]\nmax_runtime_minutes = 120\nmax_memory_mb = 1024\n",
            ),
        ]);
        let get = |k: &str| match k {
            "CODERIPPER_MAX_RUNTIME_MINUTES" => Some("999".to_string()),
            "CODERIPPER_MAX_MEMORY_MB" => Some("512".to_string()),
            _ => None,
        };
        s.apply_env_limits(&get);
        let limit = |n: &str| s.limits.iter().find(|l| l.name == n).cloned().unwrap();
        assert_eq!(
            limit("max_runtime_minutes").value,
            30,
            "the home file and the environment asked for more"
        );
        assert_eq!(limit("max_runtime_minutes").source, "/etc/c.toml");
        assert_eq!(
            limit("max_memory_mb").value,
            512,
            "the environment may lower it"
        );
        assert_eq!(limit("max_memory_mb").source, "CODERIPPER_MAX_MEMORY_MB");
    }

    #[test]
    fn the_cache_and_tools_keys_stand_in_for_the_variables_they_replace() {
        let s = Settings::from_layers(&[layer(
            "/h/c.toml",
            "[cache]\nenabled = false\ndir = \"/c\"\nmax_gb = 2\nwait_secs = 9\n[tools]\ndir = \"/t\"\n",
        )]);
        assert_eq!(s.env_default("CODERIPPER_CACHE").as_deref(), Some("off"));
        assert_eq!(s.env_default("CODERIPPER_CACHE_DIR").as_deref(), Some("/c"));
        assert_eq!(
            s.env_default("CODERIPPER_CACHE_MAX_GB").as_deref(),
            Some("2")
        );
        assert_eq!(
            s.env_default("CODERIPPER_CACHE_WAIT_SECS").as_deref(),
            Some("9")
        );
        assert_eq!(s.env_default("CODERIPPER_TOOLS_DIR").as_deref(), Some("/t"));
        assert_eq!(s.env_default("HOME"), None);
        let on = Settings::from_layers(&[layer("/h/c.toml", "[cache]\nenabled = true\n")]);
        assert_eq!(
            on.env_default("CODERIPPER_CACHE"),
            None,
            "`enabled = true` is the default: nothing to say"
        );
    }

    #[test]
    fn a_repository_may_not_set_anything_but_its_allowlist() {
        let f = ConfigFile::parse(
            "profile = \"extended\"\ndeny = \"none\"\nredact = false\ninstall-tools = true\nmessage-format = \"json\"\n\
             [cache]\nenabled = false\ndir = \"x\"\nmax_gb = 1\nwait_secs = 1\n[tools]\ndir = \"y\"\n\
             [limits]\nmax_runtime_minutes = 1\nmax_memory_mb = 1\nmax_disk_mb = 1\nmax_processes = 1\n",
            Path::new("r"),
        )
        .unwrap();
        let skipped = f.skipped_for_a_repository();
        assert_eq!(
            skipped,
            [
                "deny",
                "message-format",
                "redact",
                "install-tools",
                "cache.enabled",
                "cache.dir",
                "cache.max_gb",
                "cache.wait_secs",
                "tools.dir",
                "limits.max_runtime_minutes",
                "limits.max_memory_mb",
                "limits.max_disk_mb",
                "limits.max_processes"
            ]
        );
        assert!(
            !skipped.contains(&"profile".to_string()),
            "profile is on the allowlist"
        );
    }

    #[test]
    fn an_unknown_key_or_a_wrong_value_is_an_error_naming_the_file() {
        let e =
            ConfigFile::parse("dney = \"none\"\n", Path::new("/x/.coderipper.toml")).unwrap_err();
        assert!(
            e.to_string().contains("/x/.coderipper.toml") && e.to_string().contains("dney"),
            "{e}"
        );
        let e = ConfigFile::parse("deny = \"sometimes\"\n", Path::new("/x/c.toml")).unwrap_err();
        assert!(
            e.to_string().contains("deny") && e.to_string().contains("/x/c.toml"),
            "{e}"
        );
        assert!(
            ConfigFile::parse("[limits]\nmax_cpus = 1\n", Path::new("c")).is_err(),
            "an unknown limit is an error"
        );
        assert!(ConfigFile::parse("profile = 3\n", Path::new("c")).is_err());
    }

    #[test]
    fn blocking_names_are_read_from_the_file() {
        let f = ConfigFile::parse("blocking = [\"SUP-004\"]\n", Path::new("c")).unwrap();
        assert_eq!(f.blocking, vec!["SUP-004".to_string()]);
        assert!(ConfigFile::parse("blocking = 3\n", Path::new("c")).is_err());
    }

    #[test]
    fn the_search_order_is_wide_first_and_home_last() {
        let get = |k: &str| match k {
            "HOME" => Some("/home/u".to_string()),
            "ProgramData" => Some("/pd".to_string()),
            _ => None,
        };
        let paths = search_paths(&get);
        assert_eq!(
            paths.last().unwrap(),
            &PathBuf::from("/home/u").join(FILE_NAME)
        );
        if cfg!(windows) {
            assert_eq!(paths.len(), 3);
        } else {
            assert_eq!(paths.len(), 4);
            assert_eq!(
                paths[0],
                PathBuf::from("/etc/coderipper").join(SYSTEM_FILE_NAME)
            );
            assert_eq!(paths[1], PathBuf::from("/etc/coderipper").join(FILE_NAME));
            assert_eq!(paths[2], PathBuf::from("/etc").join(FILE_NAME));
        }
    }

    #[test]
    fn no_config_reads_nothing_and_config_replaces_the_search() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        std::fs::write(home.join(FILE_NAME), "deny = \"low\"\n").unwrap();
        let h = home.display().to_string();
        let get = move |k: &str| (k == "HOME").then(|| h.clone());
        assert_eq!(load_wide(&get, None, false).unwrap().len(), 1);
        assert!(load_wide(&get, None, true).unwrap().is_empty());
        let other = home.join("other.toml");
        std::fs::write(&other, "deny = \"high\"\n").unwrap();
        let layers = load_wide(&get, Some(&other), false).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].file.deny, Some(Deny::High));
        assert!(load_wide(&get, Some(&home.join("absent.toml")), false).is_err());
    }
}
