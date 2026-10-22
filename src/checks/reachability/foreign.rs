// SPDX-License-Identifier: MIT OR Apache-2.0
//! Which files does the LIBRARY compile, and which identifiers do every OTHER target's files mention?
//!
//! The reachability check downgrades the lib's `pub` items and builds only the lib (a bin or test
//! importing the lib by name would fail to compile against the downgraded items). So a lib item used
//! only by a bin, an integration test, an example or a bench looks dead to that build. This module
//! finds the files that are NOT part of the lib, and the identifiers they mention, so the caller can
//! decline to report an item one of them names ("rescue"). Name-based on purpose: it can only
//! over-rescue (hide a dead item that shares a name with something used elsewhere), never report a
//! live item as dead.

use std::collections::BTreeSet;
use std::path::Path;

/// Every file under `root` that `src/lib.rs` compiles, following `mod x;` declarations the way
/// rustc does (`x.rs`, `x/mod.rs`, a non-mod-rs file's own directory, inline `mod a { mod b; }`,
/// `#[path = "..."]`). Paths are `src/...` with forward slashes. A declared module whose file does
/// not exist is skipped (it may be `cfg`'d out). A file that is in the lib's tree but cannot be
/// parsed is an error: the lib must compile, so this is a bug in `syn`'s coverage, not a skip.
pub fn lib_module_files(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    module_files_from(root, "src/lib.rs")
}

/// The files a crate root `entry` (`src/lib.rs`, `tools/tool.rs`, ...) compiles, itself included.
pub fn module_files_from(root: &Path, entry: &str) -> anyhow::Result<BTreeSet<String>> {
    let mut walk = Walk {
        root,
        seen: BTreeSet::new(),
    };
    walk.file(entry, true)?;
    Ok(walk.seen)
}

struct Walk<'a> {
    root: &'a Path,
    seen: BTreeSet<String>,
}

impl Walk<'_> {
    /// `is_mod_rs`: the file is a crate root or a `mod.rs`, so its child modules live next to it;
    /// any other file `foo.rs` keeps its child modules in `foo/`.
    fn file(&mut self, rel: &str, is_mod_rs: bool) -> anyhow::Result<()> {
        if !self.seen.insert(rel.to_string()) {
            return Ok(());
        }
        let source = std::fs::read_to_string(self.root.join(rel))?;
        let parsed = syn::parse_file(&source)
            .map_err(|e| anyhow::anyhow!("could not parse {rel} with syn ({e})"))?;
        let dir = parent(rel);
        let mod_dir = if is_mod_rs {
            dir.clone()
        } else {
            join(&dir, file_stem(rel))
        };
        self.items(&parsed.items, &dir, &mod_dir, false)
    }

    fn items(
        &mut self,
        items: &[syn::Item],
        file_dir: &str,
        mod_dir: &str,
        inside_inline: bool,
    ) -> anyhow::Result<()> {
        for item in items {
            let syn::Item::Mod(m) = item else { continue };
            let name = m.ident.to_string();
            let name = name.trim_start_matches("r#").to_string();
            let path_attr = path_attribute(&m.attrs);

            if let Some((_, inner)) = &m.content {
                // `mod a { ... }`: children live in a directory named after it (or its #[path]).
                let component = path_attr.unwrap_or(name);
                let child_dir = join(mod_dir, &component);
                self.items(inner, file_dir, &child_dir, true)?;
                continue;
            }

            // `mod a;`: a #[path] outside any inline module is relative to the declaring file's
            // directory; inside one it is relative to the module directory.
            let candidates: Vec<(String, bool)> = match path_attr {
                Some(p) => {
                    let base = if inside_inline { mod_dir } else { file_dir };
                    vec![(join(base, &p), true)]
                }
                None => vec![
                    (join(mod_dir, &format!("{name}.rs")), false),
                    (join(&join(mod_dir, &name), "mod.rs"), true),
                ],
            };
            if let Some((rel, is_mod_rs)) = candidates
                .into_iter()
                .find(|(rel, _)| self.root.join(rel).is_file())
            {
                self.file(&rel, is_mod_rs)?;
            }
        }
        Ok(())
    }
}

fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|a| {
        if !a.path().is_ident("path") {
            return None;
        }
        match &a.meta.require_name_value().ok()?.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s.value()),
            _ => None,
        }
    })
}

fn parent(rel: &str) -> String {
    rel.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

fn file_stem(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".rs").unwrap_or(name)
}

fn join(dir: &str, name: &str) -> String {
    // `a/b/../c` is left as written: the OS resolves it when the file is read.
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Every identifier mentioned by a `.rs` file that is not part of the lib: all of `tests/`,
/// `examples/` and `benches/`, every file under `src/` that `lib_files` does not contain
/// (`src/main.rs`, `src/bin/**`, and modules only a bin declares), and every non-lib target the
/// manifest declares with a custom `path`, together with the modules it declares. Files are read
/// lossily: a stray non-UTF-8 byte must not fail the check.
pub fn foreign_identifiers(
    root: &Path,
    lib_files: &BTreeSet<String>,
) -> anyhow::Result<BTreeSet<String>> {
    let files = package_files(root, &non_lib_target_files(root)?)?;
    identifiers_of(root, files.difference(lib_files))
}

/// Every `.rs` file of the package at `root` that any of its targets can compile: all of `src/`,
/// `tests/`, `examples/` and `benches/`, plus the module tree of each target entry in `entries`
/// (relative to `root`, forward slashes; custom `path = "..."` targets included).
fn package_files(root: &Path, entries: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for top in ["src", "tests", "examples", "benches"] {
        let dir = root.join(top);
        if !dir.is_dir() {
            continue;
        }
        for path in crate::worktree::walk_rs_files(&dir)? {
            files.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
    }
    for entry in entries {
        // A target's own module tree; if the entry cannot be parsed, its own text still counts.
        match module_files_from(root, entry) {
            Ok(tree) => files.extend(tree),
            Err(_) => {
                files.insert(entry.clone());
            }
        }
    }
    Ok(files)
}

fn identifiers_of<'a>(
    root: &Path,
    files: impl IntoIterator<Item = &'a String>,
) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for rel in files {
        let bytes = std::fs::read(root.join(rel))?;
        out.extend(identifiers_in(&String::from_utf8_lossy(&bytes)));
    }
    Ok(out)
}

/// The workspace members that can name the package's items, and what they mention.
#[derive(Default)]
pub struct DependentMembers {
    /// Every identifier any of those members' files mention (lossy, like [`foreign_identifiers`]).
    pub identifiers: BTreeSet<String>,
    /// Their package names, sorted.
    pub names: Vec<String>,
}

