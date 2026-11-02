// SPDX-License-Identifier: MIT OR Apache-2.0
//! The module graph of a crate, and the module groups of a declaration (ARC-001, second step).
//!
//! `ARC-001` judges the dependencies between the components of the declaration. Where a component is a crate that is one node of the
//! crate graph, and where a declaration splits one crate into module groups (`paths = ["src/plan.rs", "src/billing/**"]`), the edges
//! between the groups are read here, from the source, and judged with the same layers, `allow` edges and waivers.
//!
//! **What it reads.** The tracked `.rs` files under `src/` of a crate at `HEAD`, minus files of test code, reached from `src/lib.rs`
//! (or `src/main.rs` when there is no library) through `mod x;` declarations. A file is a node. A **reference** is a path written in an
//! item (test items skipped) that starts with `crate`, `self` or `super`, or with the name of a child module of the module it is
//! written in, or a `use` of one; it resolves to the file that holds the longest module path it names. An edge goes from the file
//! holding the reference to the file it resolves to, when the two differ. A file is placed in the component whose `paths` entry names it
//! (an exact file, a directory, or a pattern with `*` and `**`; the longest entry wins).
//!
//! **Known limitations / deliberate scope:**
//! - A path written inside a macro call's tokens, a path through a `use` alias or a glob re-export, a path through a type alias and an item
//!   named only by its type are not followed: the graph is a lower bound.
//! - `#[path]` modules are not read; a file `mod`-declared nowhere is not in the graph and is counted in a note.
//! - When a crate has both `src/lib.rs` and `src/main.rs`, only the library's tree is read; the binary's modules are noted.
//! - A file `syn` cannot parse is an error, never a smaller graph.

use super::api_budget::attrs_of;
use super::{is_module_path, norm, Edge, Graph, Member};
use crate::architecture::{ArchitectureConfig, ComponentDecl};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::visit::Visit;

/// The module graph of one crate: file nodes and the edges between them. Paths are relative to the crate directory.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleGraph {
    pub files: BTreeSet<String>,
    pub edges: BTreeSet<(String, String)>,
    pub notes: Vec<String>,
}

/// A path written in a module, waiting to be resolved.
struct Ref {
    file: String,
    /// The module the path is written in: `""` is the crate root, `a::b` is `crate::a::b`.
    module: String,
    segs: Vec<String>,
}

struct Walker<'a> {
    by_path: &'a BTreeMap<&'a str, &'a SourceFile>,
    /// Module path -> the file that holds it (an inline module is held by the file it is written in).
    mods: BTreeMap<String, String>,
    refs: Vec<Ref>,
    notes: Vec<String>,
    seen: BTreeSet<String>,
}

impl Walker<'_> {
    fn walk_file(
        &mut self,
        file: &SourceFile,
        child_dir: &str,
        module: &str,
    ) -> anyhow::Result<()> {
        if !self.seen.insert(file.path.clone()) {
            return Ok(());
        }
        let parsed = syn::parse_file(&file.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                file.path
            )
        })?;
        self.walk_items(&parsed.items, child_dir, &file.path, module)
    }

    fn walk_items(
        &mut self,
        items: &[syn::Item],
        child_dir: &str,
        file: &str,
        module: &str,
    ) -> anyhow::Result<()> {
        for item in items {
            if is_cfg_test(attrs_of(item)) {
                continue;
            }
            let syn::Item::Mod(m) = item else {
                let mut v = RefVisitor {
                    file,
                    module,
                    refs: &mut self.refs,
                };
                v.visit_item(item);
                continue;
            };
            let name = m.ident.to_string();
            let child = if module.is_empty() {
                name.clone()
            } else {
                format!("{module}::{name}")
            };
            if m.attrs.iter().any(|a| a.path().is_ident("path")) {
                self.notes.push(format!(
                    "module `{child}` uses #[path]; its contents are not read"
                ));
                continue;
            }
            let nested = format!("{child_dir}/{name}");
            if let Some((_, inner)) = &m.content {
                self.mods.insert(child.clone(), file.to_string());
                self.walk_items(inner, &nested, file, &child)?;
                continue;
            }
            let by_path = self.by_path;
            let flat = format!("{child_dir}/{name}.rs");
            let dir_mod = format!("{child_dir}/{name}/mod.rs");
            let found = by_path
                .get(flat.as_str())
                .or_else(|| by_path.get(dir_mod.as_str()));
            match found {
                Some(f) => {
                    self.mods.insert(child.clone(), f.path.clone());
                    self.walk_file(f, &nested, &child)?;
                }
                None => self.notes.push(format!(
                    "module `{child}` has no tracked file {flat} or {dir_mod}; its contents are not read"
                )),
            }
        }
        Ok(())
    }
}

struct RefVisitor<'a> {
    file: &'a str,
    module: &'a str,
    refs: &'a mut Vec<Ref>,
}

