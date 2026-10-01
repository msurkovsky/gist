//! Markdown to review blocks: each top-level block's HTML for the page, its
//! plain text for re-anchoring, its source for the block diff, its source
//! lines and its heading path. Rendered markdown is untrusted: raw HTML is
//! dropped, dangerous links are emptied, and images that would load from
//! outside `serve` lose their source. docs/design/md-review-hld.md#anchor.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{format_html, parse_document, Arena, Options};
use serde::Serialize;

/// One top-level block of the rendered document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    /// First and last source line, 1-based.
    pub lines: [u32; 2],
    /// Titles of the headings this block sits under, outermost first; a
    /// heading's own path ends with itself.
    pub headings: Vec<String>,
    /// Text as the reader sees it, which is what anchors quote.
    pub text: String,
    /// The block's source lines, which the block diff compares.
    #[serde(skip)]
    pub source: String,
    /// The block's HTML, wrapped with its index and source lines.
    pub html: String,
}

/// What a block is, so the page knows which ones select as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Code,
    Mermaid,
    List,
    Quote,
    Table,
    Rule,
    /// Raw HTML, which is not rendered.
    Html,
}

/// Render `markdown` into blocks in document order.
pub fn render(markdown: &[u8]) -> Result<Vec<Block>, String> {
    let text = std::str::from_utf8(markdown).map_err(|_| "the file is not UTF-8".to_string())?;
    let source_lines: Vec<&str> = text.lines().collect();
    let options = options();
    let arena = Arena::new();
    let root = parse_document(&arena, text, &options);
    for node in root.descendants() {
        if let NodeValue::Image(link) = &mut node.data_mut().value {
            if !is_local(&link.url) {
                link.url.clear();
            }
        }
    }

    let mut blocks = Vec::new();
    let mut path: Vec<(u8, String)> = Vec::new();
    for node in root.children() {
        let (kind, start, end) = {
            let data = node.data();
            let kind = match &data.value {
                NodeValue::Heading(heading) => {
                    let title = plain_text(node).trim().to_string();
                    while path
                        .last()
                        .is_some_and(|(level, _)| *level >= heading.level)
                    {
                        path.pop();
                    }
                    path.push((heading.level, title));
                    BlockKind::Heading
                }
                NodeValue::Paragraph => BlockKind::Paragraph,
                NodeValue::CodeBlock(code)
                    if code.info.split_whitespace().next() == Some("mermaid") =>
                {
                    BlockKind::Mermaid
                }
                NodeValue::CodeBlock(_) => BlockKind::Code,
                NodeValue::List(_) => BlockKind::List,
                NodeValue::BlockQuote => BlockKind::Quote,
                NodeValue::Table(_) => BlockKind::Table,
                NodeValue::ThematicBreak => BlockKind::Rule,
                NodeValue::HtmlBlock(_) => BlockKind::Html,
                _ => continue,
            };
            (kind, data.sourcepos.start.line, data.sourcepos.end.line)
        };
        let end = end.max(start);
        let mut inner = String::new();
        format_html(node, &options, &mut inner)
            .map_err(|err| format!("could not render line {start}: {err}"))?;
        let index = blocks.len();
        blocks.push(Block {
            kind,
            lines: [start as u32, end as u32],
            headings: path.iter().map(|(_, title)| title.clone()).collect(),
            text: plain_text(node),
            source: source_lines
                .get(start - 1..end.min(source_lines.len()))
                .map(|lines| lines.join("\n"))
                .unwrap_or_default(),
            html: format!(
                "<div class=\"block\" data-block=\"{index}\" data-lines=\"{start}-{end}\">{inner}</div>"
            ),
        });
    }
    Ok(blocks)
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.r#unsafe = false;
    options
}

/// A relative URL, served by `serve` if at all; anything with a scheme or
/// a host would make the page fetch from elsewhere.
fn is_local(url: &str) -> bool {
    !url.starts_with("//") && !url.contains(':')
}

/// The text a reader sees in `node`. Whitespace between blocks is a
/// newline; re-anchoring ignores whitespace, so its exact shape is cosmetic.
fn plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out.trim().to_string()
}

fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
    match &node.data().value {
        NodeValue::Text(text) => out.push_str(text),
        NodeValue::Code(code) => out.push_str(&code.literal),
        NodeValue::CodeBlock(code) => out.push_str(&code.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push('\n'),
        NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) => {}
        _ => {
            for child in node.children() {
                let block = matches!(
                    child.data().value,
                    NodeValue::Paragraph
                        | NodeValue::Heading(_)
                        | NodeValue::List(_)
                        | NodeValue::Item(_)
                        | NodeValue::TaskItem(_)
                        | NodeValue::BlockQuote
                        | NodeValue::CodeBlock(_)
                        | NodeValue::Table(_)
                        | NodeValue::TableRow(_)
                        | NodeValue::TableCell
                );
                if block && !out.is_empty() && !out.ends_with(char::is_whitespace) {
                    out.push('\n');
                }
                collect_text(child, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(blocks: &[Block]) -> Vec<BlockKind> {
        blocks.iter().map(|b| b.kind).collect()
    }

    #[test]
    fn top_level_blocks_carry_their_lines_kind_and_heading_path() {
        let md = "# Loop\n\nIntro text.\n\n## Anchors\n\n- one\n- two\n\n```mermaid\ngraph TD\n```\n\n# Other\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let blocks = render(md.as_bytes()).unwrap();
        assert_eq!(
            kinds(&blocks),
            [
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::Heading,
                BlockKind::List,
                BlockKind::Mermaid,
                BlockKind::Heading,
                BlockKind::Table,
            ]
        );
        assert_eq!(blocks[3].lines, [7, 8]);
        assert_eq!(blocks[4].lines, [10, 12]);
        assert_eq!(blocks[1].headings, ["Loop"]);
        assert_eq!(blocks[3].headings, ["Loop", "Anchors"]);
        assert_eq!(blocks[6].headings, ["Other"]);
        assert_eq!(blocks[4].text, "graph TD");
        assert_eq!(blocks[3].source, "- one\n- two");
        assert!(blocks[3]
            .html
            .starts_with("<div class=\"block\" data-block=\"3\" data-lines=\"7-8\">"));
    }

    #[test]
    fn plain_text_drops_markup() {
        let blocks = render(b"A **bold** and `code` [link](x.md) word.\n").unwrap();
        assert_eq!(blocks[0].text, "A bold and code link word.");
    }

    #[test]
    fn table_cells_and_list_items_are_kept_apart() {
        let blocks = render(b"- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n").unwrap();
        assert_eq!(blocks[0].text, "one\ntwo");
        assert_eq!(blocks[1].text, "a\nb\n1\n2");
    }

    #[test]
    fn an_empty_file_has_no_blocks() {
        assert!(render(b"").unwrap().is_empty());
    }

    #[test]
    fn a_file_that_is_not_utf8_is_refused() {
        assert!(render(&[0xff, 0xfe]).unwrap_err().contains("UTF-8"));
    }

    // Case: docs/cases/gist-md-review.md#page-render-safety
    #[test]
    fn raw_html_is_not_passed_through() {
        let blocks =
            render(b"<script>alert(1)</script>\n\nText <b onclick=\"x()\">bold</b>.\n").unwrap();
        let html: String = blocks.iter().map(|b| b.html.as_str()).collect();
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert_eq!(blocks[0].kind, BlockKind::Html);
    }

    // Case: docs/cases/gist-md-review.md#page-render-safety
    #[test]
    fn javascript_links_are_stripped() {
        let blocks = render(b"[click](javascript:alert(1))\n").unwrap();
        assert!(
            !blocks[0].html.contains("javascript:"),
            "{}",
            blocks[0].html
        );
    }

    // Case: docs/cases/gist-md-review.md#page-render-safety
    #[test]
    fn images_from_outside_lose_their_source() {
        let blocks = render(
            b"![far](https://example.com/a.png) ![near](//example.com/b.png) ![here](img/c.png)\n",
        )
        .unwrap();
        let html = &blocks[0].html;
        assert!(!html.contains("example.com"), "{html}");
        assert!(html.contains("src=\"img/c.png\""), "{html}");
    }
}
