//! Moving an open thread's anchor to the next version, on rendered text
//! with whitespace ignored, never on markdown source. A quote is attached
//! with its context, or alone when it is unique under the same heading path
//! in both versions; anything else is orphaned rather than guessed.
//! docs/md-review-page.md#re-anchoring-after-a-round.

use super::render::Block;
use super::store::{Anchor, Point, Span};

const CONTEXT_CHARS: usize = 40;

/// Re-attach `anchor`, made on `old`, to `new`, which is version
/// `version`. `None` means the thread is orphaned.
pub fn reanchor(anchor: &Anchor, old: &[Block], new: &[Block], version: u32) -> Option<Anchor> {
    let quote = squeeze(&anchor.quote);
    if quote.is_empty() {
        return None;
    }
    let after_text = Text::new(new);
    let prefix = squeeze(&anchor.prefix);
    let suffix = squeeze(&anchor.suffix);

    let in_context: Vec<usize> = after_text
        .find_all(&quote)
        .into_iter()
        .filter(|&at| {
            after_text.key[..at].ends_with(&prefix)
                && after_text.key[at + quote.len()..].starts_with(&suffix)
        })
        .collect();
    let at = match in_context.as_slice() {
        [only] => *only,
        _ => {
            let before_text = Text::new(old);
            let [only] = after_text.under(&quote, &anchor.headings)[..] else {
                return None;
            };
            if before_text.under(&quote, &anchor.headings).len() != 1 {
                return None;
            }
            only
        }
    };
    Some(after_text.anchor_at(at, &anchor.quote, quote.len(), version))
}

/// A version's text with whitespace removed, and where each block starts
/// in it.
struct Text<'a> {
    blocks: &'a [Block],
    key: String,
    starts: Vec<usize>,
}

impl<'a> Text<'a> {
    fn new(blocks: &'a [Block]) -> Self {
        let mut key = String::new();
        let mut starts = Vec::new();
        for block in blocks {
            starts.push(key.len());
            key.extend(block.text.chars().filter(|c| !c.is_whitespace()));
        }
        Self {
            blocks,
            key,
            starts,
        }
    }

    /// Every start of `needle`, overlapping ones included.
    fn find_all(&self, needle: &str) -> Vec<usize> {
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(offset) = self.key[from..].find(needle) {
            let at = from + offset;
            found.push(at);
            from = at + self.key[at..].chars().next().map_or(1, char::len_utf8);
        }
        found
    }

    /// Starts of `needle` in blocks under exactly `headings`.
    fn under(&self, needle: &str, headings: &[String]) -> Vec<usize> {
        self.find_all(needle)
            .into_iter()
            .filter(|&at| self.blocks[self.owner(at)].headings == headings)
            .collect()
    }

    fn anchor_at(&self, at: usize, quote: &str, len: usize, version: u32) -> Anchor {
        let first = self.owner(at);
        let last = self.owner(at + len - 1);
        let prefix: String = {
            let mut chars: Vec<char> = self.key[..at].chars().rev().take(CONTEXT_CHARS).collect();
            chars.reverse();
            chars.into_iter().collect()
        };
        let suffix: String = self.key[at + len..].chars().take(CONTEXT_CHARS).collect();
        Anchor {
            quote: quote.to_string(),
            prefix,
            suffix,
            blocks: (first as u32..=last as u32).collect(),
            lines: [self.blocks[first].lines[0], self.blocks[last].lines[1]],
            headings: self.blocks[first].headings.clone(),
            version,
            span: Some(Span {
                start: self.point(first, at),
                end: self.point(last, at + len),
            }),
        }
    }

    /// The block byte `at` of the key came from: the last to start at or
    /// before it, since an empty block starts where the next one does.
    fn owner(&self, at: usize) -> usize {
        self.starts.partition_point(|&start| start <= at) - 1
    }

    /// The point at byte `at` of the key, in `block`.
    fn point(&self, block: usize, at: usize) -> Point {
        Point {
            block: block as u32,
            at: self.key[self.starts[block]..at].chars().count() as u32,
        }
    }
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_review::render::render;

    fn anchor(quote: &str, prefix: &str, suffix: &str, headings: &[&str]) -> Anchor {
        Anchor {
            quote: quote.to_string(),
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            blocks: vec![],
            lines: [0, 0],
            headings: headings.iter().map(|h| h.to_string()).collect(),
            version: 1,
            span: None,
        }
    }

