# `gk init --claude --uninstall` — track and reverse what init installed

Plan only. Not yet implemented.

## Context

`gk init --claude` (`crates/gist-cli/src/init.rs`) writes files but keeps no
record of what it wrote. There is no way to answer "what did gk put on this
machine" or to remove it cleanly — a user has to know the exact file list by
heart, and `rm -rf .claude/skills` would also delete any of their own skills
that happen to live in the same flat directory.

Need a manifest: a record of exactly what `init --claude` installed, so an
uninstall can act on *that record* rather than re-deriving the file list from
whatever skills happen to be embedded in the binary running `--uninstall` —
those two can differ (a newer/older `gk`, a locally modified file, a file the
user deleted by hand). The manifest is the source of truth for uninstall; the
embedded `SKILLS` tree is only ever the source of truth for install.

## Manifest

New file written alongside the vendored skills:
`.claude/skills/.gist-manifest.json` — dot-prefixed so it reads as tool
metadata, not a skill directory, and Claude Code's skill discovery (which
looks for `<dir>/SKILL.md`) never trips on it.

```json
{
  "gk_version": "0.1.0",
  "files": [
    { "path": "gist-doc-review/SKILL.md", "sha256": "…" },
    { "path": "gist-doc-review/references/public-surface.md", "sha256": "…" },
    { "path": "gist-outline/SKILL.md", "sha256": "…" }
  ]
}
```

Sorted by `path` (deterministic ordering, per `docs/tool-contract.md`).
`sha256` is what lets uninstall tell "untouched since install" from "locally
modified" — the same distinction `decide()` already makes for install
conflicts, just checked in the other direction.

Written unconditionally at the end of every successful `init --claude` run
(install or uninstall) from the files actually present afterward — it always
reflects current disk state, not a diff. It is not itself subject to
`decide()`/`place()`'s installed/unchanged/overwritten bookkeeping and is not
counted in `Totals`.

New dependency: `sha2` (workspace + `gist-cli`), same pattern as `include_dir`
in `tools/Cargo.toml` / `crates/gist-cli/Cargo.toml`.

## CLI surface

Extend `init::Args`, no new subcommand:

```rust
pub struct Args {
    #[arg(long)]
    claude: bool,
    #[arg(long)]
    force: bool,
    /// Remove what a previous `init --claude` installed, using the manifest
    #[arg(long)]
    uninstall: bool,
}
```

