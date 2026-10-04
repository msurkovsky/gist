# 15. Serve the review page with `axum`, long-poll and embedded mermaid

Accepted — 2026-10-01. Built and tried in Claude Code by 2026-10-04.

## Context

`gk md-review serve` ([design](../md-review.md)) is an HTTP
server on 127.0.0.1 for one reviewer: one browser page, a background
`wait`, and short `reply`, `next` and `status` calls. Both the page and
`wait` have to learn about changes as they happen: the page about a new
round or an agent reply, `wait` about a submit or approval
([ADR 0013](0013-wake-the-agent-with-a-background-wait.md)). The page
renders mermaid diagrams, and the reviewer may be offline.

`gk` is a synchronous CLI with no async runtime. Release builds use LTO and
a single codegen unit. The clients (`wait`, `reply`, `next`, `status`,
`stop`) talk to `serve` over the same HTTP API, so they need a client too.

## Options

### Server

Crate data from crates.io, checked 2026-10-01.

- **`tiny_http`.** A small synchronous server, one thread per request. Its
  last release, 0.12.0, is from October 2022. The maintained fork,
  `tiny_http_dh`, has about 60 recent downloads; `rouille` is built on
  `tiny_http`. A security-facing parser with no maintainer to fix it.
- **Small synchronous servers** (`astra`, `may_minihttp`, `touche`): about
  a thousand recent downloads each and one maintainer; no better.
- **Hand-written HTTP on `std::net`, with `httparse`.** The parser is
  hyper's and well fuzzed, but keep-alive, body framing, limits and
  timeouts would be security code that `serve` owns, and the Host check
  against DNS rebinding depends on getting them right.
- **`hyper` directly.** Maintained and the base of the ecosystem, but
  routing, middleware and in-process testing would be ours to write.
- **`axum` on `tokio` (chosen).** Maintained by the tokio project, on
  `hyper`. Routing, shared state, one middleware layer for the token and
  Host checks, and routes tested without a socket
  (`tower::ServiceExt::oneshot`). It costs an async runtime, a larger
  dependency tree, longer builds and about 1–2 MB of binary. The runtime
  stays inside `serve`; every other subcommand stays synchronous. Its
  ecosystem also covers what later work may need (SSE, WebSocket,
  compression) without a second server.

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
  makes no external requests (`docs/md-review.md`, Security).
- **Downloaded on first use and cached:** small binary, but a download
  path, a cache and a checksum check to own, and the first review offline
  still fails.
- **Compiled into `gk` (chosen):** the pinned `mermaid.min.js`, with its
  checksum recorded in the repository, served by `serve`. It works offline
  and is always the reviewed version.

## Decision

`serve` runs `axum` on a current-thread `tokio` runtime, built in `serve`
alone. The page and `wait` both long-poll with requests of about a minute;
a `tokio::sync::Notify` wakes them on a change. The clients use `ureq`, a
synchronous HTTP client, so they need no runtime. `mermaid.min.js` is
pinned, checksummed and compiled into `gk` with `include_bytes!`.

## Consequences

The binary grows by the size of mermaid: 3.4 MB for 11.x minified
(measured 2026-10-01). Measured on the release build: `axum`, `tokio` and
`ureq` added 2.4 MB, above the 1–2 MB estimate; mermaid and the page
added 3.6 MB, for 11.7 MB in all. Compressing it in the binary and serving it with
`Content-Encoding: gzip` would cut that to under 1 MB; left for when the
size matters.

`tokio`, `axum` and `ureq` become dependencies of `gist-cli`. Async code
is confined to the HTTP API module of `serve`; the store, log and renderer
stay synchronous and are called from it, holding the log mutex only for
an append or a fold.

A waiting long-poll is a parked task, not a thread. A test flag shortens
the poll interval, as for `wait`.

## Verification

Route tests in `md_review/serve.rs` call the `axum` router in process and
cover the token and Host checks, the assets served without the token,
and stale-write 409s. End-to-end tests in
`tools/crates/gist-cli/tests/md_review.rs` cover the long-poll returning
on a change, on its interval, and at once to a page open across a
restart, through `ureq`. A test compares the embedded `mermaid.min.js`,
11.17.2, with the checksum in `assets/mermaid.min.js.sha256`.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-10-04 09:47 | Martin Surkovsky | Accepted: built, tested and tried in Claude Code |
| 2026-10-01 21:47 | Martin Surkovsky | Pin mermaid 11.17.2 with a checksum test; record measured sizes |
| 2026-10-01 07:24 | Martin Surkovsky | Choose `axum` over `tiny_http`, unmaintained since 2022; name the client |
| 2026-10-01 06:29 | Martin Surkovsky | Created |
