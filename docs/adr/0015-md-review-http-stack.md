# 15. Serve the review page with `tiny_http`, long-poll and embedded mermaid

Proposed — 2026-10-01.

## Context

`gk md-review serve` ([design](../design/md-review-hld.md)) is an HTTP
server on 127.0.0.1 for one reviewer: one browser page, a background
`wait`, and short `reply`, `next` and `status` calls. Both the page and
`wait` have to learn about changes as they happen: the page about a new
round or an agent reply, `wait` about a submit or approval
([ADR 0013](0013-wake-the-agent-with-a-background-wait.md)). The page
renders mermaid diagrams, and the reviewer may be offline.

`gk` is a synchronous CLI with no async runtime. Release builds use LTO and
a single codegen unit.

## Options

### Server

- **`axum` on `tokio`.** Well maintained and the common choice, but it
  brings an async runtime and a large dependency tree into a CLI that
  serves a handful of requests, for one subcommand.
- **Hand-written HTTP on `std::net`.** No dependency, but request parsing,
  body limits and header checks are security code that `serve` would own,
  and the Host check against DNS rebinding depends on parsing headers
  right.
- **`tiny_http` (chosen).** A small synchronous server: requests arrive on
  a channel and a thread answers each. A thread per long-poll is fine for a
  few clients. Its last release, 0.12.0, is from October 2022; a
  maintained fork, `tiny_http_dh`, exists.

### Page updates

- **Polling** at a fixed interval: simple, but either slow or busy.
- **Server-sent events:** pushes at once, but a second mechanism next to
  the `wait` long-poll, and a stream that has to be held open and resumed.
- **Long-poll (chosen):** the page asks for changes after the state it
  has; the server answers when there are some or after about a minute.
  Same mechanism as `wait`, so one implementation and one set of tests.

### Mermaid

- **From a CDN:** nothing in the binary, but the page breaks offline, needs
  the network, and tells a third party that a review is open. The page
  makes no external requests (HLD Security).
- **Downloaded on first use and cached:** small binary, but a download
  path, a cache and a checksum check to own, and the first review offline
  still fails.
- **Compiled into `gk` (chosen):** the pinned `mermaid.min.js`, with its
  checksum recorded in the repository, served by `serve`. It works offline
  and is always the reviewed version.

## Decision

`serve` uses `tiny_http`, one thread per request. The page and `wait` both
long-poll with requests of about a minute. `mermaid.min.js` is pinned,
checksummed and compiled into `gk` with `include_bytes!`.

## Consequences

The binary grows by the size of mermaid: 3.4 MB for 11.x minified
(measured 2026-10-01). Compressing it in the binary and serving it with
`Content-Encoding: gzip` would cut that to under 1 MB; left for when the
size matters.

`tiny_http` has not had a release since 2022. `serve` binds to 127.0.0.1
only and checks the token and the Host header on every request, which
limits what a parser bug exposes. Run `cargo audit` when adding it; if an
advisory appears or the crate needs a fix, switch to the fork or another
synchronous server behind the same HTTP API module.

Long-poll requests hold a thread each; with one page and one `wait` that
is two threads. A test flag shortens the poll interval, as for `wait`.

## Verification

Not built. End-to-end tests in `tools/crates/gist-cli/tests/md_review.rs`
cover the long-poll returning on a change and on its interval, and the
token and Host checks. `scripts/check.sh` or a test compares the embedded
mermaid with the recorded checksum.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-10-01 06:29 | Martin Surkovsky | Created |
