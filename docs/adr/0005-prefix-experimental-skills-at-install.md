# 5. Prefix experimental skills at install, not at the source

Accepted — 2026-09-24.

## Context

`gk init --experimental=<package>` vendors one tree under `experimental/`
(full third-party history, per `docs/adr/0004-compose-via-josh.md`) into a
target repo's flat `.claude/skills/`, same as `--claude` does for this repo's
own skills. Those upstream skills carry bare names — `tdd`, `retro`, `pr` —
exactly the generic, collision-prone names `docs/adr/0003` renamed this
repo's own skills away from. Renaming at the source, `0003`'s answer, is not
available here: the source is someone else's repository, re-imported
verbatim by `scripts/vendor.sh` on every update, so a local rename would be
silently lost on the next `just vendor update`.

## Options

### Leave names as-is

Dismissed for the same reason `0003` dismissed it for canonical skills, only
worse — every skill in `experimental/mattpocock/skills/` is a bare, common
word, not the one or two `outline`/`doc-review` cases that prompted `0003`.

### Namespace by directory instead of by name (`.claude/skills/mattpocock/tdd/`)

Dismissed. Claude Code discovers project skills one level flat under
`.claude/skills/`; a skill directory nested a level deeper is invisible to
it, not merely inconsistent with `0003`'s convention.

### Rewrite the frontmatter `name:` at install time (chosen)

`0003` dismissed this exact move for canonical skills: comparing already-
written bytes against the raw embedded bytes breaks `decide()`'s invariant
(`target == embedded` means unchanged) once the two differ by construction.
That objection doesn't transfer here, because the comparison target moves
with the rewrite — `place()` is handed the already-rewritten contents
(`mattpocock-tdd`), so `target bytes == rewritten bytes` still means
unchanged; nothing is compared against the untouched upstream bytes at any
point. Rewriting happens once, in memory, before the conflict check, not
after it.

`rewrite_skill_name` in `init.rs` requires an exact `name: <dir-name>` line
to already be present and errors instead of guessing when it isn't — an
upstream skill whose frontmatter disagrees with its own directory name is a
vendor-side problem to surface, not paper over.

## Decision

`gk init --experimental=<package>` installs every skill under
`experimental/<package>/skills/**` (recursively, skipping non-skill
directories like each category's `README.md`) as
`.claude/skills/<package>-<skill-name>/`, with the frontmatter `name:` line
rewritten to match. `--claude` and `--experimental` share one `Report`, one
`--force`, one conflict model — installing from two sources in the same run
is one `init` call, not two.

## Consequences

An experimental skill can never collide with a canonical `gist-` skill or,
short of two packages sharing both a package and skill name, with another
experimental one. A vendor update that renames or restructures a skill
directory changes the installed name on the next `init`, same as any other
vendor update changing installed content — expected, not a special case.
Picking a package that doesn't exist, or has no `skills/` tree, fails with
the list of packages that do, per the "never guessed" rule in `CLAUDE.md`.

## Verification

`init_experimental_installs_a_vendored_package_prefixed`,
`init_experimental_is_idempotent_on_a_second_run`,
`init_experimental_unknown_package_names_what_is_available`,
`init_claude_and_experimental_together_install_both`, and
`init_experimental_repeated_flag_installs_the_package_once` (all in
`tests/cli.rs`). `rewrite_skill_name` has unit tests in `init.rs`: it replaces
only the frontmatter line, keeps a missing trailing newline missing, and errors
rather than guessing on an absent line or non-UTF-8 input.
