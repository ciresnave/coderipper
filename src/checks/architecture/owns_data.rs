// SPDX-License-Identifier: MIT OR Apache-2.0
//! Data owned by one component (repository scope, Rust): does a component other than the owner name a table, collection or topic the
//! project declared another component owns?
//!
//! Rule record: `ARC-007` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md` (`owns_data` on a component); background: `docs/rules/arc.md`.
//!
//! **What it counts.** For every component that is not the owner of a declared name, in the tracked files of each of its crates at `HEAD`:
//! - **String literals in `.rs` files under `src/`** (test items skipped; literals inside macro calls such as `sqlx::query!` and
//!   `format!` are read too): a literal that is exactly the name (`"orders"`, a collection or topic argument) or a literal shaped like
//!   SQL (a word among `select`, `insert`, `update`, `delete`, `join`, `into`, `table`, `alter`, `truncate`, `drop`, `create`) that
//!   holds the name as a whole word (case-insensitively, so `"orders"` quoted inside the statement counts).
//! - **`.sql` files** anywhere under the crate directory: the name as a whole word, case-insensitively, outside `--` comments.
//!
//! One finding per (owning component, name, other component), Low, with the count and the files. Two components that declare the same
//! name are reported at Info (an ownership that is not single is not an ownership).
//!
//! **Weak evidence by construction (low confidence).** The match is on text, not on a query: a literal `"orders"` used as a log label
//! counts, a name built at run time (`format!("{prefix}_orders")`) or read from configuration is NOT SEEN, an ORM that maps a type to a
//! table is NOT SEEN, a non-Rust crate is not read, and a name that is also an ordinary word in an SQL-shaped message
//! (`"update the orders"`) is counted. Allowlist a component with the reason when a hit is deliberate.
//!
//! **No declaration, or no component with `owns_data`, is not clean:** the rule reports "not run" with what to add.
//!
//! **Known limitations / deliberate scope:**
//! - Crate granularity (as ARC-001): a non-owner component of module paths inside a crate is noted, not judged.
//! - Only `src/` of a crate is read for Rust; `tests/`, `benches/`, `examples/` and `build.rs` are not.
//! - An owner may use its own name anywhere; whether the declared owner is the right one is not judged.

use super::api_budget::attrs_of;
use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::read_sources;
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_007;

/// Reports a component that names data another component owns (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct DataOwnershipCheck;

impl DataOwnershipCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-007 has no owned data to look for; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into());
    };
    if a.declares_none(crate::architecture::NoneInput::OwnsData) {
        return None;
    }
    if !a.component.iter().any(|c| !c.owns_data.is_empty()) {
        return Some("no [[architecture.component]] declares owns_data (the tables, collections or topics only that component may name), so there is no ownership to judge (docs/rules/arc.md)".into());
    }
    None
}

