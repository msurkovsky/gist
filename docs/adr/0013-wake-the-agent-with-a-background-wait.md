# 13. Wake the agent with a background `gk md-review wait`

Accepted — 2026-09-26. Built and tried in Claude Code by 2026-10-04.

## Context

`gk md-review` lets a human review a rendered markdown file in the browser
and submit all comments at once ([design](../md-review.md)). When the
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

- `gk md-review serve <file> --detach` starts the server as a process of
  its own and returns once it has printed the URL. The server must run for
  the whole review, and the host stops a background task at its limit, so
  it is not a host task; `wait` is the only one.
- `gk md-review wait <file> --timeout <dur>` returns a pending submit or approval
  at once, or blocks until one arrives, prints it, and exits 0. When the
  timeout fires first it prints a `timeout` event and exits 0; the skill
  starts it again. When no server is running for the file, or the server
  stops during the wait, it prints a `stopped` event and exits 0: like a
  timeout, an outcome the skill reads without parsing a message. It exits
  1 when the server's `gk` version differs from its own, naming
  `gk md-review stop`.
- `reply`, `next`, `status` and `stop` return immediately like any other `gk`
  subcommand. After approval, `reply` and `next` exit 1 naming it.

Delivery is durable and at least once. A submit stays pending until the
agent runs `next`, and an approval until a client has received it; a `wait`
that was killed, cut by compaction or never started loses nothing, because
the next `wait` returns what is pending. `wait` long-polls in requests of
about a minute and reconnects, so no single HTTP request lasts for the
whole review. A delivery after the first is marked with the first delivery
time and the threads the agent already replied to; the skill re-reads the
file and skips those threads, so a repeat does not apply a replied edit
twice. An edit made but not yet replied to is not in that list; nor is
any reply when two `wait`s run at once (`TODO.md`).
Exactly once was rejected: it needs the agent to acknowledge receipt, and a
crash between receipt and acknowledgement loses the review silently.

`wait` prints the human rendering by default, which is what the agent reads;
`--json` gives the envelope for tests and scripts. Output is data plus a
one-line reminder of the next commands, never a prompt: the procedure lives in
the `gist-md-review` skill.

The timeout is required and sits below the host's background-task limit.
Claude Code stops a background task at its timeout, at most 2 hours,
reports it as killed and tells the agent not to restart it. A `wait` that
times out itself completes normally, so the restart is the skill's rule,
not a fight with the host. The skill sets both from one value: 110 minutes
for `wait` under a 120-minute task limit. A submit made during the restart
is pending and returned by the next `wait`.

The page shows the agent as away when no `wait` has polled for 90 seconds,
a grace that hides the routine restart, and `status` reports waiters and an
undelivered submit, so a broken loop is visible to both the reviewer and
the agent.

## Consequences

`docs/tool-contract.md` says tools are never interactive and never unbounded.
`serve` and `wait` block, so it gained a section on long-running
subcommands: they never prompt; `serve` writes only its first result line
to stdout; `wait` writes only its final result; a timeout is an outcome and
exits 0; restarting loses nothing; no progress output.

The loop depends on a Claude Code behaviour, not a protocol. If Claude Code
stops waking the agent on background task exit, or lowers its background
limit below the `wait` timeout, the loop breaks; the host case under
`docs/cases/gist-md-review.md` checks it.

An idle review costs one short agent turn per timeout, about one every
two hours.

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

A spike on 2026-10-01 in Claude Code (a Python page with a
button, blocking until clicked) showed:

- a click wakes the agent with the output;
- a task that reaches its background limit is reported killed, its output
  cut, with a note telling the agent not to restart it;
- a task that times out itself 20 s before a 120 s limit completes with
  exit 0 and its output intact;
- a submit after 31 minutes wakes the agent, 8 s later, from a background
  browser tab.

The host case checks the same with the real `wait`, and that `wait` with no
server reports it stopped, naming `serve`.
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
| 2026-10-04 09:47 | Martin Surkovsky | Accepted: built, tested and tried in Claude Code; name the gaps a repeat leaves |
| 2026-10-02 22:23 | Martin Surkovsky | Name the file in the `wait` command, which now requires it |
| 2026-10-02 21:55 | Martin Surkovsky | A stopped server is a `wait` outcome, not exit 1 |
| 2026-10-01 22:28 | Martin Surkovsky | `serve` detaches instead of running as a background task, which the host would stop at its limit |
| 2026-10-01 17:44 | Martin Surkovsky | The tool contract section now exists |
| 2026-10-01 17:41 | Martin Surkovsky | Point to the case file by its skill name, as `gk check` requires |
| 2026-10-01 06:12 | Martin Surkovsky | Require a `wait` timeout below the host limit, after the wake spike |
| 2026-09-30 22:05 | Martin Surkovsky | Apply the CEO review: durable, at-least-once delivery; version check |
| 2026-09-26 23:40 | Martin Surkovsky | Link the design by its new HLD name |
| 2026-09-26 22:20 | Martin Surkovsky | Created |
