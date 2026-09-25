# 6. Add `--codex` as a second `init` target, same skill bytes

Accepted — 2026-09-24.

## Context

`gk init --claude` vendors `skills/` (and, per `docs/adr/0005`,
`--experimental=<package>` vendors `experimental/<package>/skills/`) into a
target repo's `.claude/skills/`, where Claude Code discovers them.

Codex CLI has its own skill discovery, per official docs
(`developers.openai.com/codex/skills`, redirecting to
`learn.chatgpt.com/docs/build-skills`; referenced from
`github.com/openai/codex/blob/main/docs/skills.md`): a repo-local
`.agents/skills/` (scanned upward from the working directory to the repo
root) or a user-global `$HOME/.agents/skills/`. The `SKILL.md` format itself
— YAML frontmatter with `name:`/`description:`, a Markdown body — is the same
"open agent skills standard" both agents read. Shared syntax does not guarantee
equivalent invocation policy or available tools: owned skill authoring follows
`docs/skill-contract.md`, with matching explicit-only policy in both frontmatter
and `agents/openai.yaml`. `experimental/mattpocock/skills/**` demonstrates this: its
skill directories carry a Codex-only optional sidecar,
`agents/openai.yaml` (display name, icon, invocation policy), sitting next to
the exact same `SKILL.md` Claude Code reads, copied through untouched by
`scripts/vendor.sh` today.

Two of Codex's rules are looser than the ones this repo already enforces for
Claude: Codex does not require a skill's directory name to match its
frontmatter `name`, and Codex tolerates two skills sharing a name — both are
listed, unmerged, no error.

Want: one `init` call can target Claude, Codex, or both, off the same skill
sources, with no Codex-specific rewrite of content — the same bytes land in
both roots.

## Options

### One shared root both agents read (e.g. symlink `.agents/skills` → `.claude/skills`)

Dismissed. Neither agent's discovery path is configurable — Claude Code
hardcodes `.claude/skills/`, Codex hardcodes `.agents/skills/` (or
`$HOME/.agents/skills/`). A symlink works on Linux/macOS but is fragile
across checkouts (Windows checkouts and zip downloads drop symlinks; git's
`core.symlinks` varies by clone), and `decide()`'s conflict check would need
to special-case a target that is actually two paths aliasing one inode.
Materializing the same bytes twice, the way `--claude` and `--experimental`
already do independently, is simpler and adds no shared state to reason
about.

### Vendor-specific content per target (e.g. generate `agents/openai.yaml` for every skill)

Dismissed for now. Nothing in canonical `skills/` needs it today, and the
ask was zero or near-zero modification. `experimental/mattpocock/skills/**`
already carries its own sidecar where relevant, copied through unmodified —
that keeps working unchanged under `--codex`. Generating a sidecar for
skills that lack one is a feature for later, not part of this decision.

### `--codex` as a parallel flag, same content, different root (chosen)

Add `.agents/skills` as a second selectable root next to `.claude/skills`.
`--claude`, `--codex`, and `--experimental=<package>` become independently
selectable and combinable in one `init` call; whichever roots are selected
each receive the identical computed bytes — canonical `skills/*` unmodified,
experimental packages prefixed exactly as `docs/adr/0005` already describes,
regardless of which root they land in. No new rewrite step is added for
Codex; `rewrite_skill_name` stays exactly what it is today, a
collision-avoidance move that happens to also satisfy Codex's laxer rules
rather than needing a Codex-specific reason to exist.

## Decision

`gk init` gains `--codex`, parallel to `--claude`. Any combination of
`--claude`, `--codex`, `--experimental=<package>` may be passed in one call,
but at least one of `--claude`/`--codex` is required: `--experimental` says
what to install, not where, and with two targets there is no default to fall
back on, so it alone is refused like no flags at all. For each
selected target root — `.claude/skills` for `--claude`, `.agents/skills` for
`--codex` — the same computed bytes are placed: canonical skills unmodified,
experimental packages name-prefixed once per package, not once per target.
`Report` moves from one target to a list of per-target reports (root, totals,
skills), since a single run can now touch more than one directory tree;
`Report::has_conflicts` aggregates across all of them for the exit code.

## Consequences

Skills land on disk twice when both flags are passed —
`.claude/skills/gist-outline/` and `.agents/skills/gist-outline/` both get
the full file set. Accepted: it matches how each agent actually discovers
skills, and avoids a cross-linked target that complicates the conflict
model for no real saving.

Codex's looser rules (directory name need not match `name:`, duplicate names
allowed) are not relied on. This repo keeps its own stricter `gist-`/
`<package>-` prefixing for both targets, so a project running both agents
sees the same name set either way — `--codex` does not loosen anything
`--claude` already enforces.

A vendored skill's Codex-only `agents/openai.yaml`, where one exists, is
copied through unmodified to both roots. Claude Code ignores files it
doesn't recognize inside a skill directory, so its presence under
`.claude/skills/` is inert, not wrong.

A third agent later (or a fourth) is the same move again — one more flag,
one more root, reusing every existing content-computation path. Nothing
proposed here is Codex-specific beyond the root path itself.

## Verification

`tests/cli.rs`, mirroring the `--claude`/`--experimental` tests:
`init_codex_installs_into_dot_agents_skills`,
`init_claude_and_codex_together_install_identical_content_into_both`,
`init_codex_experimental_reuses_the_prefix_rewrite`,
`init_codex_is_idempotent_on_a_second_run`,
`init_without_a_target_is_a_refusal_not_misuse`, and
`init_experimental_alone_is_still_a_refusal_no_implicit_target`.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-25 21:50 | Martin Surkovsky | Attribute changes to the accountable human author |
| 2026-09-25 21:15 | Martin Surkovsky | Distinguish shared file installation from host behavior and invocation policy |
| 2026-09-24 23:05 | Martin Surkovsky | Corrected the flag rule, since `--experimental` alone is refused, and named the tests that exist |
| 2026-09-24 18:51 | Martin Surkovsky | Created |
