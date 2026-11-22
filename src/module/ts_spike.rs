// SPDX-License-Identifier: MIT OR Apache-2.0
//! Spike (ROADMAP items 8 and 12): parse TypeScript with `tree-sitter` and `tree-sitter-typescript`. Not wired into any module yet.

use tree_sitter::{Language, Parser, Tree};

/// Parses `source` as TypeScript; `None` when the grammar cannot be loaded or parsing is cancelled.
pub(crate) fn parse_typescript(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    let language: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typescript_function_parses_without_error() {
        let tree = parse_typescript(
            "export function add(a: number, b: number): number { return a + b; }\n",
        )
        .expect("parses");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(!root.has_error());
    }

    #[test]
    fn a_broken_file_is_reported_as_having_an_error_not_as_clean() {
        let tree = parse_typescript("function (:\n").expect("a tree is still returned");
        assert!(tree.root_node().has_error());
    }
}