/// Every identifier mentioned by the OTHER workspace members that depend on the package at `root`,
/// directly or through other members (`path` dependencies of any kind: normal, dev, build). Only those
/// can name this package's items, so a sibling that does not depend on it rescues nothing. All of a
/// member's targets count (its library, bins, tests, examples, benches), because a member's library
/// can re-export an item to a third member. Name-based like the rest of the rescue, so it can only
/// over-rescue. A package that is not in a workspace with other members yields nothing.
pub fn dependent_member_identifiers(root: &Path) -> anyhow::Result<DependentMembers> {
    let metadata = cargo_metadata(root)?;
    let root = root.canonicalize()?;
    let packages: Vec<&serde_json::Value> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let dir_of = |p: &serde_json::Value| -> Option<std::path::PathBuf> {
        Path::new(p["manifest_path"].as_str()?)
            .parent()?
            .canonicalize()
            .ok()
    };
    let Some(me) = packages
        .iter()
        .position(|p| dir_of(p).is_some_and(|d| d == root))
    else {
        return Ok(DependentMembers::default());
    };
    let index_by_name: std::collections::BTreeMap<&str, usize> = packages
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((p["name"].as_str()?, i)))
        .collect();

    // A dependency written with a version and redirected to a workspace member by the workspace
    // root's `[patch]` or `[replace]` table has no `path` in `cargo metadata --no-deps`, so it is
    // recovered from the manifest.
    let redirected = redirected_members(&metadata, &packages, &dir_of);

    // dependents[i] = the members that depend on member i by path (or by a patch to its directory).
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); packages.len()];
    for (i, package) in packages.iter().enumerate() {
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            let Some(name) = dep["name"].as_str() else {
                continue;
            };
            if dep["path"].is_null() && !redirected.contains(name) {
                continue;
            }
            if let Some(&target) = index_by_name.get(name) {
                dependents[target].push(i);
            }
        }
    }
    let mut seen = vec![false; packages.len()];
    let mut queue = vec![me];
    seen[me] = true;
    while let Some(i) = queue.pop() {
        for &d in &dependents[i] {
            if !seen[d] {
                seen[d] = true;
                queue.push(d);
            }
        }
    }

    let mut out = DependentMembers::default();
    for (i, package) in packages.iter().enumerate() {
        if i == me || !seen[i] {
            continue;
        }
        // A dependent that cannot be read must not silently shrink the rescue set (a live item would be
        // reported dead): fail the run instead.
        let Some(dir) = dir_of(package) else {
            anyhow::bail!(
                "cannot read the directory of workspace member {}",
                package["name"].as_str().unwrap_or("?")
            );
        };
        let entries = target_entries(package, &dir, true);
        let files = package_files(&dir, &entries)?;
        out.identifiers.extend(identifiers_of(&dir, &files)?);
        if let Some(name) = package["name"].as_str() {
            out.names.push(name.to_string());
        }
    }
    out.names.sort();
    out.names.dedup();
    Ok(out)
}

