//! Line classification: comment, code, or blank.
//!
//! Deliberately line-leading only. `foo(); // why` is code with an aside, not
//! documentation, and counting it as documentation is how a doc ratio starts
//! lying to you.

use std::path::Path;

/// The comment syntax a file's extension implies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Syntax {
    /// `//`, `/* */`, and JSDoc continuation lines.
    CFamily,
    /// `#`.
    Hash,
    /// `#` plus triple-quoted docstrings.
    Python,
    /// `<!-- -->`.
    Markup,
    /// Both C-family and markup, for single-file component formats.
    Mixed,
}

/// Returns `None` for extensions with no comment syntax worth counting —
/// data files, lockfiles, prose. Those are skipped rather than guessed at.
pub fn syntax_for(path: &Path) -> Option<Syntax> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let syntax = match extension.as_str() {
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "go" | "c" | "h" | "cc" | "cpp"
        | "hpp" | "cs" | "java" | "kt" | "kts" | "swift" | "scala" | "php" | "dart" | "zig"
        | "proto" | "css" | "scss" | "less" => Syntax::CFamily,
        "py" | "pyi" => Syntax::Python,
        "sh" | "bash" | "zsh" | "fish" | "rb" | "pl" | "yaml" | "yml" | "toml" | "tf" | "nix" => {
            Syntax::Hash
        }
        "html" | "htm" | "xml" | "svg" => Syntax::Markup,
        "vue" | "svelte" | "astro" => Syntax::Mixed,
        _ => return None,
    };
    Some(syntax)
}

/// What a single line contributes to the count.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Comment,
    Code,
    Blank,
}

#[derive(Clone, Copy)]
enum Block {
    CStyle,
    Markup,
    PyQuote(&'static str),
}

/// Walks a file top to bottom, carrying block-comment state between lines.
pub struct Scanner {
    syntax: Syntax,
    block: Option<Block>,
}

impl Scanner {
    /// Start at the top of a file written in `syntax`.
    pub fn new(syntax: Syntax) -> Self {
        Self {
            syntax,
            block: None,
        }
    }

    /// Classify one line and carry any block state to the next.
    pub fn classify(&mut self, raw: &str) -> LineKind {
        let line = raw.trim();

        // A blank line inside a doc block is part of the doc block.
        if let Some(block) = self.block {
            let closed = match block {
                Block::CStyle => line.contains("*/"),
                Block::Markup => line.contains("-->"),
                Block::PyQuote(delimiter) => line.contains(delimiter),
            };
            if closed {
                self.block = None;
            }
            return LineKind::Comment;
        }

        if line.is_empty() {
            return LineKind::Blank;
        }

        let c_like = matches!(self.syntax, Syntax::CFamily | Syntax::Mixed);
        let markup = matches!(self.syntax, Syntax::Markup | Syntax::Mixed);
        let hash = matches!(self.syntax, Syntax::Hash | Syntax::Python);

        if c_like {
            if let Some(rest) = line.strip_prefix("/*") {
                if !rest.contains("*/") {
                    self.block = Some(Block::CStyle);
                }
                return LineKind::Comment;
            }
            // A leading `*` is a JSDoc continuation line that lost its state,
            // which happens constantly when reading a -U0 diff.
            if line.starts_with("//") || line.starts_with('*') {
                return LineKind::Comment;
            }
        }

        if markup {
            if let Some(rest) = line.strip_prefix("<!--") {
                if !rest.contains("-->") {
                    self.block = Some(Block::Markup);
                }
                return LineKind::Comment;
            }
        }

        if hash && line.starts_with('#') {
            return LineKind::Comment;
        }

        if self.syntax == Syntax::Python {
            for delimiter in [r#"""""#, "'''"] {
                if let Some(rest) = line.strip_prefix(delimiter) {
                    if !rest.contains(delimiter) {
                        self.block = Some(Block::PyQuote(delimiter));
                    }
                    return LineKind::Comment;
                }
            }
        }

        LineKind::Code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn classify_all(syntax: Syntax, source: &str) -> Vec<LineKind> {
        let mut scanner = Scanner::new(syntax);
        source.lines().map(|line| scanner.classify(line)).collect()
    }

    #[test]
    fn trailing_comments_are_code() {
        assert_eq!(
            classify_all(Syntax::CFamily, "let x = 1; // why"),
            vec![LineKind::Code]
        );
    }

    #[test]
    fn block_comments_stay_open_across_lines() {
        let kinds = classify_all(Syntax::CFamily, "/**\n * doc\n */\ncode();");
        assert_eq!(
            kinds,
            vec![
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Code
            ]
        );
    }

    #[test]
    fn a_blank_line_inside_a_block_belongs_to_the_block() {
        let kinds = classify_all(Syntax::CFamily, "/*\n\n*/\n");
        assert_eq!(
            kinds,
            vec![LineKind::Comment, LineKind::Comment, LineKind::Comment]
        );
    }

    #[test]
    fn single_line_block_comments_do_not_open_a_block() {
        let kinds = classify_all(Syntax::CFamily, "/* one */\ncode();");
        assert_eq!(kinds, vec![LineKind::Comment, LineKind::Code]);
    }

    #[test]
    fn python_docstrings_count_as_comments() {
        let kinds = classify_all(Syntax::Python, "\"\"\"\ndoc\n\"\"\"\ncode()");
        assert_eq!(
            kinds,
            vec![
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Code
            ]
        );
    }

    #[test]
    fn mixed_files_accept_both_syntaxes() {
        let kinds = classify_all(Syntax::Mixed, "<!-- tpl -->\n// script\ncode()");
        assert_eq!(
            kinds,
            vec![LineKind::Comment, LineKind::Comment, LineKind::Code]
        );
    }

    #[test]
    fn markup_blocks_close_on_their_own_delimiter() {
        let kinds = classify_all(Syntax::Markup, "<!--\nstill comment\n-->\n<div/>");
        assert_eq!(
            kinds,
            vec![
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Comment,
                LineKind::Code
            ]
        );
    }

    #[test]
    fn unknown_extensions_are_skipped_not_guessed() {
        assert_eq!(syntax_for(&PathBuf::from("a.json")), None);
        assert_eq!(syntax_for(&PathBuf::from("README.md")), None);
        assert_eq!(syntax_for(&PathBuf::from("a.rs")), Some(Syntax::CFamily));
        assert_eq!(syntax_for(&PathBuf::from("a.vue")), Some(Syntax::Mixed));
    }
}
