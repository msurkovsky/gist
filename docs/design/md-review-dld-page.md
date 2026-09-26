# Markdown review page — detailed design (DLD)

Draft — 2026-09-26. Details the review page of the
[high-level design](md-review-hld.md). Nothing here is built; the interaction is
to be tried in a clickable mockup before this draft is settled.

## Model

Review mode in Google Docs or Word: the document on the left, comment cards
in a margin on the right, each card level with the text it anchors to. The
reader never leaves the text to comment, and every open question stays
visible next to its context.

This replaces the ⊕ dialog in the HLD's "Review page" section; see
[Changes to the HLD](#changes-to-the-hld).

## Layout

```
┌─ docs/foo.md · round 2 · 3 pending ─ NORMAL ─ [Show changes] [Submit review] [Approve] ─┐
│                                                  │                                      │
│  ## Scope                                        │  ┌────────────────────────────────┐  │
│  v1 covers ░░░one file░░░, comments and rounds.  │──│ You · pending                  │  │
│                                                  │  │ Why not several files?         │  │
│  ```mermaid                                      │  │ [Edit]  [Delete]               │  │
│  ...                                             │  └────────────────────────────────┘  │
│                                                  │  ┌────────────────────────────────┐  │
│  Parked: ▓▓▓Explain▓▓▓ ...                        │──│ You · question                 │  │
│                                                  │  │ What does parked mean?         │  │
│                                                  │  │ ── agent ──                    │  │
│                                                  │  │ Explain is out of v1 …         │  │
│                                                  │  │ [Reply]  [Resolve]             │  │
│                                                  │  └────────────────────────────────┘  │
└──────────────────────────────────────────────────┴──────────────────────────────────────┘
```

- **Toolbar.** Fixed at the top: file, round, pending count, mode indicator,
  Show changes, Submit review, Approve. Behaviour of the last two as in the
  HLD.
- **Document column.** The rendered markdown, readable width (about 80ch),
  mermaid diagrams inline. Anchored text is highlighted; the focused
  thread's highlight is stronger.
- **Margin.** Cards positioned at the vertical offset of their anchor. When
  two would overlap, the later one moves down and a connector line keeps it
  tied to its text. The focused card may push others down, as in Google
  Docs.
- **Overall comment.** Not a card: it lives in the Submit dialog.
- **Narrow window** (below about 1000px). The margin collapses to a drawer
  opened from the toolbar; highlights stay in the text and open the drawer
  at their card.

## Cards

A card is one thread: an anchor, the human's message, replies.

| State | Shown as | Actions |
|---|---|---|
| draft | text field, Comment / Explain | save, cancel (text kept) |
| pending | saved, not yet submitted | edit, delete |
| submitted | read-only, "agent is revising" | none |
| applied | collapsed, "applied in round N" | expand, reopen |
| declined | open, agent's reason shown | reply, resolve |
| answered | open, agent's answer shown | reply, resolve |
| orphaned | top of margin, old quote shown | reply, resolve |

- **Draft buffer.** Typing is kept in localStorage per file and anchor until
  the card is saved, per ADR 0014. A reload restores open drafts.
- **Replies** start a new message in the same thread, `kind` comment or
  question, and are pending until the next submit like any comment.
- **Resolved** threads collapse to a one-line stub; a filter in the margin
  hides them.
- **Explain (parked).** The answer streams into the card. The card shows a
  waiting state, can be cancelled, and shows an error with retry when the
  answerer fails. The document never changes from Explain.

## Selection and anchors

Mouse and keyboard produce the same thing: a DOM `Selection` over rendered
text. From it the page builds the anchor of the HLD: `quote`, `prefix`,
`suffix`, source `lines`, `headings`, `version`.

- **Source lines.** The renderer tags each block element with its source
  line range (`data-lines="12-18"`). A selection maps to the union of the
  ranges of the blocks it touches.
- **Non-text blocks.** A mermaid diagram, image or table can be selected
  only as a whole block; its quote is the block's source text.
- **Empty selection.** `<space>cc` with nothing selected anchors to the
  block under the caret.

### Re-anchoring after a round

The agent edits the file between rounds, so a thread's quote may move or
vanish. On each round the server re-attaches every open thread:

1. Exact `quote` with matching `prefix` and `suffix`.
2. Exact `quote` alone, nearest to the old `lines`, preferring the same
   `headings` path.
3. Otherwise the thread is **orphaned**: kept, shown at the top of the
   margin with the old quote, never silently dropped.

Fuzzy matching is left out of v1; orphaning is honest and cheap. Revisit
after real reviews show how often it happens.

## Changes between rounds

"Show changes" works like Word's track changes, against the previous
version: inserted text underlined in green, deleted text struck through in
red, inline. It settles HLD open question 5.

- The diff is by word within a block, by block otherwise. A changed mermaid
  block shows the new diagram with a "changed" marker and the old source on
  demand.
- An applied thread links to the change that applied it when the server can
  tell (its anchor's lines overlap a changed block); otherwise it only says
  "applied in round N".

## Keyboard

Vim bindings are the primary way to use the page; the mouse keeps working.
The cursor is the browser's own caret, moved and extended with
`Selection.modify(alter, direction, granularity)`, which works on rendered
lines. Chrome, Firefox and Safari implement it; it is not standardised.

### Modes

- **NORMAL.** Moves the caret. Default.
- **VISUAL** (`v`) and **VISUAL LINE** (`V`). Moves extend the selection.
  A mouse selection enters VISUAL.
- **INSERT.** Typing in a card. Bindings are off except `Esc` and
  `Ctrl-Enter`.

The toolbar shows the mode, like vim's statusline.

### Keys

| Mode | Keys | Action |
|---|---|---|
| normal, visual | `h l` / `j k` | character / rendered line |
| normal, visual | `w b e` | word |
| normal, visual | `0 $` | line start / end |
| normal, visual | `{ }` | paragraph |
| normal, visual | `gg G` | document start / end |
| normal, visual | `Ctrl-d Ctrl-u` | half page |
| normal | `]] [[` | next / previous heading |
| normal | `/` `n N` | search, caret on the match |
| normal | `]c [c` | next / previous thread; focuses its card |
| normal | `]h [h` | next / previous change, with Show changes on |
| normal | `v` `V` | visual / visual line |
| visual | `Esc` | back to normal |
| normal, visual | `<space>cc` | comment: open a draft card on the selection or block |
| normal, visual | `<space>ce` | explain (parked) |
| normal | `<space>rs` | open the Submit dialog |
| normal | `<space>ra` | open the Approve dialog |
| normal | `Tab` / `Shift-Tab` | move focus between text and the focused card |
| card | `Esc` | back to normal; draft kept |
| card | `Ctrl-Enter` | save the comment |
| normal | `?` | help overlay |

- **Counts.** A count before a move repeats it (`5j`).
- **Leader.** `<space>` is taken from page scrolling. After it the page
  waits about one second for the rest, showing the possible completions;
  `<space>` alone does nothing.
- **Dialogs, not actions.** No key submits or approves directly; the leader
  keys only open the dialogs, which need `Enter` to confirm.
- **Non-text blocks.** `j k` step over a diagram or image as one unit;
  `V` on it selects the block.

### Implementation

No library does vim bindings over a rendered page; they target editors. A
key table and a small mode machine on top of `Selection.modify`, a few
hundred lines under `assets/`. Search uses `window.find` or a text walk,
whichever behaves across the three browsers.

Reviewing the source in CodeMirror with its vim mode was the alternative:
mature bindings, but raw markdown and no diagrams, which the page exists to
avoid.

## Risks

- `Selection.modify` with `line` may jump oddly around tables, code blocks
  and diagrams. The mockup must be tried in Chrome, Firefox and Safari.
- Margin positioning with many threads near each other gets crowded. Google
  Docs' answer (focused card pushes others) is the plan; judge in the
  mockup.
- Re-anchoring by exact quote orphans threads whose text the agent reworded
  slightly. Accepted for v1.

## Changes to the HLD

Not yet made; proposed with this draft.

- "Review page": the ⊕ and dialog become a draft card in the margin; link
  here for detail.
- "Open questions" 5 (how deleted text is shown): settled by track changes.
- "Review log": thread states `applied`, `declined`, `answered`,
  `orphaned`, and whether orphaning is an event or derived on fold.

## Open questions

1. Is orphaning recorded in the log (`thread_orphaned`) or derived when the
   state is folded against the new version?
2. Can a pending comment be moved to a new anchor, or only deleted and
   rewritten?
3. Should `j k` move by rendered line or by block? Rendered line is vim-like;
   block is more predictable around diagrams.
4. Colours for highlights and changes must stay readable in black on white
   and for colour-blind readers; pick in the mockup.
5. Does the mockup live in the repo (for example `docs/design/md-review-dld-page/`)
   or only in a scratch directory until the design is settled?
