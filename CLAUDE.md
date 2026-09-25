# Gist — working notes for agents

Skills are the product. Tools exist to serve them. `MANIFESTO.md` governs this
repo; read `docs/architecture.md` and the workflow in `CONTRIBUTING.md` before
implementation. Apply `rules/code-and-comments.md` to code changes and
`rules/merge-requests.md` to merge requests. `just ci` checks mechanical
properties; human approval and behavioral evaluation are separate evidence.

## Layout

- `skills/<name>/SKILL.md` — one skill per directory, frontmatter has `name` and
  `description`. Put anything long in `references/` so it loads on demand.
  `name` is prefixed `gist-` (`gist-outline`, not `outline`) — `gk init --claude`
  vendors these into a flat `.claude/skills/`, where a generic name silently
  collides with a target repo's own skill of the same name. See
  `docs/adr/0003-prefix-vendored-skill-names.md`; `scripts/check.sh` fails on a
  bare name. `docs/skill-contract.md` defines acceptance and host compatibility.
  Explicit-only skills carry both `disable-model-invocation: true` and
  `policy.allow_implicit_invocation: false` in `agents/openai.yaml`.
  Model-invoked skills carry trigger phrasing.
- `experimental/<vendor>/` — third-party repos vendored with full history. Never
  edit anything here. To change a vendored skill, copy it into
  `skills/gist-<name>/`, note the source in its frontmatter, then edit the copy.
  Vendors are added and updated only through `just vendor`; no `git subtree`, no
  submodules, no merging upstream by hand. See `docs/adr/0004-compose-via-josh.md`.
- `rules/` — always-on constraints, language-agnostic. A rule that holds for one
  project only belongs in that project, not here.
- `hooks/` — deterministic guards. Prefer a hook over a rule once the rule has
  been broken more than once. Git hooks are `gk hook <name>` subcommands and
  `gk hook install` puts them in a repo (`docs/adr/0009`); `hooks/` keeps only
  a forwarder for the old commit-msg symlink.
- `views/<consumer>/workspace.josh` — must be committed here before the consumer
  runs `josh clone`. Cloning a path that does not exist yet fails.
- `tools/` — cargo workspace. `gist-core` holds shared output plumbing,
  `gist-cli` builds the `gk` binary.
- `scripts/` — repo glue in shell, reached through `just` recipes.
- `docs/tool-contract.md` — read before adding a subcommand.
- `docs/adr/` — one file per decision worth not re-litigating. Add one when
  the rejected options were real. Keep it current with the code, and log each
  change in its `## Changelog`; `docs/adr/README.md` has the format.

## Language

Rust for anything a skill depends on at run time. The compiler is the reviewer,
and startup cost matters when a tool is called every turn.

A custom tool is a `gk` subcommand, Rust under `tools/`; read
`docs/tool-contract.md` first. Bash is fine only for a one-liner: one command
or pipeline, no branching, no loops, no argument parsing. Once a script grows
past that, port it to `gk` instead of growing the script. `scripts/` and
`hooks/` still carry longer shell from before this rule; `TODO.md` tracks it.

Shell is allowed in two places: `scripts/`, which only contributors run, and
`hooks/`, for a Claude Code hook wired into `settings.json`. Keep both
bash 3.2 compatible (macOS ships it): no `mapfile`, no `\|` in sed.

Python is allowed for throwaway glue only: single file under a skill's
`scripts/`, PEP 723 inline dependencies, run with `uv run`. No venvs, no
`requirements.txt`. When a skill starts depending on a script, port it to Rust.

## Commands

```bash
just ci        # fmt check, clippy, test, scripts/check.sh — run before committing
just build
just install   # put gk on PATH
just check     # skills, hooks, vendored tree only
just vendor add <name> <url> [branch]   # import a third-party skill repo
```

Without `just`: `cargo <cmd> --manifest-path tools/Cargo.toml`, `scripts/check.sh`.

## Conventions

- Tools never prompt, never spin, never emit unbounded output.
- Every output type implements `Human` and `Serialize`. No exceptions.
- Test the failure path. The happy path is the easy half.
- Skills name dependencies and ask the agent to use its host's skill mechanism.
  Do not assume a tool named `Skill`, or link across installed skill directories.
  See `docs/skill-contract.md`.
- Commit subjects are plain imperative sentences, capitalised, under 50
  characters where possible, 72 hard limit, no trailing period, no
  conventional-commit prefix. Blank line, then a body wrapped at 72 that says
  why. `gk hook commit-msg` enforces it; install once with
  `gk hook install`. Typed conventions
  like `feat(ID):` belong to individual projects, never here.