impl RefVisitor<'_> {
    fn push(&mut self, segs: Vec<String>) {
        if segs.is_empty() {
            return;
        }
        let keyword = matches!(segs[0].as_str(), "crate" | "self" | "super");
        if segs.len() >= 2 || keyword {
            self.refs.push(Ref {
                file: self.file.to_string(),
                module: self.module.to_string(),
                segs,
            });
        }
    }
}

impl<'ast> Visit<'ast> for RefVisitor<'_> {
    fn visit_path(&mut self, p: &'ast syn::Path) {
        self.push(p.segments.iter().map(|s| s.ident.to_string()).collect());
        syn::visit::visit_path(self, p);
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        flatten(&u.tree, &mut Vec::new(), &mut leaves);
        for l in leaves {
            self.push(l);
        }
    }
}

/// The paths a use tree names: a leaf is its whole path, a glob and `self` stand for their prefix.
fn flatten(t: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match t {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut v = prefix.clone();
            if n.ident != "self" {
                v.push(n.ident.to_string());
            }
            out.push(v);
        }
        syn::UseTree::Rename(r) => {
            let mut v = prefix.clone();
            if r.ident != "self" {
                v.push(r.ident.to_string());
            }
            out.push(v);
        }
        syn::UseTree::Glob(_) => out.push(prefix.clone()),
        syn::UseTree::Group(g) => {
            for i in &g.items {
                flatten(i, prefix, out);
            }
        }
    }
}

/// The file a path written in `module` resolves to, or `None` when it names nothing of this crate.
fn resolve(mods: &BTreeMap<String, String>, module: &str, segs: &[String]) -> Option<String> {
    let mut cur: Vec<&str> = module.split("::").filter(|s| !s.is_empty()).collect();
    let mut abs: Vec<&str> = Vec::new();
    match segs.first()?.as_str() {
        "crate" => abs.extend(segs[1..].iter().map(String::as_str)),
        "self" => {
            abs.extend(cur.iter().copied());
            abs.extend(segs[1..].iter().map(String::as_str));
        }
        "super" => {
            let mut rest = segs;
            while rest.first().map(String::as_str) == Some("super") {
                cur.pop()?;
                rest = &rest[1..];
            }
            abs.extend(cur.iter().copied());
            abs.extend(rest.iter().map(String::as_str));
        }
        first => {
            let mut cand = cur.clone();
            cand.push(first);
            if !mods.contains_key(&cand.join("::")) {
                return None;
            }
            abs.extend(cur.iter().copied());
            abs.extend(segs.iter().map(String::as_str));
        }
    }
    (0..=abs.len())
        .rev()
        .find_map(|n| mods.get(&abs[..n].join("::")).cloned())
}

/// The module graph of the crate whose tracked sources are `files` (paths relative to the crate directory).
pub(crate) fn module_graph(files: &[SourceFile]) -> anyhow::Result<ModuleGraph> {
    let by_path: BTreeMap<&str, &SourceFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let lib = by_path.get("src/lib.rs");
    let main = by_path.get("src/main.rs");
    let Some(root) = lib.or(main) else {
        return Ok(ModuleGraph {
            notes: vec![
                "no src/lib.rs or src/main.rs among the tracked sources; no module graph".into(),
            ],
            ..ModuleGraph::default()
        });
    };
    let mut w = Walker {
        by_path: &by_path,
        mods: BTreeMap::new(),
        refs: Vec::new(),
        notes: Vec::new(),
        seen: BTreeSet::new(),
    };
    w.mods.insert(String::new(), root.path.clone());
    w.walk_file(root, "src", "")?;
    if lib.is_some() && main.is_some() {
        w.notes
            .push("src/main.rs is a second crate root; its modules are not in the graph".into());
    }
    let mut edges = BTreeSet::new();
    for r in &w.refs {
        if let Some(to) = resolve(&w.mods, &r.module, &r.segs) {
            if to != r.file {
                edges.insert((r.file.clone(), to));
            }
        }
    }
    let files_in: BTreeSet<String> = w.seen.clone();
    let unreached = files.iter().filter(|f| !files_in.contains(&f.path)).count();
    let mut notes = w.notes;
    if unreached > 0 {
        notes.push(format!(
            "{unreached} tracked source file(s) are declared by no `mod` reachable from the root and are not in the graph"
        ));
    }
    Ok(ModuleGraph {
        files: files_in,
        edges,
        notes,
    })
}

/// Whether `file` (workspace-relative) is named by the `paths` entry `pattern`.
pub(crate) fn matches_path(pattern: &str, file: &str) -> bool {
    let p = norm(pattern);
    if p.contains('*') {
        return glob(p.as_bytes(), file.as_bytes());
    }
    file == p || (!p.ends_with(".rs") && file.starts_with(&format!("{p}/")))
}