impl Check for DataOwnershipCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn not_configured(&self, ctx: &CheckContext) -> Option<String> {
        match architecture::load(&ctx.project_root) {
            Ok(Some((a, _))) => missing_declaration(Some(&a)),
            Ok(None) => missing_declaration(None),
            // an unreadable declaration is an error of the run, raised by `run`
            Err(_) => None,
        }
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let Some((arch, file)) = architecture::load(&ctx.project_root)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        let graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        if arch.declares_none(crate::architecture::NoneInput::OwnsData) {
            return Ok((
                Vec::new(),
                vec![crate::architecture::none_note(
                    CHECK_ID,
                    crate::architecture::NoneInput::OwnsData,
                )],
            ));
        }
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = Resolved::new(&arch, &graph.members);
        let owned = owned_names(&arch);
        let mut notes = Vec::new();
        let mut uses: Vec<Use> = Vec::new();
        for c in &arch.component {
            if c.paths
                .iter()
                .any(|p| is_module_path(&norm(p), &graph.members))
            {
                if !owned.is_empty() {
                    notes.push(format!(
                        "component `{}` has module paths inside a crate; module groups are not judged in this version, only whole crates",
                        c.name
                    ));
                }
                continue;
            }
            // what to look for here: every name some other component owns
            let wanted: BTreeSet<&str> = owned
                .iter()
                .filter(|(_, owners)| !owners.contains(c.name.as_str()))
                .map(|(n, _)| n.as_str())
                .collect();
            if wanted.is_empty() {
                continue;
            }
            let mut sources = 0usize;
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                let dir = ctx.project_root.join(&m.dir);
                let prefix = |p: &str| {
                    if m.dir.is_empty() {
                        p.to_string()
                    } else {
                        format!("{}/{p}", m.dir)
                    }
                };
                for f in read_sources(&dir)? {
                    sources += 1;
                    let lits = literals(&f.text).map_err(|e| {
                        anyhow::anyhow!(
                            "could not parse {} with syn ({e}); refusing to guess",
                            prefix(&f.path)
                        )
                    })?;
                    for name in &wanted {
                        let n = lits.iter().filter(|l| literal_names(l, name)).count();
                        if n > 0 {
                            uses.push(Use {
                                name: (*name).to_string(),
                                component: c.name.clone(),
                                file: prefix(&f.path),
                                count: n,
                                manifest: m.manifest.clone(),
                            });
                        }
                    }
                }
                for (path, text) in read_sql(&dir)? {
                    sources += 1;
                    for name in &wanted {
                        let n = sql_mentions(&text, name);
                        if n > 0 {
                            uses.push(Use {
                                name: (*name).to_string(),
                                component: c.name.clone(),
                                file: prefix(&path),
                                count: n,
                                manifest: m.manifest.clone(),
                            });
                        }
                    }
                }
            }
            notes.push(format!(
                "component `{}`: {sources} source file(s) read for {} owned name(s) of other components",
                c.name,
                wanted.len()
            ));
        }
        Ok(judge(&owned, uses, &project, notes))
    }
}

/// The names declared owned, each with the components that declare it.
fn owned_names(a: &ArchitectureConfig) -> BTreeMap<String, BTreeSet<&str>> {
    let mut out: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for c in &a.component {
        for n in &c.owns_data {
            out.entry(n.clone()).or_default().insert(c.name.as_str());
        }
    }
    out
}

/// One place a component names another's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Use {
    pub name: String,
    pub component: String,
    pub file: String,
    pub count: usize,
    pub manifest: String,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `text` holds `name` as a whole word, ignoring case.
fn has_word(text: &str, name: &str) -> bool {
    text.split(|c: char| !is_word(c))
        .any(|w| w.eq_ignore_ascii_case(name))
}

const SQL_WORDS: &[&str] = &[
    "select", "insert", "update", "delete", "join", "into", "table", "alter", "truncate", "drop",
    "create",
];

/// Whether a string literal names `name`: it is exactly the name, or it reads as SQL and holds the name as a whole word.
pub(crate) fn literal_names(lit: &str, name: &str) -> bool {
    if lit.trim() == name {
        return true;
    }
    let sqlish = lit
        .split(|c: char| !is_word(c))
        .any(|w| SQL_WORDS.iter().any(|k| w.eq_ignore_ascii_case(k)));
    sqlish && has_word(lit, name)
}

/// The number of lines of an `.sql` file, outside `--` comments, that hold `name` as a whole word.
pub(crate) fn sql_mentions(text: &str, name: &str) -> usize {
    text.lines()
        .filter(|l| has_word(l.split("--").next().unwrap_or(""), name))
        .count()
}

/// Every string literal in `src`, outside test items, including those inside macro calls.
pub(crate) fn literals(src: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(src)?;
    let mut out = Vec::new();
    collect(&file.items, &mut out);
    Ok(out)
}

fn collect(items: &[syn::Item], out: &mut Vec<String>) {
    for i in items {
        if is_cfg_test(attrs_of(i)) {
            continue;
        }
        match i {
            syn::Item::Mod(m) if m.content.is_some() => {
                if let Some((_, inner)) = &m.content {
                    collect(inner, out);
                }
            }
            _ => walk(i.to_token_stream(), out),
        }
    }
}

fn walk(ts: TokenStream, out: &mut Vec<String>) {
    for tt in ts {
        match tt {
            TokenTree::Group(g) => walk(g.stream(), out),
            TokenTree::Literal(l) => {
                if let Ok(s) = syn::parse_str::<syn::LitStr>(&l.to_string()) {
                    out.push(s.value());
                }
            }
            _ => {}
        }
    }
}

