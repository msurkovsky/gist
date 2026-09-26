# 13. Wake the agent with a background `gk md-review wait`

Proposed — 2026-09-26.

## Context

`gk md-review` lets a human review a rendered markdown file in the browser
and submit all comments at once ([design](../design/md-review-hld.md)). When the
human presses Submit, the agent is idle: its last turn ended with "here is the
URL". Something has to put the review in front of it and start a turn.

An MCP server can answer a tool call but cannot make an idle agent act. The
agent acts on a user message or on a harness event. v1 targets Claude Code
only; Codex follows once the loop has been used there.

Claude Code starts a turn when a background shell task the agent launched
exits, and shows the agent the task's output. That is documented in its Bash
tool.

## Options

### MCP server with `get_review`, human says "submitted" in chat

Dismissed. This was the first sketch in `TODO.md`. It works on any host that
speaks MCP, which made it attractive for Codex. But the human has to switch to
the terminal and type after pressing Submit, which the browser page exists to
avoid. It also adds a new surface to `gk`, registration in host config, and a
tool-contract section for MCP, all to deliver one message per round.

### `claude -p --resume <session>` started by the server on submit

Dismissed. The human is usually sitting in that session in the terminal. A
second process resuming the same session appends to a transcript the running
UI holds in memory; at best the UI never shows that turn, at worst the two
diverge. Not tested, and not worth testing: the premise is two writers on one
conversation. `--fork-session` avoids the conflict but runs the edit in a
session the human is not watching.

### Channels: an MCP server pushes a notification into the session

Dismissed for now. A research preview that needs the session started with
`--channels`. Revisit if it becomes stable; it would also carry live
comments.

### Background `wait` that exits on submit (chosen)

The agent runs `gk md-review wait` as a background task. The command blocks
until the human submits or approves, prints the review, and exits. Claude Code
wakes the agent with that output. Nothing to register, nothing typed in chat,
and the agent's normal tools do the rest.

## Decision

The agent channel for v1 is the `gk` CLI, not MCP:

- `gk md-review serve <file>` runs as a background task for the whole review
  and prints the URL as its first line of stdout.
- `gk md-review wait` blocks until `review_submitted` or `approved`, prints
  the review, and exits 0. No timeout by default; `--timeout` exists for
  callers that want one and exits 1 when it fires. It exits 1 at once when no
  server is running for the file.
- `reply`, `next`, `status` and `stop` return immediately like any other `gk`
  subcommand.

`wait` prints the human rendering by default, which is what the agent reads;
`--json` gives the envelope for tests and scripts. Output is data plus a
one-line reminder of the next commands, never a prompt: the procedure lives in
the `gist-md-review` skill.

## Consequences

`docs/tool-contract.md` says tools are never interactive and never unbounded.
`serve` and `wait` block, which it does not cover. Before implementation it
gains a section on long-running subcommands: they never prompt; `serve`
writes only its first result line to stdout; `wait` writes only its final
result; timeouts are opt-in and exit 1; no progress output.

The loop depends on a Claude Code behaviour, not a protocol. If Claude Code
stops waking the agent on background task exit, the loop breaks; the first
host run under `docs/cases/md-review.md` checks it, together with whether a
long-running background task is ever cut off.

Codex has no known way to wake an idle TUI session. Adding Codex means a
second channel, likely `codex exec resume` driven by the server, or MCP after
all. The rest of the design (server, review log, CLI clients) stays; only how
the agent is woken changes.

Live comments, delivered one by one, fit the same shape: `wait` exits on each
message instead of on submit.

## Verification

Not built. The first host run checks, in Claude Code: the agent wakes on
`wait` exit with the review visible; a `wait` left running for 30 minutes is
not cut off; `wait` with no server exits 1 with a message naming `serve`.
End-to-end tests in `tools/crates/gist-cli/tests/md_review.rs` cover the exit
codes and both renderings.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-26 22:20 | Martin Surkovsky | Created |
| 2026-09-26 23:40 | Martin Surkovsky | Link the design by its new HLD name |