    fn moved(anchor: &Anchor, old: &str, new: &str) -> Option<Anchor> {
        let old = render(old.as_bytes()).unwrap();
        let new = render(new.as_bytes()).unwrap();
        reanchor(anchor, &old, &new, 2)
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn a_quote_with_its_context_follows_the_text_across_markup_and_whitespace() {
        let thread = anchor("keeps the  quoted text", "A comment ", ", and more", &[]);
        let old = "# Top\n\nA comment keeps the quoted text, and more.\n";
        let new = "# Top\n\nNew intro.\n\nA comment keeps **the quoted**\ntext, and more.\n";
        let found = moved(&thread, old, new).expect("attached");
        assert_eq!(found.blocks, [2]);
        assert_eq!(found.lines, [5, 6]);
        assert_eq!(found.headings, ["Top"]);
        assert_eq!(found.version, 2);
        assert_eq!(found.quote, "keeps the  quoted text");
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn context_picks_one_of_several_copies() {
        let thread = anchor("same", "second ", " here", &["S"]);
        let text = "# S\n\nfirst same here\n\nsecond same here\n";
        let found = moved(&thread, text, text).expect("attached");
        assert_eq!(found.blocks, [2]);
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn the_suffix_also_decides_between_copies() {
        let thread = anchor("same", "x ", " two", &["S"]);
        let text = "# S\n\nx same one\n\nx same two\n";
        let found = moved(&thread, text, text).expect("attached");
        assert_eq!(found.blocks, [2]);
    }

    #[test]
    fn overlapping_occurrences_make_a_quote_ambiguous() {
        let thread = anchor("abab", "", "", &[]);
        assert_eq!(moved(&thread, "ababab\n", "ababab\n"), None);
    }

    #[test]
    fn a_blank_quote_is_orphaned() {
        let thread = anchor(" \n ", "", "", &[]);
        assert_eq!(moved(&thread, "text\n", "text\n"), None);
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn a_quote_without_its_context_attaches_when_unique_under_its_heading_in_both() {
        let thread = anchor("the phrase", "old context ", " old tail", &["A"]);
        let old = "# A\n\nold context the phrase old tail\n\n# B\n\nthe phrase\n";
        let new = "# A\n\nrewritten around the phrase now\n\n# B\n\nthe phrase\n";
        let found = moved(&thread, old, new).expect("attached");
        assert_eq!(found.headings, ["A"]);
        assert_eq!(found.blocks, [1]);
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn a_quote_without_context_that_is_not_unique_under_its_heading_is_orphaned() {
        let thread = anchor("the phrase", "old context ", "", &["A"]);
        let old = "# A\n\nold context the phrase\n";
        let new = "# A\n\nthe phrase once\n\nthe phrase twice\n";
        assert_eq!(moved(&thread, old, new), None);
    }

    // Case: docs/cases/gist-md-review.md#reanchor
    #[test]
    fn a_quote_that_is_gone_is_orphaned() {
        let thread = anchor("vanished words", "", "", &[]);
        assert_eq!(moved(&thread, "vanished words\n", "other words\n"), None);
    }

    // Case: docs/cases/gist-md-review.md#reanchor-copied-phrase
    #[test]
    fn a_phrase_copied_then_deleted_at_its_origin_is_orphaned_not_moved_to_the_copy() {
        let thread = anchor("copied phrase", "origin: ", "", &["S"]);
        let old = "# S\n\norigin: copied phrase\n\ncopy: copied phrase\n";
        let new = "# S\n\norigin: rewritten\n\ncopy: copied phrase\n";
        assert_eq!(moved(&thread, old, new), None);
    }

    #[test]
    fn a_quote_across_blocks_spans_them() {
        let thread = anchor("end of one. Start of two", "", "", &[]);
        let text = "Lead.\n\nThe end of one.\n\nStart of two here.\n";
        let found = moved(&thread, text, text).expect("attached");
        assert_eq!(found.blocks, [1, 2]);
        assert_eq!(found.lines, [3, 5]);
        let span = found.span.expect("placed");
        assert_eq!((span.start.block, span.start.at), (1, 3));
        assert_eq!((span.end.block, span.end.at), (2, 10));
    }
}
