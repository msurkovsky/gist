// Packing comment cards into the margin, level with their text.
// docs/design/md-review-dld-page.md#layout.

/** Space between two cards, in pixels. */
export const GAP = 8;

/**
 * The top of each card, for `cards` in document order, each `{top,
 * height}` with `top` the offset of its anchor. A card that would overlap
 * the one above moves down. The `focused` card, an index, stays level with
 * its text and pushes the cards above it up instead; when they would leave
 * the margin's top, it moves down as little as it can.
 */
export function pack(cards, focused = -1, gap = GAP) {
  const tops = cards.map((card) => Math.max(0, card.top));
  if (cards.length === 0) return tops;
  const down = (from) => {
    for (let index = Math.max(1, from); index < cards.length; index++) {
      tops[index] = Math.max(tops[index], tops[index - 1] + cards[index - 1].height + gap);
    }
  };
  if (focused < 0 || focused >= cards.length) {
    down(1);
    return tops;
  }
  for (let index = focused - 1; index >= 0; index--) {
    tops[index] = Math.min(tops[index], tops[index + 1] - cards[index].height - gap);
  }
  if (tops[0] < 0) {
    tops[0] = 0;
    down(1);
  } else {
    // The cards above are settled; only the focused one and those below move.
    down(focused + 1);
  }
  return tops;
}
