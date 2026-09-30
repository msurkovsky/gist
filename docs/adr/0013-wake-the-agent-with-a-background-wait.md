# 13. Wake the agent with a background `gk md-review wait`

Proposed — 2026-09-26.

## Context

`gk md-review` lets a human review a rendered markdown file in the browser
and submit all comments at once ([design](../design/md-review-hld.md)). When the
human presses Submit, the agent is idle: its last turn ended with "here is the
URL". Something has to put the review in front of it and start a turn.

An MCP server can answer a tool call but cannot make an idle agent act. The
agent acts on a user message or on a harness event. Slice 1 targets Claude
Code only; Codex follows once the loop has been used there.

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

The agent channel for slice 1 is the `gk` CLI, not MCP:

- `gk md-review serve <file>` runs as a background task for the whole review
  and prints the URL as its first line of stdout.
- `gk md-review wait` returns a pending submit or approval at once, or blocks
  until one arrives, prints it, and exits 0. No timeout by default;
  `--timeout` exists for callers that want one and exits 1 when it fires. It
  exits 1 at once when no server is running for the file, or when the
  server's `gk` version differs from its own, naming `gk md-review stop`.
- `reply`, `next`, `status` and `stop` return immediately like any other `gk`
  subcommand. After approval, `reply` and `next` exit 1 naming it.

Delivery is durable and at least once. A submit stays pending until the
agent runs `next`, and an approval until a client has received it; a `wait`
that was killed, cut by compaction or never started loses nothing, because
the next `wait` returns what is pending. `wait` long-polls in requests of
about a minute and reconnects, so no single HTTP request lasts for the
whole review. A delivery after the first is marked with the first delivery
time and the threads the agent already replied to; the skill re-reads the
file and skips those threads, so a repeat does not apply an edit twice.
Exactly once was rejected: it needs the agent to acknowledge receipt, and a
crash between receipt and acknowledgement loses the review silently.

`wait` prints the human rendering by default, which is what the agent reads;
`--json` gives the envelope for tests and scripts. Output is data plus a
one-line reminder of the next commands, never a prompt: the procedure lives in
the `gist-md-review` skill.

The page shows "no agent listening" while a submit is pending and no `wait`
is connected, and `status` reports waiters and an undelivered submit, so a
broken loop is visible to both the reviewer and the agent.

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

At-least-once delivery puts the burden of idempotence on the skill: it must
treat a redelivered review as possibly half done. The `redelivered` marker
makes that explicit rather than leaving the agent to guess.

## Verification

Not built. The first host run checks, in Claude Code: the agent wakes on
`wait` exit with the review visible; a `wait` left running for 30 minutes is
not cut off; `wait` with no server exits 1 with a message naming `serve`.
End-to-end tests in `tools/crates/gist-cli/tests/md_review.rs` cover the exit
codes and both renderings, and delivery: a `wait` started after the submit
returns it; a delivered submit fetched again by a new `wait` is marked
redelivered with the replied threads; an approval with no `wait` running is
returned by the next one; `reply` after approval exits 1; a client with a
different `gk` version exits 1. Long-poll tests shorten the poll interval
with a test flag instead of sleeping.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-30 22:05 | Martin Surkovsky | Apply the CEO review: durable, at-least-once delivery; version check |
| 2026-09-26 23:40 | Martin Surkovsky | Link the design by its new HLD name |
| 2026-09-26 22:20 | Martin Surkovsky | Created |
