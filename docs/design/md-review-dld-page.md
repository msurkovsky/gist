# Markdown review page — detailed design (DLD)

2026-09-26, updated 2026-10-02. Details the review page of the
[high-level design](md-review-hld.md). Slice 1 is built as described here and
was tried by hand on 2026-10-01 instead of in a clickable mockup; see Trial 1
in `docs/cases/gist-md-review.md`. The CEO review
(2026-09-26) is applied: slice 1 is mouse only with block-level changes;
[Keyboard](#keyboard) and word-level changes are slice 2.

## Model

Review mode in Google Docs or Word: the document on the left, comment cards
in a margin on the right, each card level with the text it anchors to. The
reader never leaves the text to comment, and every open question stays
visible next to its context.

## Layout

```
┌─ docs/foo.md · round 2 · 3 pending ────────── [Show changes] [Submit review] [Approve] ─┐
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

- **Toolbar.** Fixed at the top: file, round, pending count, Show changes,
  Submit review, Approve, the connection indicator. Behaviour of Submit and
  Approve as in the HLD. The vim mode indicator joins it in slice 2.
- **Connection indicator.** Always visible, one of four states:

  | State | When | Shown as |
  |---|---|---|
  | agent listening | a `wait` polled within 90 s | green dot |
  | agent revising | the agent has the submit and has not run `next` yet | green dot; it runs no `wait` meanwhile, so this is not away |
  | agent away | `serve` answers, no `wait` for 90 s or more, and no submit being revised | amber dot, "agent away since 14:02; your submit is kept", the request that resumes the review ("Continue the review of docs/foo.md with me.") with a copy button |
  | server offline | the page cannot reach `serve` | red dot, "reconnecting…" |

  The 90 s grace hides the routine `wait` restart after a timeout, which
  takes seconds. Offline, the page retries with backoff (1 s doubling to
  30 s) and returns to the right state by itself; `serve` resumes on its
  old port when free, so an open tab finds it. The page cannot wake the
  agent; it can only say how.
- **Banners.** Below the toolbar, one at a time, most severe first: write
  refused because the page is stale (reload, drafts kept); the round is
  submitted; working file differs from the version on screen. Drafts
  survive all of them.
- **Revising banner.** From submit until the next round it says what the
  agent is doing: before it has the submit, "Submitted. The agent gets it
  when it next listens."; after, "The agent is revising since 14:02 · 1 of
  2 comments answered." Both add that the next round appears by itself.
  The count rises as replies arrive. A crashed agent leaves the banner up;
  the time since is how the reviewer tells.
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
| pending | saved, not yet submitted | edit, delete (logged; 409 after submit) |
| submitted | read-only, "sent"; the revising banner says the rest | none |
| applied | collapsed, "applied in round N" | expand, reopen (opens a reply) |
| declined | open, agent's reason shown | reply, resolve |
| answered | open, agent's answer shown | reply, resolve |
| orphaned | top of margin, old quote shown | reply, resolve |

- **Draft buffer.** Typing is kept in localStorage per file and anchor until
  `serve` has logged the card, per ADR 0014. A reload restores open drafts.
  A card saved while the server is offline stays in the buffer, marked
  "not saved yet", and is sent when the page reconnects; a 409 on resend
  keeps it as a draft, as for any stale write.
- **Replies** start a new message in the same thread, `kind` comment or
  question, and are pending until the next submit like any comment.
- **Reopen** opens a reply field; the reply reopens the thread. There is no
  reopen without a reason, and no reopen event: the fold treats a reviewer
  message after resolution or an `applied` outcome as reopening.
- **Resolved** threads collapse to a one-line stub; a filter in the margin
  hides them.
- **Explain (parked).** The answer streams into the card. The card shows a
  waiting state, can be cancelled, and shows an error with retry when the
  answerer fails. The document never changes from Explain.

## Page states

| State | Shown as |
|---|---|
| loading | the toolbar, and a placeholder where the document renders |
| empty file | "nothing to review"; Approve available, Submit disabled |
| render error | the error and the raw markdown; Approve and Submit available |
| mermaid block fails | that block's source and the mermaid error; the rest renders |
| no comments yet | a hint in the margin: "select text to comment" |
| approved | a final screen naming the approved round and the record path; the server is gone after it |

## Selection and anchors

In slice 1 a selection is made with the mouse; slice 2 adds the keyboard,
which produces the same thing. Either way it is a DOM `Selection` over
rendered text. From it the page builds the anchor of the HLD: `quote`,
`prefix`, `suffix`, `blocks`, source `lines`, `headings`, `version`.
Building the anchor is a pure function in `anchor.js`, unit tested with
`node --test`.

- **Blocks and source lines.** The renderer tags each block element with
  its index and source line range (`data-block="12" data-lines="12-18"`). A
  selection maps to the blocks it touches and the union of their ranges.
  Blocks are the document's top-level elements: a list, table or block
  quote is one block.
- **Non-text blocks.** A mermaid diagram, image or table can be selected
  only as a whole block; its quote is the block's rendered text, so it
  re-anchors like any other quote.

### Re-anchoring after a round

The agent edits the file between rounds, so a thread's quote may move or
vanish. On each round the server re-attaches every open thread. It matches
against the new version's rendered plain text, block by block, after
collapsing runs of whitespace, never against markdown source:

1. Exact `quote` with matching `prefix` and `suffix`.
2. Exact `quote` alone, only when it occurs once under the same
   `headings` path in both the old and the new version.
3. Otherwise the thread is **orphaned**: kept, shown at the top of the
   margin with the old quote, never silently dropped.

Orphaned is not logged; it is derived when the log is folded against the
current version. Step 2 never guesses between copies of a phrase: a
phrase copied in the old version and deleted at its original place is
not unique in the old version, so the thread is orphaned rather than
attached to the copy.

Fuzzy matching is left out of slice 1; orphaning is honest and cheap.
Revisit after real reviews show how often it happens.

## Changes between rounds

"Show changes" marks what changed against the previous version, by block:

- A changed or added block gets a bar in the left gutter and a light tint.
- A deleted block shows as a struck-through stub where it was, expandable
  to its old text.
- A changed block, a mermaid diagram included, shows its new form with a
  "changed" marker and its old source on demand.
- Slice 2 adds Word-style track changes inside a changed block: inserted
  words underlined in green, deleted words struck through in red.
- An applied thread links to the change that applied it when the server can
  tell (its anchor's lines overlap a changed block); otherwise it only says
  "applied in round N".

## Keyboard

Slice 2. Slice 1 is mouse only; keyboard access there is `Tab` between the
toolbar, the document and the cards. This section waits for a mockup that
shows `Selection.modify` behaves across browsers.

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

- Slice 2: `Selection.modify` with `line` may jump oddly around tables,
  code blocks and diagrams. The mockup must be tried in Chrome, Firefox and
  Safari.
- Margin positioning with many threads near each other gets crowded. Google
  Docs' answer (focused card pushes others) is the plan; judge in the
  mockup. The packing is a pure function in `margin.js`, unit tested with
  `node --test`.
- Re-anchoring by exact quote orphans threads whose text the agent reworded
  slightly. Accepted for slice 1.

## Testing

`anchor.js` (selection to anchor) and `margin.js` (card packing) hold the
logic as pure functions, tested with `node --test` in `just ci`, with no
npm dependencies. The DOM glue is covered by the host case in
`docs/cases/gist-md-review.md`.

## Open questions

1. Can a pending comment be moved to a new anchor, or only deleted and
   rewritten? Editing changes the body only.
2. Slice 2: should `j k` move by rendered line or by block? Rendered line is
   vim-like; block is more predictable around diagrams.
3. Colours for highlights and changes must stay readable in black on white
   and for colour-blind readers; pick in the mockup.
4. Does the mockup live in the repo (for example `docs/design/md-review-dld-page/`)
   or only in a scratch directory until the design is settled?
