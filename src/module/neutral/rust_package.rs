// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Rust sources of each package of a repository, parsed with `syn`: the walk that `SEC-005` and `EXT-003` share.
//!
//! A *package* is the nearest tracked `Cargo.toml` above a file. Files are read from `HEAD` through the [`RepoView`], never from the
//! disk. A file that is development tooling or test code (`efx003::is_dev_path`: `tests`, `examples`, `tools`, `build.rs`, `tests.rs` ...)
//! or sits in a vendored directory is not read; a file `syn` cannot parse is listed in `unparsed`, never read as clean.

use super::efx003::is_dev_path;
use super::{dir_and_ancestors, dir_of, join, RepoView};
use std::collections::BTreeMap;

/// Directories that hold somebody else's code.
const VENDORED: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "target",
];

/// One parsed source file.
pub(super) struct Source {
    pub path: String,
    pub text: String,
    pub file: syn::File,
}

/// A package: its directory (empty for the repository root) and its parsed sources, sorted by path.
pub(super) struct Package {
    pub dir: String,
    pub sources: Vec<Source>,
}

/// Every package with at least one source, and the files that could not be parsed.
pub(super) struct Packages {
    pub packages: Vec<Package>,
    pub unparsed: Vec<String>,
}

fn skipped(path: &str) -> bool {
    path.split('/').any(|s| VENDORED.contains(&s))
}

/// The directory of the nearest tracked `Cargo.toml` at or above the file.
fn package_dir(view: &RepoView, path: &str) -> Option<String> {
    dir_and_ancestors(dir_of(path))
        .into_iter()
        .find(|d| view.has(&join(d, "Cargo.toml")))
}

pub(super) fn load(view: &RepoView) -> Packages {
    let mut by_dir: BTreeMap<String, Vec<Source>> = BTreeMap::new();
    let mut unparsed = Vec::new();
    for path in view.paths() {
        if !path.ends_with(".rs") || skipped(path) || is_dev_path(path) {
            continue;
        }
        let Some(dir) = package_dir(view, path) else {
            continue;
        };
        let Some(text) = view.read(path) else {
            unparsed.push(path.clone());
            continue;
        };
        match syn::parse_file(&text) {
            Ok(file) => by_dir.entry(dir).or_default().push(Source {
                path: path.clone(),
                text,
                file,
            }),
            Err(_) => unparsed.push(path.clone()),
        }
    }
    Packages {
        packages: by_dir
            .into_iter()
            .map(|(dir, sources)| Package { dir, sources })
            .collect(),
        unparsed,
    }
}

/// The note the run prints for files that could not be read as Rust.
pub(super) fn unparsed_note(rule: &str, unparsed: &[String]) -> Option<String> {
    if unparsed.is_empty() {
        return None;
    }
    let shown: Vec<&str> = unparsed.iter().take(5).map(String::as_str).collect();
    Some(format!(
        "{rule}: {} file(s) could not be parsed as Rust and were not read ({}{})",
        unparsed.len(),
        shown.join(", "),
        if unparsed.len() > shown.len() {
            ", ..."
        } else {
            ""
        }
    ))
}
