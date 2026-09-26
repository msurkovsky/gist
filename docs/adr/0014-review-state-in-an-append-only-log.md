# 14. Keep review state in an append-only log owned by the server

Proposed — 2026-09-26.

## Context

A `gk md-review` session ([design](../design/md-review.md)) holds comments,
questions, agent replies, round boundaries, and a snapshot of the file per
round. Three parties touch it: the browser page, the long-running `serve`
process, and short-lived CLI calls from the agent (`wait`, `reply`, `next`).
A review can last an hour; the browser may be reloaded, the server may crash,
and the agent needs the comments as text it can read.

The markdown file itself is not review state. The agent edits it in place
with its normal tools; `gk` only reads and snapshots it.

## Options

### Browser localStorage

Dismissed as the store, kept as a draft buffer. localStorage is scoped per
origin, which includes the port. A server restarted on a new random port
starts with an empty store, and the agent cannot read the browser's storage
at all. It stays for text typed but not yet sent to the server.

### Server memory only

Dismissed. Simplest while the server runs, but a crash or a stop loses the
review, and nothing is left to inspect when the agent misreads a comment.

### Files written by every process, with locks

Dismissed. `serve` and `reply` both appending to the same files needs
cross-process locking in Rust that works on Linux and macOS, and a reader
that tolerates a half-written line. One writer removes the problem instead
of managing it.

### Versions committed to git

Dismissed. The user decides what to commit. Intermediate versions are noise
once the review is approved, and a commit per round rewrites history the
user did not ask for.

### An append-only log and snapshots on disk, written only by `serve` (chosen)

## Decision

**Location.** `<repo root>/.md-review/<slug>/`, the slug derived from the
file's path relative to the root (`docs/foo.md` → `docs-foo-md`). `serve`
adds `.md-review/` to `.git/info/exclude`, never to the user's `.gitignore`.
Outside a git repository: open, tracked in the design.

**Contents.**

```
server.json      pid, port, token; mode 0600
events.jsonl     one JSON event per line, append-only
v1.md … vN.md    the working file as it was at the start of each round
```

Current state is a fold over `events.jsonl`. Events: `review_started`,
`message_posted`, `review_submitted`, `round_started`, `thread_resolved`,
`approved`; fields in the design.

**One writer.** Only `serve` writes the store. The page and the CLI calls
are HTTP clients on 127.0.0.1, authenticated by the token; the CLI finds the
port and token in `server.json`. A CLI call with no live server exits 1 and
names `serve`; it never writes the store itself.

**Resume.** `serve` on a file whose `server.json` names a live process
prints that URL and exits 0. A `server.json` whose process is gone is
replaced, and the new server rebuilds state from the log.

**End.** On approve, after `wait` has been released, `serve` deletes the
slug directory and exits. The working file already holds the final text;
git keeps whatever the user committed during the review. `stop` ends the
server and keeps the directory, so the review can resume.

## Consequences

Deletion of a directory is new for `gk`. It must be confined: the path is
built from the repository root and the slug, the slug never contains a path
separator or `..`, and a symlinked `.md-review` or slug directory is refused,
following ADR 0008 and ADR 0010. `docs/architecture.md` lists persisted
formats and deletions as review hot spots; `md_review/store.rs` joins them.

The log format is a persisted format. Within one review it only has to be
read back by the same binary, so it needs no version field yet; if reviews
are ever kept across `gk` upgrades, add one then.

A crash between appending an event and answering the client can make the
client retry and append twice. Messages carry a client-generated id; the
fold drops duplicates.

The server is a single point: if it is down, the agent cannot reply and the
page cannot post. Both fail loudly and `serve` resumes from the log.

## Verification

Not built. Tests in `tools/crates/gist-cli/tests/md_review.rs`: state
survives a server restart; a CLI call with no server exits 1 and writes
nothing; a second `serve` reuses the running one; a stale `server.json` is
replaced; approve removes the directory and nothing outside it; a symlinked
store directory is refused; `.git/info/exclude` gains the entry once; a
duplicated message id appears once in the folded state.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-26 22:20 | Martin Surkovsky | Created |
