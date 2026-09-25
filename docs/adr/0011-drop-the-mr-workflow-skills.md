# 11. Drop the MR workflow skills

Accepted — 2026-09-25. Supersedes the per-branch type in `docs/adr/0009`.

## Context

The `gist-mr-start`, `gist-mr-ready`, `gist-mr-reply`, and `gist-mr-close`
skills were meant to walk a change from branch to closed MR, backed by
`gist-toolchain` and a `languages/` adapter per language. None of it was used on
real work. Once we laid the four skills along an actual workflow, they did not
hold up:

- `mr-start` installed the commit-msg hook, which is per repo and permanent, not
  per branch. That is repo setup, not a step in starting a feature.
- `mr-ready` and `mr-reply` both pushed, so the gate never ran on the pushes a
  review round makes.
- `mr-close` never merges; merging happens in the forge. It only covered
  superseding or abandoning an MR.
- `gist-toolchain` had one real caller, `mr-ready`, and no adapter was ever
  written. Projects already name their check commands in `CLAUDE.md`.
- Nothing covered rebasing, CI failures, or cleanup after merge.

What was worth keeping were a handful of constraints inside those procedures:
the backup ref before a rewrite, what goes in a description, the reply format,
the supersede order. Those apply to every MR, so they belong in an always-on rule,
not in a skill someone has to remember to run.

## Options

### Rename and restructure (`gist-feature-start`, `gist-mr-submit`, ...)

Dismissed. Better names do not fix skills whose steps collide, and the hook
install would still sit in the wrong place.

### Keep `gist-mr-reply` alone

Dismissed. Its loop (fetch, skip answered, fix, reply) is what an agent does
anyway once the reply rules are in context. The rules carry the value.

### Write the first language adapter so the gate works

Dismissed. It would add a second place for commands a project's `CLAUDE.md`
already names, and it would keep a layer alive for one caller.

## Decision

- Delete the four `gist-mr-*` skills, `gist-toolchain`, `languages/`, and
  `hooks/post-rebase-nag.sh`, whose only message was "run gist-toolchain".
- Add `rules/merge-requests.md` with the constraints that survive.
- Point to the vendored `mattpocock` skills for review (`code-review`), PR text
  (`pr`), and rebase conflicts (`resolving-merge-conflicts`).
- Keep `gk hook commit-msg` and `mr.commitPattern`, which are repo-level. Drop
  `branch.<name>.mrType`: only `mr-start` ever set it.

## Consequences

`gk init --claude` now installs two skills. Re-running it in a repo installed
before this change leaves the old skill directories in place. The manifest still
records them, since a run keeps entries outside its selection (ADR 0007), so
`gk init --claude --uninstall` followed by `gk init --claude` removes them. A typed commit convention still works through `mr.commitPattern`,
but nothing holds a branch to one type any more.