/// `*` matches within a path segment, `**` across segments.
fn glob(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => {
            if let Some(rest2) = rest.strip_prefix(b"*") {
                let rest2 = rest2.strip_prefix(b"/").unwrap_or(rest2);
                (0..=s.len()).any(|i| glob(rest2, &s[i..]))
            } else {
                (0..=s.len())
                    .take_while(|&i| i == 0 || s[i - 1] != b'/')
                    .any(|i| glob(rest, &s[i..]))
            }
        }
        Some((c, rest)) => s.first() == Some(c) && glob(rest, &s[1..]),
    }
}

/// The component of the declaration that names `file`: the longest matching `paths` entry wins.
pub(crate) fn component_of<'a>(arch: &'a ArchitectureConfig, file: &str) -> Option<&'a str> {
    arch.component
        .iter()
        .flat_map(|c| c.paths.iter().map(move |p| (c, p)))
        .filter(|(_, p)| matches_path(p, file))
        .max_by_key(|(_, p)| norm(p).len())
        .map(|(c, _)| c.name.as_str())
}

/// Whether the module path `p` of a component lies inside the crate `m`.
fn inside(p: &str, m: &Member) -> bool {
    if m.dir.is_empty() {
        p.starts_with("src")
    } else {
        p.starts_with(&format!("{}/", m.dir))
    }
}

/// Adds the module groups of the declaration to `graph`: a member per group (named as the component) and an edge for every dependency
/// between two groups read from the module graph of the crate that holds them. Notes go to `graph.notes`.
pub(crate) fn add_module_groups(
    arch: &ArchitectureConfig,
    graph: &mut Graph,
    root: &Path,
) -> anyhow::Result<()> {
    let modular: Vec<&ComponentDecl> = arch
        .component
        .iter()
        .filter(|c| {
            c.paths
                .iter()
                .any(|p| is_module_path(&norm(p), &graph.members))
        })
        .collect();
    if modular.is_empty() {
        return Ok(());
    }
    let hosts: Vec<Member> = graph
        .members
        .iter()
        .filter(|m| {
            modular
                .iter()
                .any(|c| c.paths.iter().any(|p| inside(&norm(p), m)))
        })
        .cloned()
        .collect();
    let mut placed: BTreeMap<String, usize> = modular.iter().map(|c| (c.name.clone(), 0)).collect();
    let group_names: BTreeSet<&str> = modular.iter().map(|c| c.name.as_str()).collect();
    let mut manifest_of: BTreeMap<String, Member> = BTreeMap::new();
    let mut edges: BTreeSet<Edge> = BTreeSet::new();
    for m in &hosts {
        let files = read_sources(&root.join(&m.dir))?;
        let mg = module_graph(&files)?;
        let ws = |p: &str| {
            if m.dir.is_empty() {
                p.to_string()
            } else {
                format!("{}/{p}", m.dir)
            }
        };
        let comp = |f: &str| component_of(arch, &ws(f)).filter(|c| group_names.contains(*c));
        let mut unplaced = 0usize;
        for f in &mg.files {
            match comp(f) {
                Some(c) => {
                    *placed.entry(c.to_string()).or_default() += 1;
                    manifest_of
                        .entry(c.to_string())
                        .or_insert_with(|| m.clone());
                }
                None => unplaced += 1,
            }
        }
        for (a, b) in &mg.edges {
            if let (Some(ca), Some(cb)) = (comp(a), comp(b)) {
                if ca != cb {
                    edges.insert(Edge {
                        from: ca.to_string(),
                        to: cb.to_string(),
                    });
                }
            }
        }
        graph.notes.push(format!(
            "module graph of `{}`: {} file(s), {} file-to-file edge(s), {unplaced} file(s) in no module group",
            m.name,
            mg.files.len(),
            mg.edges.len()
        ));
        graph.notes.extend(
            mg.notes
                .into_iter()
                .map(|n| format!("crate `{}`: {n}", m.name)),
        );
    }
    for c in &modular {
        if graph.members.iter().any(|m| m.name == c.name) {
            graph.notes.push(format!(
                "module group `{}` has the name of a workspace member and is not judged; rename the component",
                c.name
            ));
            placed.remove(&c.name);
            continue;
        }
        let host = manifest_of.get(&c.name).or_else(|| hosts.first());
        if let Some(h) = host {
            graph.members.push(Member {
                name: c.name.clone(),
                dir: h.dir.clone(),
                manifest: h.manifest.clone(),
            });
        }
    }
    graph.members.sort_by(|a, b| a.name.cmp(&b.name));
    graph.edges.extend(
        edges
            .into_iter()
            .filter(|e| placed.contains_key(&e.from) && placed.contains_key(&e.to)),
    );
    graph.module_groups = placed;
    Ok(())
}

#[cfg(test)]
mod tests;
