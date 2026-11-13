// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `[architecture]` declaration: the components a project says it is made of, the direction its dependencies may run, and the
//! deliberate exceptions. Design and the accepted format: `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`.
//!
//! The declaration is **normative**: it states where things should sit, and the rules (ARC-001, later ARC-002, 003, 006, 007)
//! report where the code disagrees. It lives in the repository's own `.coderipper.toml`, read by the same loader as every other
//! key, so an unknown key is an error that names the file.
//!
//! All keys of the accepted format parse today, including those only later rules read (`volatile`, `composition`, `api_budget`,
//! `owns_data`, `invariant_types`), so a project can write its whole declaration once; a key no rule reads yet is kept and not judged.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// `[architecture]`, as written.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "parsed so a declaration may carry every key of the accepted format; the later ARC and MOD rules read them"
)]
pub(crate) struct ArchitectureConfig {
    /// Informational only: layered, hexagonal, slices, modular-monolith, pipeline, plugin, or free text.
    #[serde(default)]
    pub style: Option<String>,
    /// A long declaration may live in another file (same schema). Not read yet: setting it is an error that says so.
    #[serde(default)]
    pub file: Option<String>,
    /// `[[architecture.component]]`.
    #[serde(default)]
    pub component: Vec<ComponentDecl>,
    /// `[[architecture.layer]]`, bottom first.
    #[serde(default)]
    pub layer: Vec<LayerDecl>,
    /// `[[architecture.allow]]`: an edge the layers do not allow, with the reason it is allowed.
    #[serde(default)]
    pub allow: Vec<AllowEdge>,
    /// `[[architecture.waiver]]`: a deliberate violation, with owner and end date.
    #[serde(default)]
    pub waiver: Vec<WaiverDecl>,
    /// `[architecture.volatile]` (ARC-003/004; not read yet).
    #[serde(default)]
    pub volatile: Option<VolatileDecl>,
    /// `[architecture.composition]` (ARC-012).
    #[serde(default)]
    pub composition: Option<CompositionDecl>,
    /// `[architecture.api_budget]` (ARC-006).
    #[serde(default)]
    pub api_budget: BTreeMap<String, u32>,
    /// Once set, the number of waivers may only fall (not read yet).
    #[serde(default)]
    pub ratchet: Option<bool>,
    /// `not_applicable = [{ rule = "ARC-006", reason = "..." }]`: a rule that cannot apply to this project (a library with no adapters has
    /// no composition root to declare), said once, with the reason, instead of contriving a declaration.
    #[serde(default)]
    pub not_applicable: Vec<NotApplicableDecl>,
}

/// The rules whose declaration a project may not have anything to put in.
pub(crate) const NOT_APPLICABLE_RULES: [&str; 3] = ["ARC-006", "ARC-007", "ARC-012"];

/// One entry of `[architecture] not_applicable`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotApplicableDecl {
    pub rule: String,
    /// Required: a rule is set aside for a reason, not silently.
    pub reason: String,
}

/// Why `rule` is declared not applicable in the repository at `root`: the declared reason, worded as the rule's note. `None` when the
/// repository declares no such thing (the rule then says what is missing, "not run"), or the entry is not well formed (the declaration
/// is rejected by [`ArchitectureConfig::validate`], so a malformed entry never silences a rule).
pub(crate) fn declared_not_applicable(root: &Path, rule: &str) -> Option<String> {
    let (a, _) = load(root).ok()??;
    let d = a
        .not_applicable
        .iter()
        .find(|d| d.rule == rule && !d.reason.trim().is_empty())?;
    Some(format!(
        "[architecture] not_applicable declares {rule} not applicable to this project: {} (a declared exemption, never a clean pass)",
        d.reason.trim()
    ))
}

