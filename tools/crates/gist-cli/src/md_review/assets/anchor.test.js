// Case: docs/cases/gist-md-review.md#page-anchor
import { test } from "node:test";
import assert from "node:assert/strict";
import { anchorFor, chars, slice, visible } from "./anchor.js";

// Blocks as `serve` sends them for:
//   # Loop / Intro text. / ## Anchors / the quoted text here / mermaid / table / ---
const blocks = [
  { kind: "heading", text: "Loop", lines: [1, 1], headings: ["Loop"] },
  { kind: "paragraph", text: "Intro text.", lines: [3, 3], headings: ["Loop"] },
  { kind: "heading", text: "Anchors", lines: [5, 5], headings: ["Loop", "Anchors"] },
  { kind: "paragraph", text: "the quoted\ntext here", lines: [7, 8], headings: ["Loop", "Anchors"] },
  { kind: "mermaid", text: "graph TD\nA-->B", lines: [10, 13], headings: ["Loop", "Anchors"] },
  { kind: "table", text: "a\nb\n1\n2", lines: [15, 17], headings: ["Loop", "Anchors"] },
  { kind: "rule", text: "", lines: [19, 19], headings: ["Loop", "Anchors"] },
];

test("whitespace is not counted, as the server does not count it", () => {
  assert.equal(visible(" a\u00a0b\n c\u3000"), 3);
  // U+FEFF is not White_Space in Rust, though JavaScript's \s matches it.
  assert.deepEqual(chars("a\ufeffb"), ["a", "\ufeff", "b"]);
  assert.equal(visible("𝔸b"), 2);
});

test("a selection inside a block quotes it with its context, lines and headings", () => {
  // "quoted text" is visible characters 3 to 13 of "thequotedtexthere".
  const anchor = anchorFor(blocks, { block: 3, at: 3 }, { block: 3, at: 13 }, 2);
  assert.deepEqual(anchor, {
    quote: "quoted\ntext",
    prefix: "LoopIntrotext.Anchorsthe",
    suffix: "heregraphTDA-->Bab12",
    blocks: [3],
    lines: [7, 8],
    headings: ["Loop", "Anchors"],
    version: 2,
    span: { start: { block: 3, at: 3 }, end: { block: 3, at: 13 } },
  });
});

test("context stops at forty characters", () => {
  const long = [{ kind: "paragraph", text: "x".repeat(50) + "Q" + "y".repeat(50), lines: [1, 1], headings: [] }];
  const anchor = anchorFor(long, { block: 0, at: 50 }, { block: 0, at: 51 }, 1);
  assert.equal(anchor.prefix, "x".repeat(40));
  assert.equal(anchor.suffix, "y".repeat(40));
});

test("a selection made backwards is the same selection", () => {
  assert.deepEqual(
    anchorFor(blocks, { block: 3, at: 13 }, { block: 1, at: 2 }, 1),
    anchorFor(blocks, { block: 1, at: 2 }, { block: 3, at: 13 }, 1),
  );
});

test("a selection across blocks spans their whole lines and starts under the first one's headings", () => {
  const anchor = anchorFor(blocks, { block: 1, at: 5 }, { block: 3, at: 3 }, 1);
  assert.equal(anchor.quote, "text.\nAnchors\nthe");
  assert.deepEqual(anchor.blocks, [1, 2, 3]);
  assert.deepEqual(anchor.lines, [3, 8]);
  assert.deepEqual(anchor.headings, ["Loop"]);
});

test("a selection over a diagram or a table takes the whole block", () => {
  const into = anchorFor(blocks, { block: 3, at: 13 }, { block: 4, at: 2 }, 1);
  assert.equal(into.quote, "here\ngraph TD\nA-->B");
  assert.deepEqual(into.lines, [7, 13]);
  const inside = anchorFor(blocks, { block: 5, at: 1 }, { block: 5, at: 2 }, 1);
  assert.equal(inside.quote, "a\nb\n1\n2");
  assert.deepEqual(inside.blocks, [5]);
});

test("a point that is not known takes the block to its edge", () => {
  assert.equal(anchorFor(blocks, { block: 1, at: null }, { block: 1, at: null }, 1).quote, "Intro text.");
  assert.equal(anchorFor(blocks, { block: 1, at: 6 }, { block: 3, at: null }, 1).quote, "ext.\nAnchors\nthe quoted\ntext here");
});

test("touching the edge of a block does not take it", () => {
  // Triple-click ends at the start of the next block.
  const triple = anchorFor(blocks, { block: 1, at: 0 }, { block: 2, at: 0 }, 1);
  assert.equal(triple.quote, "Intro text.");
  assert.deepEqual(triple.blocks, [1]);
  const late = anchorFor(blocks, { block: 1, at: 10 }, { block: 3, at: 3 }, 1);
  assert.deepEqual(late.blocks, [2, 3]);
});

test("a selection of nothing visible has no anchor", () => {
  assert.equal(anchorFor(blocks, { block: 1, at: 4 }, { block: 1, at: 4 }, 1), null);
  assert.equal(anchorFor(blocks, { block: 6, at: 0 }, { block: 6, at: 0 }, 1), null);
});

test("a slice keeps the whitespace inside and trims the ends", () => {
  assert.equal(slice("  a  b c ", 1, 3), "b c");
  assert.equal(slice("ab", 0, 2), "ab");
});

test("an anchor records where its selection starts and ends", () => {
  const anchor = anchorFor(blocks, { block: 1, at: 5 }, { block: 3, at: 3 }, 1);
  assert.deepEqual(anchor.span, { start: { block: 1, at: 5 }, end: { block: 3, at: 3 } });
  // Taken whole, a block's span covers all of it.
  const inside = anchorFor(blocks, { block: 5, at: 1 }, { block: 5, at: 2 }, 1);
  assert.deepEqual(inside.span, { start: { block: 5, at: 0 }, end: { block: 5, at: 4 } });
});
