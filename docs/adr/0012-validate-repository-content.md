# 12. Validate repository content with gk

Accepted — 2026-09-25.

## Context

The shell consistency check accepted unclosed skill frontmatter. Its vendor
check inspected only non-merge first-parent history, so normal PR merges hid
local edits, and worktree edits were invisible. A green check did not establish
the properties advertised by the contribution instructions.

## Options

- Extend the shell checks: rejected because YAML and Git topology need structured
  parsing, and repository policy puts growing tools in Rust.
- Add a separate Python validator: rejected because it adds another runtime to
  the standard check path and duplicates Git/output plumbing already in `gk`.
- Add `gk check`: chosen. Reuse the CLI contract, local Git library, and fixture
  testing; use a YAML parser rather than approximate YAML with regular expressions.

## Decision

`gk check [path]` validates owned skill YAML, name/directory agreement, invocation
policy consistency, inline local Markdown resources, README entries, case-file
presence, shell syntax, and final ADR Changelogs. `serde-saphyr` supplies YAML
parsing and duplicate-key rejection; JSON values retain scalar types for validation.
Local links resolve relative to their document and stay inside the skill directory.
Inline code examples are excluded. Percent-encoded targets are decoded before
filesystem lookup, after separating the fragment; confinement checks use the decoded path.
The command does not fetch remote links or execute skill instructions.

Vendor verification reads the registry and reachable import merges carrying the
matching `Upstream:` URL/SHA and `Filter:` trailers. The newest import must descend
from all other imports for that vendor; competing imports require reconciliation.
The registry accepts `name url [branch]`, defaulting the branch to `main` as the
importer does; branch names do not affect content verification.
Its second parent's subtree is compared with HEAD by Git object identity and mode.
Index/worktree status additionally catches staged, unstaged, and non-ignored
untracked changes. Ignored untracked files and directories are excluded; tracked
files remain checked even when an ignore pattern matches their names.
Missing history, malformed import records, and unregistered vendor content
fail. Import records remain reviewed repository data, not cryptographic provenance.

`--vendors-only` supports `scripts/vendor.sh check`. `scripts/check.sh` forwards to
the source checkout's binary via `cargo run --locked`, so an installed binary cannot
silently run old checks. Both wrappers require the Rust toolchain. CI retains full
Git history and the checked-in Cargo lockfile.

Results use the normal human/JSON contract. Completed validation reports include
the total failure count, sorted details bounded by `--limit`, and `truncated`.
Any failure exits 1 even with a zero detail limit. Operational failures use the
error envelope. No files are installed or rewritten by this command.

## Consequences

The check now measures content instead of guessing from commit shape. A local
vendor edit that was subsequently fully reverted is clean; forbidden historical
editing is a review concern, while this gate protects current imported content.
Ignored editor files should not interrupt local checks. Ignored build artifacts
are also outside this integrity check. Since embedding still reads the source tree,
use a clean checkout for release builds; this check does not certify that an arbitrary
working tree contains only distributable inputs.

Mechanical validation does not approve intent, evaluate a skill's decisions, or
prove its behavior in either host. Those remain explicit evidence under
`docs/skill-contract.md`. Reference-style Markdown links and arbitrary prose
paths are outside the resource check; required resources use inline links.

## Verification

`tools/crates/gist-cli/tests/check.rs` exercises the cases in
`docs/cases/repository-checks.md`, including a real branch merge containing a
vendor edit and a valid update merged from another branch.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-25 21:50 | Martin Surkovsky | Record the implemented decision as accepted and attribute changes to the human author |
| 2026-09-25 21:38 | Martin Surkovsky | Match importer defaults, ignore local artifacts, and resolve Markdown links correctly |
| 2026-09-25 21:15 | Martin Surkovsky | Replace demonstrated false passes with checks of the actual content |