/// One `[[architecture.component]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ComponentDecl {
    pub name: String,
    /// Workspace-relative crate directories, or module paths inside a crate.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub volatility: Option<Volatility>,
    #[serde(default)]
    pub owns_data: Vec<String>,
    #[serde(default)]
    pub invariant_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Role {
    Policy,
    Adapter,
    Infrastructure,
    Composition,
    TestSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Volatility {
    Stable,
    Volatile,
}

/// One `[[architecture.layer]]`: a component may depend on its own layer or any layer BELOW it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayerDecl {
    pub name: String,
    pub components: Vec<String>,
}

/// One `[[architecture.allow]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AllowEdge {
    pub from: String,
    pub to: String,
    /// Required: an allowed cross-edge says why.
    pub reason: String,
}

/// One `[[architecture.waiver]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WaiverDecl {
    /// `[from, to]`.
    pub edge: [String; 2],
    pub reason: String,
    pub owner: String,
    /// `YYYY-MM-DD`. Required: an expired waiver is a finding, not silence.
    pub until: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VolatileDecl {
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub namespaces: Vec<String>,
    #[serde(default)]
    pub feature_cfg_in_policy: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompositionDecl {
    /// Files (or directories, or `*` globs) where concrete adapters may be built.
    #[serde(default)]
    pub roots: Vec<String>,
    /// Names of the traits that are the project's ports. Empty: every trait declared in a workspace crate is one.
    #[serde(default)]
    pub ports: Vec<String>,
}

/// A calendar date, enough to say whether a waiver has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Parses `YYYY-MM-DD`; `None` when it is not a real date.
    pub(crate) fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim().split('-');
        let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return None;
        }
        let (year, month, day): (i32, u32, u32) =
            (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days)
            .contains(&day)
            .then_some(Self { year, month, day })
    }

    /// Today in UTC, from the system clock.
    pub(crate) fn today() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self::from_unix_days((secs / 86_400) as i64)
    }

    /// Howard Hinnant's `civil_from_days`.
    fn from_unix_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl ArchitectureConfig {
    /// Checks what the schema cannot: every name used is declared, a name is declared once, a layer names a component once, a waiver's
    /// date is a date. The error names the file. (A layer may name a workspace member that has no `[[component]]` block: components
    /// default to one per member named by a layer, so `members` are the names a layer or an edge may use besides declared components.)
    pub(crate) fn validate(&self, members: &BTreeSet<String>, file: &Path) -> anyhow::Result<()> {
        let bad = |what: String| anyhow::anyhow!("{}: [architecture] {what}", file.display());
        if self.file.is_some() {
            return Err(bad(
                "`file = ...` is not read yet: put the whole declaration in .coderipper.toml"
                    .into(),
            ));
        }
        let mut set_aside: BTreeSet<&str> = BTreeSet::new();
        for d in &self.not_applicable {
            if !NOT_APPLICABLE_RULES.contains(&d.rule.as_str()) {
                return Err(bad(format!(
                    "not_applicable names `{}`; only {} may be declared not applicable",
                    d.rule,
                    NOT_APPLICABLE_RULES.join(", ")
                )));
            }
            if d.reason.trim().is_empty() {
                return Err(bad(format!("not_applicable for {} needs a reason", d.rule)));
            }
            if !set_aside.insert(d.rule.as_str()) {
                return Err(bad(format!("declares {} not applicable twice", d.rule)));
            }
        }
        let mut declared: BTreeSet<&str> = BTreeSet::new();
        for c in &self.component {
            if !declared.insert(c.name.as_str()) {
                return Err(bad(format!("declares the component `{}` twice", c.name)));
            }
        }
        let known = |name: &str| declared.contains(name) || members.contains(name);
        let mut layer_names = BTreeSet::new();
        let mut placed: BTreeSet<&str> = BTreeSet::new();
        for l in &self.layer {
            if !layer_names.insert(l.name.as_str()) {
                return Err(bad(format!("declares the layer `{}` twice", l.name)));
            }
            for c in &l.components {
                if !known(c) {
                    return Err(bad(format!(
                        "layer `{}` names `{c}`, which is neither a declared component nor a workspace member",
                        l.name
                    )));
                }
                if !placed.insert(c.as_str()) {
                    return Err(bad(format!("places `{c}` in more than one layer")));
                }
            }
        }
        for a in &self.allow {
            for end in [&a.from, &a.to] {
                if !known(end) {
                    return Err(bad(format!(
                        "an `allow` edge names `{end}`, which is neither a declared component nor a workspace member"
                    )));
                }
            }
            if a.reason.trim().is_empty() {
                return Err(bad(format!(
                    "the `allow` edge {} -> {} has an empty reason",
                    a.from, a.to
                )));
            }
        }
        for w in &self.waiver {
            for end in &w.edge {
                if !known(end) {
                    return Err(bad(format!(
                        "a waiver names `{end}`, which is neither a declared component nor a workspace member"
                    )));
                }
            }
            if Date::parse(&w.until).is_none() {
                return Err(bad(format!(
                    "the waiver {} -> {} has `until = \"{}\"`, which is not a date (YYYY-MM-DD)",
                    w.edge[0], w.edge[1], w.until
                )));
            }
            if w.reason.trim().is_empty() || w.owner.trim().is_empty() {
                return Err(bad(format!(
                    "the waiver {} -> {} needs a reason and an owner",
                    w.edge[0], w.edge[1]
                )));
            }
        }
        Ok(())
    }
}