/// Names of the workspace members that the workspace root manifest's `[patch.*]` or `[replace]` table
/// redirects to the member's own directory by `path`. Only these dependencies are recovered: a patch to
/// a git source, to a path outside the workspace, or in a `.cargo/config.toml` is not seen (a dependent
/// that reaches the package that way still rescues nothing, as before).
fn redirected_members(
    metadata: &serde_json::Value,
    packages: &[&serde_json::Value],
    dir_of: &dyn Fn(&serde_json::Value) -> Option<std::path::PathBuf>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(workspace_root) = metadata["workspace_root"].as_str() else {
        return out;
    };
    let workspace_root = Path::new(workspace_root);
    let Ok(text) = std::fs::read_to_string(workspace_root.join("Cargo.toml")) else {
        return out;
    };
    let Ok(manifest) = text.parse::<toml::Table>() else {
        return out;
    };
    for relative in patched_paths(&manifest) {
        let Ok(target) = workspace_root.join(relative).canonicalize() else {
            continue;
        };
        for package in packages {
            if dir_of(package).is_some_and(|d| d == target) {
                if let Some(name) = package["name"].as_str() {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

/// The `path` of every entry in a manifest's `[patch.<source>]` and `[replace]` tables, as written.
fn patched_paths(manifest: &toml::Table) -> Vec<String> {
    let mut tables: Vec<&toml::Table> = Vec::new();
    if let Some(patch) = manifest.get("patch").and_then(|v| v.as_table()) {
        tables.extend(patch.values().filter_map(|v| v.as_table()));
    }
    if let Some(replace) = manifest.get("replace").and_then(|v| v.as_table()) {
        tables.push(replace);
    }
    tables
        .into_iter()
        .flat_map(|t| t.values())
        .filter_map(|entry| entry.as_table()?.get("path")?.as_str().map(str::to_string))
        .collect()
}

/// `src_path` of every target of the package at `root` that is not the library (bins, tests,
/// examples, benches, the build script), relative to `root` with forward slashes, as cargo itself
/// reports them (so custom `path = "..."` entries are found).
fn non_lib_target_files(root: &Path) -> anyhow::Result<Vec<String>> {
    let metadata = cargo_metadata(root)?;
    let root = root.canonicalize()?;
    let mut out = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        out.extend(target_entries(package, &root, false));
    }
    Ok(out)
}

/// `cargo metadata --no-deps` for the package at `root`: every member of its workspace.
fn cargo_metadata(root: &Path) -> anyhow::Result<serde_json::Value> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The `src_path` of the targets of one metadata `package` that lie under `dir` (canonical), relative
/// to it with forward slashes; the library kinds only when `include_lib`. A target whose file is
/// missing or outside `dir` is skipped, not an error.
fn target_entries(package: &serde_json::Value, dir: &Path, include_lib: bool) -> Vec<String> {
    let mut out = Vec::new();
    for target in package["targets"].as_array().into_iter().flatten() {
        let is_lib = target["kind"].as_array().into_iter().flatten().any(|k| {
            matches!(
                k.as_str(),
                Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
            )
        });
        let Some(src_path) = target["src_path"].as_str() else {
            continue;
        };
        if is_lib && !include_lib {
            continue;
        }
        let rel = Path::new(src_path).canonicalize().ok().and_then(|p| {
            p.strip_prefix(dir)
                .ok()
                .map(|r| r.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
        });
        out.extend(rel);
    }
    out
}

/// Identifiers in `source`. Lexed, so comments and string literals do not count; if the text does
/// not lex (a template file, syntax this lexer predates) every identifier-looking word counts
/// instead -- over-approximating, which is the safe direction here.
pub fn identifiers_in(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    match source.parse::<proc_macro2::TokenStream>() {
        Ok(tokens) => collect(tokens, &mut out),
        Err(_) => scan_words(source, &mut out),
    }
    out
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield", "gen",
];

fn collect(tokens: proc_macro2::TokenStream, out: &mut BTreeSet<String>) {
    for tree in tokens {
        match tree {
            proc_macro2::TokenTree::Ident(i) => {
                let text = i.to_string();
                // a raw identifier (`r#type`) is a real name; a bare keyword is syntax
                if text.starts_with("r#") || !KEYWORDS.contains(&text.as_str()) {
                    out.insert(text.trim_start_matches("r#").to_string());
                }
            }
            proc_macro2::TokenTree::Group(g) => collect(g.stream(), out),
            proc_macro2::TokenTree::Literal(l) => literal_idents(&l.to_string(), out),
            _ => {}
        }
    }
}

/// What a string literal can name. `{name}` / `{name:?}` is a use of `name` (inline format arguments,
/// thiserror's `#[error("...")]`): any word right after a `{` counts, and `{{` escapes only
/// over-approximate. A literal that is itself a path (`"default_port"`, `"codec::parse"`) names what
/// it points at: `#[serde(default = "...")]`, `with = "..."` and friends call it from generated code.
/// Ordinary prose (spaces, punctuation) names nothing.
fn literal_idents(literal: &str, out: &mut BTreeSet<String>) {
    if let Some(inner) = literal
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        let segments: Vec<&str> = inner.split("::").collect();
        let is_path = !inner.is_empty()
            && segments.iter().all(|seg| {
                seg.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && seg.chars().all(|c| c.is_alphanumeric() || c == '_')
            });
        if is_path {
            out.extend(segments.iter().map(|s| s.to_string()));
        }
    }
    for part in literal.split('{').skip(1) {
        let word: String = part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if word.chars().next().is_some_and(|f| !f.is_ascii_digit()) {
            out.insert(word);
        }
    }
}

fn scan_words(source: &str, out: &mut BTreeSet<String>) {
    let mut word = String::new();
    for c in source.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            // Keywords appear in every header and body; counting them would make unrelated items
            // reachable.
            if word.chars().next().is_some_and(|f| !f.is_ascii_digit())
                && !KEYWORDS.contains(&word.as_str())
            {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, contents) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        tmp
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn module_files_are_resolved_the_way_rustc_does() {
        let tmp = tree(&[
            (
                "src/lib.rs",
                "mod a;\nmod b { mod c; }\n#[path = \"x/y.rs\"]\nmod z;\nmod dir;\nmod absent;\n",
            ),
            ("src/a.rs", "mod aa;\n"),
            ("src/a/aa.rs", ""),
            ("src/b/c.rs", ""),
            ("src/x/y.rs", ""),
            ("src/dir/mod.rs", "mod leaf;\n"),
            ("src/dir/leaf.rs", ""),
            // not declared by the lib: belongs to a bin
            ("src/main.rs", "mod util;\nfn main() {}\n"),
            ("src/util.rs", ""),
        ]);
        assert_eq!(
            lib_module_files(tmp.path()).unwrap(),
            set(&[
                "src/lib.rs",
                "src/a.rs",
                "src/a/aa.rs",
                "src/b/c.rs",
                "src/x/y.rs",
                "src/dir/mod.rs",
                "src/dir/leaf.rs",
            ])
        );
    }

    #[test]
    fn a_file_the_lib_does_not_declare_is_foreign_and_so_are_tests_examples_and_benches() {
        let tmp = tree(&[
            (
                "Cargo.toml",
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "mod inner;\npub fn lib_only() {}\n"),
            ("src/inner.rs", "pub fn inside_lib() {}\n"),
            ("src/main.rs", "mod util;\nfn main() { from_main(); }\n"),
            ("src/util.rs", "fn from_util() {}\n"),
            ("src/bin/tool.rs", "fn main() { from_bin(); }\n"),
            ("tests/t.rs", "fn t() { from_test(); }\n"),
            ("examples/e.rs", "fn main() { from_example(); }\n"),
            ("benches/b.rs", "fn b() { from_bench(); }\n"),
        ]);
        let lib = lib_module_files(tmp.path()).unwrap();
        let foreign = foreign_identifiers(tmp.path(), &lib).unwrap();
        for name in [
            "from_main",
            "from_util",
            "from_bin",
            "from_test",
            "from_example",
            "from_bench",
        ] {
            assert!(
                foreign.contains(name),
                "{name} should be foreign: {foreign:?}"
            );
        }
        // the lib's own files are not foreign: a lib item mentioned only inside the lib is not rescued
        assert!(!foreign.contains("lib_only") && !foreign.contains("inside_lib"));
    }

    #[test]
    fn identifiers_come_from_code_not_comments_or_strings() {
        let ids = identifiers_in(
            "// in_comment\nfn real() { let s = \"has spaces here\"; call!(in_macro); r#type(); }\n",
        );
        assert!(ids.contains("real") && ids.contains("in_macro") && ids.contains("type"));
        assert!(!ids.contains("in_comment") && !ids.contains("has"));
    }

    #[test]
    fn a_name_captured_inside_a_format_string_counts() {
        // `format!("{NAME}")` uses NAME exactly as `format!("{}", NAME)` does, and so does a
        // thiserror `#[error("... {NAME}")]`. A self-scan of CodeRipper reported two live constants
        // as dead because the lexer treats the string as opaque.
        let ids =
            identifiers_in("fn f() { println!(\"{captured} and {spec:?} and {{escaped}}\"); }");
        assert!(ids.contains("captured") && ids.contains("spec"), "{ids:?}");
        // an ordinary string with no braces still contributes nothing
        assert!(!identifiers_in("fn f() { let s = \"plain words here\"; }").contains("plain"));
    }

    #[test]
    fn text_that_does_not_lex_still_yields_every_word() {
        // A template file under src/: not Rust, but it must neither fail the check nor be ignored.
        let ids = identifiers_in("fn {{name}}() { \"unterminated\n some_word");
        assert!(ids.contains("some_word") && ids.contains("name"), "{ids:?}");
    }

    #[test]
    fn keywords_are_not_identifiers() {
        // `impl` and `for` are in every trait impl header; counting them made every impl live.
        let ids =
            identifiers_in("impl<T> Drop for Guard<T> { fn drop(&mut self) { for i in 0..3 {} } }");
        for kw in ["impl", "for", "fn", "mut", "self"] {
            assert!(!ids.contains(kw), "{kw} must not count: {ids:?}");
        }
        assert!(ids.contains("Guard") && ids.contains("Drop") && ids.contains("drop"));
    }

    #[test]
    fn a_path_shaped_string_literal_names_what_it_points_at() {
        // `#[serde(default = "default_port")]`, `#[serde(with = "module::fn")]`: the derive calls the
        // named function, and nothing else in the source mentions it.
        let ids = identifiers_in(
            "#[serde(default = \"default_port\", with = \"codec::parse\")] pub port: u16,",
        );
        assert!(
            ids.contains("default_port") && ids.contains("codec") && ids.contains("parse"),
            "{ids:?}"
        );
        // ordinary prose in a string is not a path
        assert!(!identifiers_in("fn f() { let s = \"two words\"; }").contains("two"));
    }

    #[test]
    fn non_utf8_files_do_not_fail_the_scan() {
        let tmp = tree(&[
            (
                "Cargo.toml",
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub fn lib_only() {}\n"),
        ]);
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        std::fs::write(
            tmp.path().join("tests/t.rs"),
            b"fn t() { from_test(); } // caf\xe9\n",
        )
        .unwrap();
        let lib = lib_module_files(tmp.path()).unwrap();
        let found = foreign_identifiers(tmp.path(), &lib).unwrap();
        assert!(found.contains("from_test"), "{found:?}");
    }

    #[test]
    fn targets_with_a_custom_path_are_foreign_with_their_own_module_tree() {
        let tmp = tree(&[
            (
                "Cargo.toml",
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"tool\"\npath = \"tools/tool.rs\"\n\n[[test]]\nname = \"it\"\npath = \"it/main.rs\"\n",
            ),
            ("src/lib.rs", "pub fn lib_only() {}\n"),
            ("tools/tool.rs", "mod helpers;\nfn main() { from_tool(); }\n"),
            ("tools/helpers.rs", "pub fn go() { from_helpers(); }\n"),
            ("it/main.rs", "fn t() { from_it(); }\n"),
        ]);
        let lib = lib_module_files(tmp.path()).unwrap();
        let found = foreign_identifiers(tmp.path(), &lib).unwrap();
        for name in ["from_tool", "from_helpers", "from_it"] {
            assert!(found.contains(name), "{name} should be foreign: {found:?}");
        }
        assert!(!found.contains("lib_only"));
    }

    #[test]
    fn patch_and_replace_tables_yield_their_paths_and_nothing_else() {
        let manifest: toml::Table = r#"
            [patch.crates-io]
            a = { path = "crates/a" }
            b = { git = "https://example.invalid/b" }
            [patch."https://example.invalid/index"]
            c = { path = "../c", package = "real-c" }
            [replace]
            "d:0.1.0" = { path = "crates/d" }
            [dependencies]
            e = { path = "crates/e" }
        "#
        .parse()
        .unwrap();
        let mut paths = patched_paths(&manifest);
        paths.sort();
        assert_eq!(paths, vec!["../c", "crates/a", "crates/d"]);
    }

    #[test]
    fn an_unparseable_lib_file_is_an_error_not_a_silent_skip() {
        let tmp = tree(&[("src/lib.rs", "this is not rust {{{")]);
        assert!(lib_module_files(tmp.path()).is_err());
    }
}
