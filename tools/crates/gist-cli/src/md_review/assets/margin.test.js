// Case: docs/cases/gist-md-review.md#page-margin
import { test } from "node:test";
import assert from "node:assert/strict";
import { GAP, pack } from "./margin.js";

const card = (top, height = 50) => ({ top, height });

test("cards apart stay level with their text", () => {
  assert.deepEqual(pack([card(0), card(100), card(300)]), [0, 100, 300]);
});

test("an overlapping card moves down in document order", () => {
  assert.deepEqual(pack([card(0), card(10), card(20)]), [0, 50 + GAP, 2 * (50 + GAP)]);
});

test("a card pushed down pushes the next one only if they meet", () => {
  assert.deepEqual(pack([card(0), card(10), card(200)]), [0, 58, 200]);
});

test("the focused card stays level and pushes the cards above it up", () => {
  assert.deepEqual(pack([card(100), card(120), card(130)], 1), [120 - 50 - GAP, 120, 120 + 50 + GAP]);
});

test("cards above the focused one keep their place when there is room", () => {
  assert.deepEqual(pack([card(0), card(200)], 1), [0, 200]);
});

test("the focused card moves down when the cards above would leave the top", () => {
  assert.deepEqual(pack([card(0), card(10)], 1), [0, 50 + GAP]);
});

test("a focused index out of range packs as if none were focused", () => {
  assert.deepEqual(pack([card(0), card(10)], 5), [0, 58]);
  assert.deepEqual(pack([]), []);
});

test("a card above the margin's top is placed at it", () => {
  assert.deepEqual(pack([card(-20)]), [0]);
});