/// The repository's `[architecture]` table, from `<root>/.coderipper.toml`; `None` when there is no file or no table. An unreadable or
/// invalid file is an error naming the file.
pub(crate) fn load(
    root: &Path,
) -> anyhow::Result<Option<(ArchitectureConfig, std::path::PathBuf)>> {
    let path = root.join(crate::config::FILE_NAME);
    let Some(file) = crate::config::read_file(&path, false)? else {
        return Ok(None);
    };
    Ok(file.architecture.map(|a| (a, path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> ArchitectureConfig {
        crate::config::ConfigFile::parse(toml, Path::new(".coderipper.toml"))
            .unwrap()
            .architecture
            .unwrap()
    }

    fn members(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn not_applicable_needs_a_known_rule_a_reason_and_no_repeat() {
        let check = |entries: &str| {
            parse(&format!("[architecture]\nnot_applicable = [{entries}]\n"))
                .validate(&members(&[]), Path::new("c"))
        };
        assert!(check(r#"{ rule = "ARC-006", reason = "no adapters" }"#).is_ok());
        let e = check(r#"{ rule = "ARC-001", reason = "x" }"#).unwrap_err();
        assert!(e.to_string().contains("only ARC-006"), "{e}");
        let e = check(r#"{ rule = "ARC-007", reason = "  " }"#).unwrap_err();
        assert!(e.to_string().contains("needs a reason"), "{e}");
        let e = check(r#"{ rule = "ARC-012", reason = "a" }, { rule = "ARC-012", reason = "b" }"#)
            .unwrap_err();
        assert!(e.to_string().contains("twice"), "{e}");
    }

    #[test]
    fn a_declared_exemption_is_read_for_its_rule_only() {
        let dir = tempfile::tempdir().unwrap();
        // positive control: nothing declared, nothing found
        assert!(declared_not_applicable(dir.path(), "ARC-006").is_none());
        std::fs::write(
            dir.path().join(".coderipper.toml"),
            "[architecture]\nnot_applicable = [{ rule = \"ARC-006\", reason = \"a library with no adapters\" }]\n",
        )
        .unwrap();
        let why = declared_not_applicable(dir.path(), "ARC-006").expect("declared");
        assert!(why.contains("a library with no adapters"), "{why}");
        assert!(declared_not_applicable(dir.path(), "ARC-007").is_none());
    }

    #[test]
    fn the_smallest_declaration_parses_and_validates() {
        let a = parse(
            "[architecture]\nstyle = \"layered\"\n[[architecture.layer]]\nname = \"lib\"\ncomponents = [\"mylib\"]\n[[architecture.layer]]\nname = \"bin\"\ncomponents = [\"mylib-cli\"]\n",
        );
        a.validate(&members(&["mylib", "mylib-cli"]), Path::new("c"))
            .unwrap();
    }

    #[test]
    fn an_unknown_key_is_an_error_naming_the_file() {
        let e = crate::config::ConfigFile::parse(
            "[architecture]\nlayers = []\n",
            Path::new("/r/.coderipper.toml"),
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("/r/.coderipper.toml") && e.contains("layers"),
            "{e}"
        );
    }

    #[test]
    fn a_name_used_but_not_declared_is_an_error() {
        let a = parse(
            "[architecture]\n[[architecture.layer]]\nname = \"x\"\ncomponents = [\"ghost\"]\n",
        );
        let e = a
            .validate(&members(&["real"]), Path::new("c"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("ghost"), "{e}");
    }

    #[test]
    fn a_component_in_two_layers_is_an_error() {
        let a = parse(
            "[architecture]\n[[architecture.layer]]\nname = \"a\"\ncomponents = [\"m\"]\n[[architecture.layer]]\nname = \"b\"\ncomponents = [\"m\"]\n",
        );
        assert!(a.validate(&members(&["m"]), Path::new("c")).is_err());
    }

    #[test]
    fn an_allow_edge_needs_a_reason() {
        let a = parse(
            "[architecture]\n[[architecture.allow]]\nfrom = \"a\"\nto = \"b\"\nreason = \" \"\n",
        );
        assert!(a.validate(&members(&["a", "b"]), Path::new("c")).is_err());
        let missing = crate::config::ConfigFile::parse(
            "[architecture]\n[[architecture.allow]]\nfrom = \"a\"\nto = \"b\"\n",
            Path::new("c"),
        );
        assert!(missing.is_err(), "a reason is required, not defaulted");
    }

    #[test]
    fn a_waiver_needs_a_real_end_date() {
        let w = |until: &str| {
            parse(&format!(
                "[architecture]\n[[architecture.waiver]]\nedge = [\"a\", \"b\"]\nreason = \"r\"\nowner = \"o\"\nuntil = \"{until}\"\n"
            ))
            .validate(&members(&["a", "b"]), Path::new("c"))
        };
        assert!(w("2027-03-01").is_ok());
        assert!(w("2027-02-30").is_err());
        assert!(w("soon").is_err());
        let none = crate::config::ConfigFile::parse(
            "[architecture]\n[[architecture.waiver]]\nedge = [\"a\", \"b\"]\nreason = \"r\"\nowner = \"o\"\n",
            Path::new("c"),
        );
        assert!(none.is_err(), "`until` is required");
    }

    #[test]
    fn the_keys_later_rules_read_parse_today() {
        let a = parse(
            "[architecture]\n[architecture.volatile]\nnames = [\"BackendId\"]\nfeature_cfg_in_policy = true\n[architecture.composition]\nroots = [\"src/main.rs\"]\n[architecture.api_budget]\ndomain = 40\n",
        );
        assert_eq!(a.api_budget.get("domain"), Some(&40));
    }

    #[test]
    fn file_is_refused_until_it_is_read() {
        let a = parse("[architecture]\nfile = \"architecture.toml\"\n");
        let e = a
            .validate(&members(&[]), Path::new("c"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not read yet"), "{e}");
    }

    #[test]
    fn dates_parse_compare_and_the_clock_gives_a_plausible_one() {
        assert!(Date::parse("2027-03-01") > Date::parse("2026-12-31"));
        assert_eq!(Date::parse("2024-02-29").map(|d| d.day), Some(29));
        assert!(Date::parse("2025-02-29").is_none());
        assert_eq!(Date::from_unix_days(0).to_string(), "1970-01-01");
        assert_eq!(Date::from_unix_days(20_000).to_string(), "2024-10-04");
        assert!(Date::today().year >= 2026);
    }
}
