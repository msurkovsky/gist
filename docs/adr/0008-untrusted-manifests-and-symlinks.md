# 8. Treat manifests and symlinks under a target as untrusted input

Accepted — 2026-09-24.

## Context

`gk init` runs inside repos the user may not have written. `.claude/skills/` is
usually committed, so a clone or a pull request can carry whatever is under it.
Two things there are controlled by the repo, not the user: the manifest
(`.gist-manifest.json`, `docs/adr/0007`) and the filesystem entries
themselves, symlinks included. `init` and `init --uninstall` trusted both.

Reproduced against the binary in a scratch directory before this change:

- A manifest entry `../../../outside/x` with a matching hash made `--uninstall`
  delete a file outside the root. An absolute path with `--force` deleted any
  file, the hash being skipped. The empty-directory cleanup then removed
  emptied directories outside the root too.
- A symlinked skill directory made `init` create files wherever it pointed, and
  `--force` overwrote an existing `SKILL.md` there. `--uninstall` removed
  through such a link the same way.
- A dangling symlink at the manifest path made `init` create a file at an
  arbitrary path.

Separately, the manifest was written in place, so a crash left a truncated
file. Since `init` refuses a corrupt manifest (ADR 0007), that turned a crash
into a stuck state.

## Options

### Check each path where it is joined to the root

Dismissed. Every join site has to remember, and one missed is the bug. There
are three today; a fourth is one edit away.

### Canonicalize and require the result to start with the root

Dismissed. `canonicalize` needs the path to exist, and a missing file is a
normal uninstall outcome (`Missing`), so it has no answer there. It resolves
symlinks where the goal is to reject them, and it leaves a window between the
check and the use.

### Follow symlinks that stay inside the root

Dismissed. Nothing gk places is a symlink, so a link under the root belongs to
the user or to the repo. `scripts/link.sh` makes links that point outside the
root by design, so a containment rule would refuse them anyway. Refusing all
of them gives the same result with one rule instead of two.

### Report a symlinked entry as a conflict

Dismissed. `--force` means "overwrite local changes", and a symlink must not be
something `--force` can override. It would also need a new status in both
report vocabularies.

### Validate paths at parse time, refuse symlinks, write atomically (chosen)

**Manifest paths are a `ManifestPath`**: relative, plain components only (no
`..`, no root, no prefix). It is checked in `TryFrom<String>` and used as the
deserialization type, so a manifest holding any other path fails to parse. Code
that joins a manifest path to the root can only ever hold one that stays inside
it. Paths gk builds from its own embedded tree are plain by construction and are
not re-checked.

**`refuse_symlink(root, rel)`** inspects every component under the root, the
file itself included, with `symlink_metadata`, before each manifest access,
placement, and removal. The error names the symlink and says to remove it. The
root itself may be a symlink: the user chooses it, the repo does not, and people
link `~/.claude/skills` into a dotfiles repo. `--uninstall` checks every recorded
path before removing any, so a refusal removes nothing.

**A manifest that is JSON but not a valid manifest** (`Rejected`: a bad path, a
missing field) makes `--uninstall` exit 1, naming the reason, and removes
nothing. ADR 0007 chose a silent zero-op for a manifest that is not JSON at all,
which is right for a truncated write but would read as success on a file built
to look harmless. An install run refuses both kinds. The split is `serde_json`'s
data errors against its syntax and EOF errors.

**The manifest is written to `.gist-manifest.json.tmp` and renamed.** The
temporary file is created with `create_new` (`O_EXCL`) after any leftover is
removed, so a symlink planted at that name is replaced, not followed. A plain
`fs::write` to the temporary name would follow it.

## Decision

Treat a manifest and everything under a target root as input the repo controls.
Reject a manifest path that could leave the root when the manifest is parsed.
Refuse to read, write, or remove through a symlink under the root. Fail
`--uninstall` on a manifest that parses but is invalid. Write the manifest
atomically, through an exclusively created temporary file.

## Consequences

A link under a target root, including one `scripts/link.sh` made, now stops
`init` and `--uninstall` until it is removed. The error says so and the README
notes it. Someone who symlinks individual skills into a project has to unlink
them before running `gk init` there.

Each path costs one `lstat` per component.

Not covered: a local attacker who can change the tree between the check and the
use, who already has write access there; and Windows, whose symlink and path
rules were not considered. The `could not inspect` error arm has no test, since
it needs an unreadable directory; it stays because deleting it would mean
carrying on after an I/O error.

## Verification

`init.rs` unit test `manifest_paths_must_be_plain_and_relative`. `tests/cli.rs`
end-to-end:
`uninstall_never_removes_a_file_outside_the_root_named_by_dotdot`,
`uninstall_never_removes_an_absolute_path_even_with_force`,
`init_refuses_a_manifest_whose_paths_leave_the_root`,
`uninstall_refuses_a_manifest_that_is_json_but_not_a_manifest`,
`init_refuses_to_write_through_a_symlinked_skill_directory`,
`init_refuses_a_manifest_that_is_a_dangling_symlink`,
`uninstall_removes_nothing_when_a_recorded_path_crosses_a_symlink`,
`uninstall_refuses_a_manifest_that_is_a_symlink`,
`a_symlinked_skills_root_is_still_allowed`,
`writing_the_manifest_leaves_no_temporary_file_and_clears_a_stale_one`, and
`a_symlink_planted_at_the_temporary_name_is_not_written_through`.

Each was run red against the source from just before its fix, with two
exceptions. `a_symlinked_skills_root_is_still_allowed` guards against
over-blocking and passes either way. `a_symlink_planted_at_the_temporary_name_is_not_written_through`
had no old behaviour to fail against, since the old code used no temporary
file, so it was checked by mutating the new implementation to a plain write.