`--claude` stays the required target selector for both directions (today
`.claude/skills` is the only target, but the flag already means "this
target", not "do the install verb"). `--uninstall` and `--force` are
modifiers on it:

| flags | behavior |
|---|---|
| `init --claude` | install/update (today's behavior, unchanged) |
| `init --claude --force` | install, overwriting conflicts (today's behavior, unchanged) |
| `init --claude --uninstall` | remove manifest-recorded files that are unmodified; leave modified ones, report them |
| `init --claude --uninstall --force` | remove everything the manifest recorded, modified or not |
| `init` (no `--claude`) | refusal, unchanged |
| `init --uninstall` (no `--claude`) | same refusal — `--claude` is checked first regardless of `--uninstall` |

`--force` keeping one meaning ("override the safety check, in whichever
direction we're going") beats a second flag — considered `--yes` /
`--purge` for uninstall specifically and dropped it, no real behavioral
difference to justify a second name.

A standalone `gk drop-skills` subcommand was the other option raised — dropped
in favor of a flag on `init` because uninstall's every input (target path,
force semantics, manifest location) already belongs to `init`; a separate
subcommand would just re-declare `--claude`/`--force` under a new name.

## Uninstall behavior

1. Refuse (existing path) if `--claude` wasn't passed.
2. Read `.claude/skills/.gist-manifest.json`. Missing manifest → nothing to
   do, succeed with an all-zero report (exit 0) — running `--uninstall` twice
   is not an error, matching install's own idempotency.
3. For each recorded `{path, sha256}`:
   - file missing on disk → status `Missing` (already gone, not an error)
   - file present, hash matches → delete it, status `Removed`
   - file present, hash differs, no `--force` → leave it, status `Kept`
   - file present, hash differs, `--force` → delete it, status `Removed`
4. After processing files, prune now-empty directories bottom-up (removes
   `gist-doc-review/references/` then `gist-doc-review/` if both end up
   empty; never touches a directory that still has something in it, gk's or
   not).
5. Rewrite the manifest with only the `Kept` entries. If none are kept,
   delete the manifest file too — a full uninstall leaves zero gk footprint,
   short of `.claude/skills/` itself if the user has other skills there.
6. Exit code: `FAILURE` if any file was `Kept` (mirrors the install-conflict
   pattern — `--force` is the documented way past it), `OK` otherwise.

## Types (sketch, not final — TDD drives the exact shape)

```rust
#[derive(Serialize)]
struct ManifestEntry { path: String, sha256: String }

#[derive(Serialize, Deserialize)]
struct Manifest { gk_version: String, files: Vec<ManifestEntry> }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum UninstallStatus { Removed, Kept, Missing }

#[derive(Serialize)]
struct UninstallReport {
    target: String,
    totals: UninstallTotals, // removed / kept / missing counts
    files: Vec<UninstallFileReport>,
}

impl UninstallReport {
    fn has_kept(&self) -> bool { self.totals.kept > 0 }
}
```

`Human` impl for `UninstallReport` follows the same shape as `Report::human`
(one line per file, a trailer line when something was kept, pointing at
`--force`).

## Tests (TDD, unit first then e2e in `tests/cli.rs`)

Unit (`init.rs`, no I/O, mirrors the existing `decide()` tests):
- a file whose hash matches the manifest entry decides `Removed`
- a file whose hash differs decides `Kept` without `--force`, `Removed` with it
- a manifest entry with no file on disk decides `Missing`

E2e:
1. `init_claude_writes_a_manifest_after_install` — install, read
   `.claude/skills/.gist-manifest.json`, assert 3 entries, paths match, hashes
   are 64 hex chars.
2. `init_claude_uninstall_removes_untouched_files` — install, uninstall;
   assert all 3 files gone, `gist-outline/` and `gist-doc-review/` gone,
   manifest gone, exit 0.
3. `init_claude_uninstall_keeps_locally_modified_files_and_reports_them` —
   install, modify `gist-outline/SKILL.md`, uninstall; assert exit 1, that
   file still present with the local content, the other two files and their
   now-empty `gist-doc-review/` gone, manifest on disk now lists only the one
   kept entry.
4. `init_claude_uninstall_force_removes_everything` — same setup, `--force`;
   assert exit 0, nothing left under `.claude/skills/`, manifest gone.
5. `init_claude_uninstall_handles_a_file_already_deleted_by_hand` — install,
   `rm` one vendored file directly, uninstall; assert it reports `missing`
   for that path, still removes the other two, no error.
6. `init_claude_uninstall_with_no_prior_install_is_a_zero_op_not_an_error` —
   fresh repo, `init --claude --uninstall`; assert exit 0, all totals zero.
7. `init_claude_uninstall_human_output_lists_kept_files_and_the_force_hint` —
   non-JSON run with one kept file; assert stdout mentions the path and
   `--force`.

Test 3 and 5 are the failure/edge paths `docs/tool-contract.md` calls out —
the ones worth writing before the happy path stops being interesting.

## ADR

`docs/adr/0004-manifest-driven-uninstall.md`, same Context/Options/Decision/
Consequences/Verification shape as 0002 and 0003:

- **Context**: uninstall needs to know exactly what a prior `init --claude`
  did, independent of what the currently-running binary's embedded `SKILLS`
  happen to contain.
- **Options dismissed**: re-derive the removal set from the current `SKILLS`
  static (wrong the moment the binary's skill set has changed since install —
  removes files that don't belong to this manifest, or fails to remove ones
  that were renamed away); no manifest, just `rm -rf` the flat directory
  (destroys any of the user's own skills colocated there); a path-only
  manifest with no hash (can't distinguish "safe to remove" from "locally
  modified," which is the whole point).
- **Decision**: write `.claude/skills/.gist-manifest.json` on every
  `init --claude` run; uninstall reads it and never consults `SKILLS`.
- **Consequences**: an extra small file in the target repo's `.claude/skills/`
  (dot-prefixed, ignorable); `sha2` becomes a new dependency; uninstall is now
  provably safe against binary version skew, at the cost of being unusable if
  the manifest itself is deleted or corrupted (falls back to the
  zero-op-not-an-error path, not a crash).
- **Verification**: test 5 above (manifest references a file that no longer
  exists) and test 3 (manifest hash disagrees with disk) are the two cases
  that would silently misbehave without the manifest.

## Critical files

- `tools/crates/gist-cli/src/init.rs` (manifest read/write, uninstall path,
  new types)
- `tools/crates/gist-cli/tests/cli.rs` (7 new e2e tests)
- `tools/Cargo.toml`, `tools/crates/gist-cli/Cargo.toml` (`sha2` dependency)
- `docs/adr/0004-manifest-driven-uninstall.md` (new)
- `docs/tool-contract.md` — no change expected, `--uninstall` fits the
  existing exit-code contract as-is

## Verification

1. `just ci`.
2. Manual: `init --claude`, edit a file, `init --claude --uninstall`,
   confirm the edited file survives and the others are gone; `--force` and
   confirm it's gone too.
