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

## In progress: browser review loop for markdown

Design under review in [docs/design/md-review-hld.md](docs/design/md-review-hld.md), with the
review page detailed in
[docs/design/md-review-dld-page.md](docs/design/md-review-dld-page.md):
`gk md-review` and the `gist-md-review` skill, Claude Code first. Open
questions are listed there; this entry leaves when the feature ships.

A CEO review (2026-09-26, scope reduction) settled decisions D2–D19; the
HLD, the DLD and ADRs 0013 and 0014 carry them. Record, outside the repo:
`~/.gstack/projects/msurkovsky-gist/ceo-plans/2026-09-26-md-review.md`.

The CEO review covered architecture, errors, security, tests and
deployment, so no full eng review; the steps below replace it, in order:

1. ~~Validate the wake.~~ Done 2026-10-01 with a spike: a submit after
   31 minutes woke the agent. The host kills a task at its limit and says
   not to restart it, so `wait` now times out first (ADR 0013, HLD Settled).
2. ~~Answer the open questions.~~ Done 2026-10-01: `axum`, long-poll
   and embedded mermaid (ADR 0015); 1 MiB cap; quote unique in the old
   version too; optional `<file>` on clients; a reply reopens a thread.
3. ~~Write the tool contract section and the behavior cases.~~ Done
   2026-10-01: `docs/tool-contract.md` "Long-running subcommands" and
   `docs/cases/gist-md-review.md`, both approved.
4. ~~Implement, starting with the store, log and lock (ADR 0014).~~ Done
   2026-10-01: slice 1 built; first trial recorded in
   `docs/cases/gist-md-review.md`.

Fix from use: a mouse selection opens the comment draft at once. Keep
the selection visible first and open the draft from an icon or button
next to it, as Google Docs does.

Slice 2, deferred by the review: vim key bindings; word-level track changes
(slice 1 marks changed blocks); export and import of a review.

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
