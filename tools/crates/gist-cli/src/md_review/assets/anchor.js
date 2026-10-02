// Selections to anchors, on the blocks `serve` renders. An anchor's span
// says where it is; the server moves it with the anchor between versions.
// Offsets count visible characters only: whitespace is skipped, as the
// server's re-anchoring skips it (anchor.rs), so the page's text and the
// server's may differ in whitespace and still agree.
// docs/design/md-review-dld-page.md#selection-and-anchors.

/** Characters of context kept on each side of a quote, as anchor.rs keeps. */
export const CONTEXT_CHARS = 40;

/** Blocks a selection takes whole: their rendered text is not what shows. */
const WHOLE = new Set(["mermaid", "table", "html", "rule"]);

// Rust's `char::is_whitespace`, the Unicode White_Space property; JavaScript's
// `\s` differs from it in two characters.
const SPACE = /[\t\n\v\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/u;

/** The visible characters of `text`, one code point each. */
export function chars(text) {
  return Array.from(text).filter((c) => !SPACE.test(c));
}

/** How many visible characters `text` has. */
export function visible(text) {
  return chars(text).length;
}

/** Whether a selection in a block of `kind` takes the whole block. */
export function selectsWhole(kind) {
  return WHOLE.has(kind);
}

/**
 * The anchor for a selection from `start` to `end`, each `{block, at}`: a
 * block index and the visible characters before the point in that block.
 * `at` is `null` at a block edge, or where the page's text of the block is
 * not the server's, so the point is not known: the block is taken whole.
 * `blocks` are the server's, with `kind`, `text`, `lines` and `headings`.
 * The anchor's `span` is the selection in the same counting, so the page
 * draws it without searching. Returns `null` when the selection quotes
 * nothing.
 */
export function anchorFor(blocks, start, end, version) {
  if (end.block < start.block || (end.block === start.block && (end.at ?? Infinity) < (start.at ?? 0))) {
    [start, end] = [end, start];
  }
  const keys = blocks.map((block) => chars(block.text));
  let from = { block: start.block, at: start.at };
  let to = { block: end.block, at: end.at };
  if (from.at === null || selectsWhole(blocks[from.block].kind)) from.at = 0;
  if (to.at === null || selectsWhole(blocks[to.block].kind)) to.at = keys[to.block].length;
  from.at = Math.min(from.at, keys[from.block].length);
  to.at = Math.min(to.at, keys[to.block].length);
  // A selection that only touches the edge of a block does not take it:
  // triple-click ends at the start of the next block.
  while (from.block < to.block && from.at >= keys[from.block].length) {
    from = { block: from.block + 1, at: 0 };
  }
  while (to.block > from.block && to.at === 0) {
    to = { block: to.block - 1, at: keys[to.block - 1].length };
  }

  const offsets = [];
  let total = 0;
  for (const key of keys) {
    offsets.push(total);
    total += key.length;
  }
  const first = offsets[from.block] + from.at;
  const last = offsets[to.block] + to.at;
  if (first >= last) return null;
  const all = keys.flat();

  const parts = [];
  for (let index = from.block; index <= to.block; index++) {
    const begin = index === from.block ? from.at : 0;
    const finish = index === to.block ? to.at : keys[index].length;
    parts.push(slice(blocks[index].text, begin, finish));
  }
  const quote = parts.filter((part) => part !== "").join("\n");
  const indexes = [];
  for (let index = from.block; index <= to.block; index++) indexes.push(index);
  return {
    quote,
    prefix: all.slice(Math.max(0, first - CONTEXT_CHARS), first).join(""),
    suffix: all.slice(last, last + CONTEXT_CHARS).join(""),
    blocks: indexes,
    lines: [blocks[from.block].lines[0], blocks[to.block].lines[1]],
    headings: blocks[from.block].headings,
    version,
    span: { start: { block: from.block, at: from.at }, end: { block: to.block, at: to.at } },
  };
}

/**
 * The part of `text` from its `from`th to its `to`th visible character,
 * with the whitespace inside kept and the ends trimmed.
 */
export function slice(text, from, to) {
  const points = Array.from(text);
  let seen = 0;
  let begin = points.length;
  let finish = points.length;
  for (let index = 0; index < points.length; index++) {
    if (SPACE.test(points[index])) continue;
    if (seen === from && begin === points.length) begin = index;
    seen += 1;
    if (seen === to) {
      finish = index + 1;
      break;
    }
  }
  return points.slice(begin, finish).join("").trim();
}