/// The tracked `.sql` files under `dir` at `HEAD`, as `(path relative to dir, text)`.
fn read_sql(dir: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let listing = git(
        dir,
        &["ls-tree", "-r", "-z", "--name-only", "HEAD", "--", "."],
    )?;
    let mut out = Vec::new();
    for path in String::from_utf8_lossy(&listing).split('\0') {
        if !path.to_ascii_lowercase().ends_with(".sql") {
            continue;
        }
        let blob = git(dir, &["show", &format!("HEAD:./{path}")])?;
        // a file that is not UTF-8 is not text we can read; it is skipped, and it is rare for a migration
        if let Ok(text) = String::from_utf8(blob) {
            out.push((path.to_string(), text));
        }
    }
    Ok(out)
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    anyhow::ensure!(
        out.status.success(),
        "git {} failed in {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

fn owners_of(owned: &BTreeMap<String, BTreeSet<&str>>, name: &str) -> String {
    owned
        .get(name)
        .map(|s| s.iter().copied().collect::<Vec<_>>().join("`, `"))
        .unwrap_or_default()
}

/// The findings and notes for the uses found.
pub(crate) fn judge(
    owned: &BTreeMap<String, BTreeSet<&str>>,
    uses: Vec<Use>,
    project: &str,
    mut notes: Vec<String>,
) -> (Vec<Finding>, Vec<String>) {
    let mut findings = Vec::new();
    for (name, owners) in owned {
        if owners.len() > 1 {
            findings.push(Finding {
                check_id: CHECK_ID.into(),
                claim: Claim::Presence,
                severity: Severity::Info,
                confidence: Confidence::High,
                project: project.into(),
                location: None,
                subject: Some(format!("{name}:owners")),
                summary: format!(
                    "`{name}` is declared owned by {} components: `{}`",
                    owners.len(),
                    owners.iter().copied().collect::<Vec<_>>().join("`, `")
                ),
                detail: format!(
                    "owns_data names `{name}` on more than one component, so no component owns it. Keep it on the one component that does and let the others go through its interface. A component of the declared owners is not judged for `{name}`, so none of them is checked against the others."
                ),
                member: None,
                positive_control: None,
            });
        }
    }
    // one finding per (name, other component)
    let mut grouped: BTreeMap<(String, String), Vec<&Use>> = BTreeMap::new();
    for u in &uses {
        grouped
            .entry((u.name.clone(), u.component.clone()))
            .or_default()
            .push(u);
    }
    for ((name, component), us) in &grouped {
        let total: usize = us.iter().map(|u| u.count).sum();
        let mut files: Vec<&Use> = us.clone();
        files.sort_by(|a, b| b.count.cmp(&a.count).then(a.file.cmp(&b.file)));
        let shown = files
            .iter()
            .take(5)
            .map(|u| format!("`{}` x{}", u.file, u.count))
            .collect::<Vec<_>>()
            .join(", ");
        let owner = owners_of(owned, name);
        findings.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Low,
            confidence: Confidence::Low,
            project: project.into(),
            location: us.first().map(|u| Location {
                file: u.manifest.clone(),
                line: None,
            }),
            subject: Some(format!("{component}:{name}")),
            summary: format!(
                "the component `{component}` names `{name}`, which `{owner}` owns: {total} place(s) in {} file(s)",
                us.len()
            ),
            detail: format!(
                "`{name}` is declared in owns_data of `{owner}`, so only that component should name it. In `{component}`: {shown}. The match is on text (a string literal that is the name or an SQL-shaped literal holding it, or an `.sql` file naming it), so a log label or a message can match, and a name built at run time or reached through an ORM is not seen. Route the access through `{owner}`'s interface, move the ownership if the split is wrong, or allowlist the subject `{component}:{name}` with a reason."
            ),
            member: None,
            positive_control: None,
        });
    }
    findings.sort_by(|a, b| a.subject.cmp(&b.subject));
    if !owned.is_empty() {
        notes.push(format!(
            "ARC-007 owned names: {}",
            owned
                .iter()
                .map(|(n, o)| format!("{n} ({})", o.iter().copied().collect::<Vec<_>>().join(", ")))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    (findings, notes)
}

#[cfg(test)]
mod tests;
