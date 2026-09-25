# 3. Prefix vendored skill names

Accepted — 2026-09-24. The prefix is enforced now: `scripts/check.sh` fails on a
skill directory without it, and `CLAUDE.md` says so.

## Context

`gk init --claude` vendors this repo's skills into a target repo's flat
`.claude/skills/<name>/`. `outline` and `doc-review` are exactly the kind of
generic one-word names a target repo is likely to already have or add later.
Two skills landing in that flat directory with the same name collide
silently: Claude Code's precedence picks one location, the other never
triggers, and nothing errors — there is no equivalent of the
`plugin-name:skill-name` display prefix a plugin loader adds for you; a plain
project skill vendored straight into `.claude/skills/` has no such layer.

Checked Claude Code's docs rather than assuming: a skill's frontmatter
`name:` accepts lowercase letters, digits, and hyphens only (max 64 chars, no
XML tags, `anthropic`/`claude` reserved) — no colon, no slash.

## Options

### Leave names as-is

Dismissed. Silent shadowing risk in the common case, not the edge case —
`outline` and `doc-review` are the names most likely to already be taken.

### A colon-style namespace (`gist:outline`)

Dismissed. Illegal in the `name:` field per the actual frontmatter rules, not
just nonstandard — colons and slashes are not in the allowed character set.

### Rewrite the name at install time, keep canonical files bare

Dismissed. Makes the embedded bytes and the written bytes differ for the same
file, which breaks the conflict detector's core assumption (`target bytes ==
embedded bytes` means unchanged) for every single comparison, to solve a
problem a plain rename solves for free.

### Rename at the source (chosen)

`skills/outline` → `skills/gist-outline`, `skills/doc-review` →
`skills/gist-doc-review`, frontmatter `name:` updated to match — Claude Code
requires the directory name and the frontmatter `name:` to agree. This is the
convention already visible among this session's own skills
(`plan-ceo-review`, `ios-clean`, etc.).

## Decision

Rename the canonical skills at the source, prefix baked into the name itself.
Grepped the repo: the only other match for either bare name was the
unrelated `gk outline` CLI subcommand, so the rename is isolated.

## Consequences

This repo's own skills are now named with the `gist-` prefix everywhere,
including locally. The prefix is convention, not enforced — a future skill
added without it would still pass the name/directory-match test, so it is
worth a mention in `CLAUDE.md`. This narrows collisions, it does not
eliminate them — a target repo could still have its own `gist-outline`; that
is inherent to Claude Code's flat project-skill discovery, not something `gk`
can fix.

## Verification

`the_embedded_skills_are_present_and_named_with_the_gist_prefix` and
`every_embedded_skill_declares_a_frontmatter_name_matching_its_directory`
(both in `init.rs`).
