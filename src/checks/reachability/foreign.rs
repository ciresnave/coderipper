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
    let mut walk = Walk {
        root,
        seen: BTreeSet::new(),
    };
    walk.file("src/lib.rs", true)?;
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
/// `examples/` and `benches/`, plus every file under `src/` that `lib_files` does not contain
/// (`src/main.rs`, `src/bin/**`, and modules only a bin declares).
pub fn foreign_identifiers(
    root: &Path,
    lib_files: &BTreeSet<String>,
) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for top in ["src", "tests", "examples", "benches"] {
        let dir = root.join(top);
        if !dir.is_dir() {
            continue;
        }
        for path in crate::worktree::walk_rs_files(&dir)? {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            if lib_files.contains(&rel) {
                continue;
            }
            out.extend(identifiers_in(&std::fs::read_to_string(&path)?));
        }
    }
    Ok(out)
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

fn collect(tokens: proc_macro2::TokenStream, out: &mut BTreeSet<String>) {
    for tree in tokens {
        match tree {
            proc_macro2::TokenTree::Ident(i) => {
                out.insert(i.to_string().trim_start_matches("r#").to_string());
            }
            proc_macro2::TokenTree::Group(g) => collect(g.stream(), out),
            proc_macro2::TokenTree::Literal(l) => format_captures(&l.to_string(), out),
            _ => {}
        }
    }
}

/// `{name}` / `{name:?}` inside a string literal is a use of `name` (inline format arguments, and
/// thiserror's `#[error("...")]`). Any word right after a `{` counts; a literal with no `{` adds
/// nothing, and `{{` escapes only over-approximate.
fn format_captures(literal: &str, out: &mut BTreeSet<String>) {
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
            if word.chars().next().is_some_and(|f| !f.is_ascii_digit()) {
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
            "// in_comment\nfn real() { let s = \"in_string\"; call!(in_macro); r#type(); }\n",
        );
        assert!(ids.contains("real") && ids.contains("in_macro") && ids.contains("type"));
        assert!(!ids.contains("in_comment") && !ids.contains("in_string"));
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
    fn an_unparseable_lib_file_is_an_error_not_a_silent_skip() {
        let tmp = tree(&[("src/lib.rs", "this is not rust {{{")]);
        assert!(lib_module_files(tmp.path()).is_err());
    }
}
