# 7. Manifest-driven `init --uninstall`

Accepted — 2026-09-24.

## Context

`gk init` writes files into a target repo's `.claude/skills/` and/or
`.agents/skills/` but kept no record of what it wrote. There was no way to
answer "what did gk put on this machine" or to remove it cleanly — a user had
to know the exact file list by heart, and `rm -rf .claude/skills` would also
delete any of their own skills that happen to live in the same flat
directory.

Uninstall needs to act on a record of exactly what a prior `init` run placed
at a given target root, not on whatever the currently running binary's
embedded `skills`/`experimental` trees happen to contain — those two can
differ: a newer or older `gk`, a locally modified file, a file the user
deleted by hand, or (since `docs/adr/0006`) a target root that was never
built from the same `--experimental` selection as another.

## Options

### Re-derive the removal set from the current embedded `SKILLS`/`EXPERIMENTAL`

Dismissed. Wrong the moment the binary's skill set has changed since install
— removes files that don't belong to what was actually installed at this
root, or fails to remove ones renamed away since. Also wrong the moment
`--uninstall` isn't given the same `--experimental` selection the original
`install` was, since there is no way to know what that was without a record.

### No manifest, just `rm -rf` the flat target directory

Dismissed. Destroys any of the user's own skills colocated in the same flat
`.claude/skills/` or `.agents/skills/` directory — gk does not own that
directory, only the entries it placed in it.

### A path-only manifest, no hash

Dismissed. Can't distinguish "safe to remove" from "locally modified since
install," which is the entire point of a separate uninstall mode instead of
just replaying the file list.

### Per-root manifest, hash-verified, manifest is the sole source of truth for uninstall (chosen)

