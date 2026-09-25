# TODO

## Next: validate the skill workflow in real hosts

- Run the scenarios in `docs/cases/gist-outline.md` and
  `docs/cases/gist-doc-review.md` in Claude Code and Codex. Record actual
  host/model, request, fixture, observed behavior, and limitations. Static
  instruction review and passing CLI tests are not host evaluation.
- Use the manual review map in `CONTRIBUTING.md` on the next skill change.
  Revise the workflow from observed friction before automating it.

## Remaining manifesto implementation

- Backfill prose cases and test-to-case links for unchanged tool behavior.
  New and changed behavior follows `docs/cases/README.md` now.
- Build the mechanical coherence mapper only after the manual format has been
  exercised: pair changed modules with source sections, and tests with cases.
  Its judgment belongs in an independent review context and remains evidence
  for the human. A red run must fail on the relevant behavior, not compilation.
- `docs/architecture.md` identifies areas needing extra review attention. A hook
  that routes those reminders does not exist yet; define its format when needed.
- Extend source pointers as modules change. The initial architecture map and
  ADR 0001 pointer are present; exhaustive link enforcement remains future work.
- Resolve the uncovered I/O error arms against `rules/code-and-comments.md`:
  they can run in production but have no tests. The earlier init review left
  that decision open; the rule has no carve-out.

## Proposed: browser review loop for markdown

Rough shape only. A human reviews a markdown file rendered in the browser,
comments on selected text, and the agent revises it. Each round shows what
changed since the last one. Skill first: `gist-md-review` drives the loop; a
new `gk` subcommand (working name `gk md-review`) serves it.

- **Loop (decided for v1: whole-file rounds).** Agent starts the server and
  prints the URL. Human reviews the whole file, comments on selections, and
  presses "Submit review" once done. The agent receives all comments at
  once, edits the file, replies per comment (applied / declined with
  reason). Page reloads with changes highlighted. Repeat until the human
  marks the document done. The agent never sees half a review.
- **Rendering.** Server side markdown with source positions (e.g. comrak
  `sourcepos`) so a selection maps back to source lines. Mermaid renders in
  the page. All assets embedded in `gk`; no CDN, works offline.
- **Anchors.** A comment stores the quoted text, surrounding context, and the
  source line range, so it survives small edits and the agent can locate it
  without guessing.
- **Changes.** Snapshot the file at each round. Diff the snapshot against the
  current file; highlight new or modified text green, word-level inside
  prose, whole block for code and diagrams. Deletions: open question (marker,
  toggle, or hidden).
- **Agent channel: MCP.** `gk` runs a stdio MCP server that also hosts the
  HTTP server for the page. Claude Code and Codex both speak MCP, so one
  channel serves both. Tools, roughly: `open_review(file)` returns the URL;
  `get_review()` returns the submitted comments with anchors, or `pending`;
  `reply(comment_id, status, note)` records the agent's answer. v1 does not
  block: the human submits in the browser and says so in chat, then the
  agent calls `get_review`. That sidesteps host tool-call timeouts and the
  tool contract's "never interactive".
- **State.** Rounds, snapshots, and comments in a sidecar directory, not in
  git history. Location and gitignore handling undecided.
- **Security.** Bind to 127.0.0.1 with a random token in the URL; any local
  process could otherwise post comments the agent will act on.

Open before building:

- MCP is a new surface for `gk`. ADR for choosing it over a CLI-only
  channel, and a section in `docs/tool-contract.md`: JSON results, bounded
  lists, errors as MCP errors. Decide who registers the server in host
  config (`gk init`, or a manual step the skill names).
- HTTP stack and markdown/diff crates; cost of embedding mermaid in binary
  size. ADR if the rejected options are real.
- Multiple files per session, or one file per server.
- Relation to `gist-doc-review`, which reviews code comments, not documents.
  Pick names that do not collide.

Later, after v1 has been used:

- **Live comments.** Each comment reaches the agent as it is written, with its
  anchor and surrounding section as context, instead of one batch per round
  (as Antigravity does on its artifacts). The browser side is plain HTTP to
  the `gk` server either way. The hard part is the agent side: an MCP server
  cannot make an idle agent act on its own. Pushing into a running session
  needs host-specific support. Unverified notes on what exists:
  - Claude Code: a background shell task wakes the agent when it exits, so
    `gk md-review wait` run in the background fires on submit without MCP.
    The Monitor tool turns each stdout line into an event, one per comment.
    Channels (research preview) let an MCP server push notifications into a
    session started with `--channels`. Hooks fire on harness events only.
  - Outside a running session: `claude -p --resume <session>` or the Agent
    SDK starts a new turn headless.
  - Codex: `codex exec` (and `exec resume`), app-server JSON-RPC drives
    turns. No known way to push into a running TUI session.

  Verify each against current docs before designing.
- **Round summary.** Agent writes a short "what changed and why" shown above
  the highlights.

## Later: distribution and platform support

- `gk init` currently installs skills only. Decide how reusable rules should be
  activated in consumer projects before offering installation of them.
- Remove `hooks/commit-msg.sh` only when old symlink users have migrated to
  `gk hook install`. It remains a compatibility forwarder.
- First Josh view: `views/claude/workspace.josh` for a consumer that needs one.
- CI is Linux-only. Validate on macOS before claiming support, especially vendor
  import/update behavior with Bash 3.2 and BSD tools, then consider a CI job.
- Embedded file data does not retain executable modes. Before an owned skill
  depends on an executable helper, decide how installation preserves modes,
  includes resources outside `skills/`, and handles platform-specific binaries.
  The existing experimental guardrail script is affected. Consider embedding
  only vendor skill trees instead of entire experimental repositories.
- Renamed and removed skills remain after a normal install rerun. An explicit
  uninstall/reinstall is the current migration path under
  [ADR 0007](docs/adr/0007-manifest-driven-uninstall.md). Revisit automatic
  reconciliation and `gk doctor` only with a concrete consumer need.

## Later: remaining shell tooling

- `scripts/check.sh` now forwards to `gk check`; the vendor integrity check is
  also Rust. Import/update/list in `scripts/vendor.sh` and `scripts/link.sh`
  still predate the one-command shell rule. Port them when their behavior grows.
- `scripts/vendor.sh` passes the registry URL to `git fetch` without `--`;
  a leading dash is interpreted as an option. Contributors only.

Completed plans leave this file when their branch merges. Lasting decisions,
behavior, and evidence belong in the linked documentation and review record.
