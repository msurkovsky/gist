# 10. Refuse a symlinked target root, and check every root before writing

Accepted — 2026-09-25.

## Context

ADR 0008 refused symlinks under a target root but let the root itself be one,
because "the user chooses the root, the repo does not". That is wrong for these
roots. `gk init --claude` writes `./.claude/skills` and `--codex` writes
`./.agents/skills`, relative to the repo the user is standing in, and `.claude`
is usually committed. A cloned repo chooses both directories, links included.

A review reproduced it. A repo that commits `.claude -> <dir>` makes
`gk init --claude` write the skills and the manifest into `<dir>`. With
`.claude/skills` linked and a file planted at a skill's path, `--force`
overwrites it and `--uninstall` then removes every recorded file through the
link. It is the attack 0008 set out to stop, one directory higher.

The same review found the "refused before it starts" promise only half kept.
Uninstall checked every recorded path before removing any, but per root, so
`--claude --codex --uninstall` could empty one root and refuse the other.
Install checked each file as it placed it, so a symlink under a late-sorting
skill, or a corrupt manifest in the second root, left earlier skills installed
and a manifest written, while the README said a refused run writes none.

## Options

### Keep allowing a symlinked root

Dismissed. Its premise is false above. The cost of dropping it is a user who
links `.claude/skills` into a dotfiles repo, and none is known here.

### Allow a link that resolves inside the repository

Dismissed, for 0008's reasons against `canonicalize`: it needs the path to
exist, and it leaves a gap between check and use. A link into the repo is also
no safer, since the repo can carry files and a manifest there.

### A flag that permits a symlinked root

Dismissed for now. Nobody has asked, and a flag that switches a safety check off
is one an agent adds by habit. Add it when someone has a real case.

### Check each root as it is reached

Dismissed. It is the behaviour that broke the promise.

### Refuse a link on the way to the root, and check every root first (chosen)

## Decision

`refuse_symlink` now covers the path from the working directory down to the
root, so `./.claude` and `./.claude/skills` are refused as well as anything
below them. The working directory itself may be a link: the user chose it.

Install and uninstall each run in two phases. The first, for every root, refuses
a symlinked root or manifest, refuses a corrupt or invalid manifest, and refuses
a symlink at any file the run would touch. It reads the disk and writes nothing.
The second places or removes. A refused run leaves every root as it found it. A
real I/O failure in the second phase, such as an unreadable file, still records
what was already placed, as 0007 says.

## Smaller fixes made with it

- A manifest is not rewritten when its bytes would not change, so a rerun that
  did nothing works in a read-only root.
- When placing fails and the manifest cannot be written either, the message says
  both, and that the files already placed are then untracked.
- A manifest path must equal its normal form. `a//b`, `a/./b` and `a/` used to
  pass as plain and stay separate keys for one file.

## Consequences

Someone who keeps `.claude` or `.claude/skills` as a link must unlink it or run
`init` where the path is a real directory. The error names the link.

Not covered: a manifest written by an earlier build that recorded a conflicted
file under its on-disk hash is still trusted, so `--uninstall` would remove that
file. That build was never released (0.1.0, untagged), so no version gate was
added. Delete `.gist-manifest.json` if you ran it and hit a conflict.

The `could not inspect` and `could not read` error arms still have no test that
reaches them by the intended route; see `TODO.md`.

## Verification

`tests/cli.rs`, each red before its fix:
`a_symlinked_skills_root_is_refused` (replacing the test that pinned the old
behaviour), `a_symlinked_dot_claude_is_refused_by_install_and_uninstall`,
`install_checks_every_root_before_writing_any`,
`a_symlink_found_late_in_the_skill_order_leaves_nothing_installed`,
`uninstall_checks_every_root_before_removing_any`, and
`an_unchanged_rerun_does_not_rewrite_the_manifest`. Unit tests in `init.rs`:
`manifest_paths_must_be_plain_and_relative`, extended, and
`a_manifest_write_failure_is_not_hidden_by_a_placing_failure`.

`an_io_failure_partway_still_records_the_files_already_placed` now plants a
directory where a skill file belongs. Its old setup, a file where a skill
directory belongs, trips the first phase.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-25 19:02 | Martin Surkovsky | Dropped the supersedes note, since ADR 0008 now states the root-symlink rule itself |
| 2026-09-25 06:51 | Martin Surkovsky | Created |
