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

## Later: md-review, after first feedback

The plan as it stood at merge is in git:
`fd63c3d:docs/design/md-review-hld.md` and `md-review-dld-page.md` there,
with the full designs of the items below.

- Slice 2: vim key bindings (needs a mockup showing `Selection.modify`
  behaves across browsers); word-level track changes in a changed block;
  export and import of a review; finer blocks and block pairing by
  content; images by repository path, and commenting on an image; a link
  from an applied thread to its change.
- Parked: Explain, a question answered by a read-only fork of the agent
  session (open: how the agent learns its session id); Codex as a second
  host; live comments delivered one by one.
- Known gaps from the 2026-10-04 branch review, left until use shows they
  matter:
  - two `wait`s running at once both get a submit, the second marked
    redelivered with no threads replied, so an agent acting on both edits
    twice; a thread edited but not yet replied is invisible to a repeat;
  - an edit after approve, before delivery, leaves the record without the
    approved text;
  - `stop` and `wait` on a moved or deleted file exit 1, leaving the
    detached server to `kill`;
  - Add comment keeps a stale anchor when the selection changes by
    keyboard, and a drag released outside the document shows no button;
  - a failed append can leave a torn line mid-log; deleting a reply that
    reopened a thread leaves it open; `wait` output is bounded by thread
    count only.

## Later: distribution and platform support

- `gk init` currently installs skills only. Decide how reusable rules should be
  activated in consumer projects before offering installation of them.
- Remove `hooks/commit-msg.sh` only when old symlink users have migrated to
  `gk hook install`. It remains a compatibility forwarder.
- First Josh view: `views/claude/workspace.josh` for a consumer that needs one.
- CI runs `gk` on macOS, but not `scripts/vendor.sh`. Validate vendor
  import/update with Bash 3.2 and BSD tools before claiming it works there.
- Embedded file data does not retain executable modes. Before an owned skill
  depends on an executable helper, decide how installation preserves modes,
  includes resources outside `skills/`, and handles platform-specific binaries.
  The existing experimental guardrail script is affected.
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
