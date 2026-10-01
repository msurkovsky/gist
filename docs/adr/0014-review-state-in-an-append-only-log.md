# 14. Keep review state in an append-only log owned by the server

Proposed — 2026-09-26.

## Context

A `gk md-review` session ([design](../design/md-review-hld.md)) holds comments,
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

Dismissed. `serve` and `reply` both appending to the same files needs a lock
around every write, shared by several processes, and a reader that
tolerates a half-written line. One writer removes the problem instead of
managing it. The one lock this design keeps is different: taken once at
startup and held for the server's lifetime, it only decides which process
is the writer.

### Versions committed to git

Dismissed. The user decides what to commit. Intermediate versions are noise
once the review is approved, and a commit per round rewrites history the
user did not ask for.

### An append-only log and snapshots on disk, written only by `serve` (chosen)

## Decision

**Location.** `<repo root>/.md-review/reviews/<key>/`, the key a hash of the
file's path relative to the root. `review_started` records the path.
`serve` adds `.md-review/` to `.git/info/exclude`, never to the user's
`.gitignore`. Outside a git repository: open, tracked in the design.

The key has to be derived from the path alone, so `serve` finds an existing
review, and has to be one path segment, so deletion stays confined. A
readable slug that flattens the path (`docs/foo.md` → `docs-foo-md`) was
dismissed: `docs/foo.md` and `docs-foo.md` share it, and one review would
resume the other's log. Mirroring the path (`reviews/docs/foo.md/`) never
collides but makes deletion walk a nested path. A hash does both; the cost
is that a directory name does not say which file it belongs to, which
`status` answers.

**Contents.**

```
lock             held by serve for its lifetime
server.json      pid, port, token, gk version; mode 0600
events.jsonl     one JSON event per line, append-only
v1.md … vN.md    the working file as it was at the start of each round
```

Current state is a fold over `events.jsonl`. Events: `review_started`,
`message_posted`, `message_edited`, `message_deleted`, `review_submitted`,
`review_delivered`, `round_started`, `thread_resolved`, `approved`; fields
in the design. `review_started` carries `format: 1`; `round_started` and
`approved` carry the version's hash. Whether a thread is orphaned is not
an event: it is derived on fold against the current version. Nor is
reopening: a reviewer message after a thread's resolution or its `applied`
outcome reopens it on fold.

**One writer.** Only `serve` writes the store. It takes an exclusive OS
file lock (`File::lock`) on `lock` before reading any state and holds it
until it exits; the OS releases it when the process dies. Within `serve`,
one mutex covers each append and fold. The page and the CLI calls are HTTP
clients on 127.0.0.1, authenticated by the token; the CLI finds the port
and token in `server.json`. A CLI call with no live server exits 1 and
names `serve`; it never writes the store itself. A CLI call whose `gk`
version differs from the one in `server.json` exits 1 and names `stop`.

**Resume.** A `serve` that cannot take the lock prints the live server's URL
and exits 0. One that takes it over a `server.json` whose process is gone
replaces the file, and rebuilds state from the log. A log whose `format`
this `gk` does not know is refused with exit 1 and left untouched.

**End.** Approve is delivered like a submit. `serve` first writes a review
record, `.md-review/records/<name>-<time>.md`, outside `reviews/`: every
round's threads, messages and outcomes, the approve note, and the approved
version with its hash. `<name>` is the path flattened for reading; it is
never looked up. When the working file differs from the approved version,
the record says so and carries the approved text in full and the block
diff to the working file, so deleting the store loses nothing that was
approved. The store stays until a client has received the approval with
the record's path; then `serve` deletes `reviews/<key>/` and exits. The working file already
holds the final text; git keeps whatever the user committed during the
review. `stop` ends the server and keeps the directory, so the review can
resume.

## Consequences

Deletion of a directory is new for `gk`. It must be confined: the path is
built from the repository root and the key, the key is hex and never
contains a path separator or `..`, and a symlinked `.md-review`, `reviews`
or store directory is refused,
following ADR 0008 and ADR 0010. `docs/architecture.md` lists persisted
formats and deletions as review hot spots; `md_review/store.rs` joins them.

The log format is a persisted format, and it outlives one binary: `stop`
keeps a review across a `gk` upgrade. `format` in `review_started` lets a
newer `gk` refuse a log it cannot read instead of misreading it, and the
`gk` version in `server.json` keeps a new CLI from talking to an old server.

The review record is the only part that outlives approval. It is
git-excluded with the rest of `.md-review/`; the skill copies it into the
merge request as review evidence, and the user deletes old records.

A crash between appending an event and answering the client can make the
client retry and append twice. Messages carry a client-generated id; the
fold drops duplicates.

A crash in the middle of an append leaves a last line cut short. `serve`
drops it on open: the event was never acknowledged, so no client acted
on it. Refusing the log instead would end the review over one lost
write. A log whose `review_started` names another file is refused, so a
copied store or a key collision never resumes the wrong review.

The server is a single point: if it is down, the agent cannot reply and the
page cannot post. Both fail loudly and `serve` resumes from the log.

## Verification

Not built. Tests in `tools/crates/gist-cli/tests/md_review.rs`: state
survives a server restart; a CLI call with no server exits 1 and writes
nothing; two concurrent `serve` leave one writer, and the lock is free
again after the writer is killed; a stale `server.json` is replaced; a
file whose flattened path matches another's gets its own store; a log with an
unknown `format` is refused and its bytes are unchanged; a CLI call against
a different `gk` version exits 1; an edited or deleted pending message
survives a restart as edited or gone; approve writes a complete record,
even for more threads than `--limit`, and with the approved text when the
working file differs, then removes the store directory and
nothing outside it; a `stop` before the approval is delivered keeps the
store; a symlinked store directory is refused; `.git/info/exclude` gains
the entry once; a duplicated message id appears once in the folded state.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-10-01 19:36 | Martin Surkovsky | Drop a line cut short by a crash; refuse a log of another file |
| 2026-10-01 06:29 | Martin Surkovsky | Derive reopening on fold, as the HLD now settles |
| 2026-09-30 22:10 | Martin Surkovsky | Apply the CEO review: store lock, format version, review record; key the store by a path hash |
| 2026-09-26 23:40 | Martin Surkovsky | Link the design by its new HLD name |
| 2026-09-26 22:20 | Martin Surkovsky | Created |