Every `init` run that touches a target root — `.claude/skills` or
`.agents/skills` — writes `<root>/.gist-manifest.json` at the end of the run:
`{gk_version, files: [{path, sha256}, ...]}`, sorted by `path`, recording only
what gk placed. `gk_version` is for debugging and, later, for telling a user
their installed skills predate the running `gk`; nothing reads it yet. Each
run merges into the previous manifest: a file placed or found identical gets
the hash of the embedded content, a file left alone as a conflict keeps
whatever an earlier run recorded (nothing, if it was never gk's), and files
outside this run's selection stay recorded. Recording a conflicted file's
on-disk hash would let `--uninstall` delete a file gk never wrote.

A symlink anywhere under a target root, the manifest and each file included,
makes `init` and `--uninstall` refuse: a write or removal through it lands
wherever the link points. Uninstall checks every recorded path before
removing any. The root itself may be a symlink.

Every `path` is checked when the manifest is parsed: relative, plain
components only, no `..`, no root. A manifest is a file a repo can carry, and
joining an unchecked path to the root would let `--uninstall` delete anything
the user can reach. Dot-prefixed so it reads as tool metadata, not a skill
directory — Claude Code's discovery only looks for `<dir>/SKILL.md` and never
trips on it.

`--uninstall` reads that root's manifest and only that — never `SKILLS` or
`EXPERIMENTAL` — and for each recorded entry: missing on disk → `Missing`
(already gone, not an error); present and hash matches → delete it,
`Removed`; present and hash differs → `Kept` unless `--force`, which removes
it anyway. `--force` keeps its one existing meaning ("override the safety
check") rather than gaining a second uninstall-specific flag — no behavioral
difference would justify a second name. Empty ancestor directories are pruned
bottom-up after deletions; `std::fs::remove_dir` already refuses a non-empty
directory on its own, so pruning is safe to attempt on every touched ancestor
without a separate emptiness check. The manifest is rewritten with only the
`Kept` entries (their original recorded hash, not a fresh one — so a modified
file stays protected across repeated `--uninstall` runs, not just the first
one), or deleted entirely if nothing was kept. The target root directory
itself (`.claude/skills/`, `.agents/skills/`) is never removed, even on a
full uninstall — it may hold skills of the user's own.

`--claude`/`--codex` stay the target selectors for both directions, now
modified by `--uninstall`: `init --claude --codex --uninstall` acts on both
roots' manifests independently; passing only one acts on only that one, per
its own manifest, leaving a target left unselected untouched.
`--experimental` combined with `--uninstall` is a refusal, not a silent
no-op or a narrowing filter — uninstall's whole premise is that it does not
consult what's currently embedded, `--experimental` names a *live* package,
and letting the two coexist would look like "uninstall only this package" to
a reader when it does nothing of the sort.

A standalone `gk drop-skills` subcommand was considered and dropped in favor
of a flag on `init` — uninstall's every input (target roots, force
semantics, manifest location) already belongs to `init`; a separate
subcommand would just re-declare `--claude`/`--codex`/`--force` under a new
name.

## Decision

Write `<root>/.gist-manifest.json` on every `init` run, per selected target
root, including a run that fails partway, so files placed before the error
stay removable. `--uninstall` reads the manifest for each selected root and acts only
on it, independent of the embedded skill trees and independent of the other
selected root, if any.

## Consequences

An extra small file lands in each target root (`.gist-manifest.json`,
dot-prefixed, ignorable). New `sha2` dependency (workspace + `gist-cli`),
same pattern as `include_dir`. Uninstall is now provably safe against binary
version skew and against a target root that was built with a different
`--experimental` selection than another — at the cost of being unusable if
the manifest itself is deleted or corrupted, which falls back to the
zero-op-not-an-error path (same as a target `init` never touched), not a
crash. An install run does not take that fallback for a corrupt manifest: it
refuses, because rebuilding over the file would silently drop tracking of
every file outside the run's selection. Deleting the file acknowledges that.
A manifest that is valid JSON but not a valid manifest (a path leaving the
root, a missing field) is different again: someone wrote it on purpose, or a
newer `gk` did, so silence would read as success. `--uninstall` exits 1
naming the reason and removes nothing. `Report` becomes an enum over an install-direction and an
uninstall-direction report, since the two produce genuinely different
per-file status vocabularies (`installed`/`unchanged`/`overwritten`/
`conflict` vs. `removed`/`kept`/`missing`); `Report::has_conflicts` is
replaced by `Report::is_failure`, checked against whichever variant ran.

## Verification

`init.rs` unit tests cover the per-file decision (`Removed` on hash match,
`Kept` vs. `Removed` on mismatch with/without `--force`, `Missing` with
nothing on disk) with no I/O. `tests/cli.rs` end-to-end:
`init_claude_writes_a_manifest_after_install`,
`init_claude_uninstall_removes_untouched_files`,
`init_claude_uninstall_keeps_locally_modified_files_and_reports_them`,
`init_claude_uninstall_force_removes_everything`,
`init_claude_uninstall_handles_a_file_already_deleted_by_hand`,
`init_claude_uninstall_with_no_prior_install_is_a_zero_op_not_an_error`,
`init_claude_uninstall_human_output_lists_kept_files_and_the_force_hint`,
`init_uninstall_with_experimental_is_a_refusal`,
`init_claude_uninstall_does_not_touch_a_codex_target_left_unselected`,
`init_claude_uninstall_does_not_delete_a_file_that_conflicted_at_install_time`,
`a_conflict_on_rerun_keeps_the_hash_the_first_run_recorded`,
`rerunning_without_experimental_keeps_earlier_experimental_files_tracked`,
`init_refuses_to_overwrite_a_corrupt_manifest`,
`init_claude_uninstall_with_a_corrupt_manifest_is_a_zero_op`,
`an_io_failure_partway_still_records_the_files_already_placed`,
`uninstall_never_removes_a_file_outside_the_root_named_by_dotdot`,
`uninstall_never_removes_an_absolute_path_even_with_force`,
`init_refuses_a_manifest_whose_paths_leave_the_root`,
`uninstall_refuses_a_manifest_that_is_json_but_not_a_manifest`,
`init_refuses_to_write_through_a_symlinked_skill_directory`,
`init_refuses_a_manifest_that_is_a_dangling_symlink`,
`uninstall_removes_nothing_when_a_recorded_path_crosses_a_symlink`, and
`a_symlinked_skills_root_is_still_allowed`.
