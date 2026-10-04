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

`rewrite_skill_name` in `init.rs` parses closed YAML frontmatter and requires a
single top-level name line matching the directory (plain or quoted), and errors
instead of guessing when it isn't present — an
upstream skill whose frontmatter disagrees with its own directory name is a
vendor-side problem to surface, not paper over.

## Decision

`gk init --experimental=<package>` installs every skill under
`experimental/<package>/skills/**` (recursively, skipping non-skill
directories like each category's `README.md`) as
`.claude/skills/<package>-<skill-name>/`, with the frontmatter `name:` line
rewritten to match. Build a complete package name map before placing files and
refuse duplicate basenames. Rewrite references to those names in Markdown and
YAML resources: `/name`, `$name`, and quoted names in paragraphs referring to the
`Skill tool`. Preserve ordinary prose, local paths, unknown host commands, and
already-prefixed names. New reference conventions need an explicit packaging
change; unstructured prose is not treated as a dependency language. Upstream
source remains untouched. `--claude` and `--experimental` share one `Report`, one
`--force`, one conflict model — installing from two sources in the same run
is one `init` call, not two.

`--experimental` is for contributors trying a vendored skill, not a way to
ship one. Only a build with the `experimental` cargo feature embeds
`experimental/`: the default for a build from a checkout, so `just install`
and the tests have it. Release builds, made by `dist` with
`default-features = false`, refuse the flag and hide it from `--help`; users
install a third-party skill from its upstream. Shipping it would make Gist the
distributor of a frozen snapshot of someone else's work, with their notice to
carry.

Rewriting preserves original line endings and final-newline presence. LF and CRLF
blank lines, including whitespace-only lines, delimit the paragraphs used to scope
quoted references; a `Skill tool` mention does not affect other paragraphs.

## Consequences

An experimental skill can never collide with a canonical `gist-` skill or,
short of two packages sharing both a package and skill name, with another
experimental one. A vendor update that renames or restructures a skill
directory changes the installed name on the next `init`, same as any other
vendor update changing installed content — expected, not a special case.
Picking a package that doesn't exist, or has no `skills/` tree, fails with
the list of packages that do, per the "never guessed" rule in `CLAUDE.md`.

## Verification

`experimental_skill_dependencies_use_the_installed_namespace` verifies that
installed workflows name the dependencies actually installed for both hosts.
`docs/cases/packaging.md` specifies the reference boundary. Existing checks include
`init_experimental_installs_a_vendored_package_prefixed`,
`init_experimental_is_idempotent_on_a_second_run`,
`init_experimental_unknown_package_names_what_is_available`,
`init_claude_and_experimental_together_install_both`, and
`init_experimental_repeated_flag_installs_the_package_once` (all in
`tests/cli.rs`), and, without the feature, `init_experimental_is_refused_by_a_release_build`;
`just ci` runs both builds. `rewrite_skill_name` has unit tests in `init.rs`: it replaces
only the frontmatter line, keeps a missing trailing newline missing, and errors
rather than guessing on an absent line or non-UTF-8 input.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-10-04 21:23 | Martin Surkovsky | Release binaries leave `experimental/` out and refuse `--experimental`; vendored skills are for contributors to try, not to ship |
| 2026-09-25 21:50 | Martin Surkovsky | Attribute changes to the accountable human author |
| 2026-09-25 21:38 | Martin Surkovsky | Preserve CRLF content and paragraph boundaries during packaging |
| 2026-09-25 21:15 | Martin Surkovsky | Preserve dependency resolution when package skill names are prefixed |
| 2026-09-24 23:13 | Martin Surkovsky | Listed the tests added for behaviours that mutation testing showed nothing pinned |
| 2026-09-24 18:28 | Martin Surkovsky | Created |
