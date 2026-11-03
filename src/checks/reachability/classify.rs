// SPDX-License-Identifier: MIT OR Apache-2.0
//! The class of a dead-code finding (E8, PM-approved option B of 2026-11-03).
//!
//! `reachability` reports every item `rustc`'s `dead_code` lint finds once the `pub` items are narrowed, at Medium. Two classes of what it
//! finds are not a defect of the code, and are now **Info** instead:
//!
//! - **Library API.** The item is written `pub`, is reachable through `pub` modules (or a `pub use`) from the library root, and carries
//!   documentation: it is part of the interface of a library, whose callers are outside this repository, and removing it is a breaking
//!   change. (The same reading of the public surface as `ARC-006` and `MOD-001`: `architecture/api_budget.rs`.) A `pub` item that is
//!   undocumented, or reachable through no `pub` module, stays Medium.
//! - **Test-only.** The item's name appears in the crate's own test code (a `#[cfg(test)]` item, or a file named `tests.rs`, `test.rs`,
//!   `*_test.rs`, `*_tests.rs`) and `rustc` finds no live caller in the library build, which does not compile that code (a caller that
//!   is itself dead does not count, so the name may also appear in library code that nothing reaches).
//!
//! Everything else stays Medium. `[reachability] public_api` declarations work as before (the allowlist is applied by the host after the
//! check). **Matching is by bare name**, so a dead item that shares its name with something a test calls is Info too: it is still
//! reported, one grade lower, and the text says why.
//!
//! Classes (a), (d) and (f) of the evidence (`C:\Projects\.lane-state\e8-reachability-evidence.md`: a type reached through a field or a
//! return type, a name collision between callers, a chain of dead roots) are NOT handled here.

use crate::checks::architecture::api_budget::surface;
use crate::checks::rust_sources::{read_sources, read_test_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::BTreeSet;
use std::path::Path;

/// What the source says about the items of a package.
#[derive(Debug, Default)]
pub(super) struct Facts {
    /// `(file, line)` of the public, documented declarations and inherent functions reachable through `pub` modules.
    pub api: BTreeSet<(String, usize)>,
    /// Identifiers written in test code.
    pub test_idents: BTreeSet<String>,
}

/// The class of one finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    LibraryApi,
    TestOnly,
    Plain,
}

fn idents(ts: TokenStream, out: &mut BTreeSet<String>) {
    for tt in ts {
        match tt {
            TokenTree::Ident(i) => {
                out.insert(i.to_string());
            }
            TokenTree::Group(g) => idents(g.stream(), out),
            _ => {}
        }
    }
}

fn idents_of_test_items(items: &[syn::Item], out: &mut BTreeSet<String>) {
    for item in items {
        let attrs: &[syn::Attribute] = match item {
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            syn::Item::Enum(x) => &x.attrs,
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            syn::Item::Trait(x) => &x.attrs,
            syn::Item::Type(x) => &x.attrs,
            syn::Item::Use(x) => &x.attrs,
            _ => &[],
        };
        if is_cfg_test(attrs) {
            idents(item.to_token_stream(), out);
        } else if let syn::Item::Mod(m) = item {
            if let Some((_, inner)) = &m.content {
                idents_of_test_items(inner, out);
            }
        }
    }
}

/// The facts of the package at `root` (its tracked sources at `HEAD`). `has_lib`: whether the package has a library; without one there
/// is no public surface to speak of.
pub(super) fn read_facts(root: &Path, has_lib: bool) -> anyhow::Result<Facts> {
    let mut facts = Facts::default();
    let files: Vec<SourceFile> = read_sources(root)?;
    if has_lib {
        let (totals, _) = surface(&files, "", &BTreeSet::new())?;
        facts.api = totals.documented_sites;
    }
    for f in &files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        idents_of_test_items(&parsed.items, &mut facts.test_idents);
    }
    for f in read_test_sources(root)? {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        idents(parsed.into_token_stream(), &mut facts.test_idents);
    }
    Ok(facts)
}

/// The class of the finding for `symbol` at `file:line`, which was `written_pub` (a `pub` item before the narrowing).
pub(super) fn classify(f: &Facts, file: &str, line: u32, symbol: &str, written_pub: bool) -> Class {
    if written_pub && f.api.contains(&(file.to_string(), line as usize)) {
        Class::LibraryApi
    } else if f.test_idents.contains(symbol) {
        Class::TestOnly
    } else {
        Class::Plain
    }
}

#[cfg(test)]
mod tests;
