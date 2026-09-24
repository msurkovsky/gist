# Gist — working notes for agents

Skills are the product. Tools exist to serve them.

## Layout

- `skills/<name>/SKILL.md` — one skill per directory, frontmatter has `name` and
  `description`. Put anything long in `references/` so it loads on demand.
  `name` is prefixed `gist-` (`gist-outline`, not `outline`) — `gk init --claude`
  vendors these into a flat `.claude/skills/`, where a generic name silently
  collides with a target repo's own skill of the same name. See
  `docs/adr/0003-prefix-vendored-skill-names.md`.
- `tools/` — cargo workspace. `gist-core` holds shared output plumbing,
  `gist-cli` builds the `gk` binary.
- `docs/tool-contract.md` — read before adding a subcommand.
- `docs/adr/` — one file per decision worth not re-litigating. Add one when
  the rejected options were real.

## Language

Rust for anything a skill depends on. The compiler is the reviewer, and startup
cost matters when a tool is called every turn.

Python is allowed for throwaway glue only: single file under a skill's
`scripts/`, PEP 723 inline dependencies, run with `uv run`. No venvs, no
`requirements.txt`. When a skill starts depending on a script, port it to Rust.

## Commands

```bash
just ci        # fmt check, clippy, test — run before committing
just build
just install   # put gk on PATH
```

Without `just`: `cargo <cmd> --manifest-path tools/Cargo.toml`.

## Conventions

- Tools never prompt, never spin, never emit unbounded output.
- Every output type implements `Human` and `Serialize`. No exceptions.
- Test the failure path. The happy path is the easy half.

<!-- Imported from my-skills. Consolidated with the sections above in a follow-up commit. -->

# Working in this repository

Read `README.md` for the layout and lifecycle, `CONTRIBUTING.md` for the recipe for each
kind of change. Run `tools/check.sh` before every commit; it enforces the rules below and
exits non-zero on a violation. Rules that are easy to break:

- Never edit anything under `experimental/`. It is vendored upstream history. To change a
  vendored skill, copy it into `skills/<name>/` first, note the source in its frontmatter,
  then edit the copy. `tools/vendor.sh check` finds violations.
- Never put language-specific commands into a skill. They go into
  `languages/<lang>/toolchain.md`. A skill that needs them calls the `toolchain` skill.
  Missing language or verb is reported as `unsupported`, never guessed.
- New language: copy `languages/_template/` to `languages/<lang>/` and fill every verb or
  mark it `none`. No skill changes are needed.
- Vendors are added and updated only through `tools/vendor.sh`. Do not `git merge` upstream
  by hand and do not use `git subtree` or submodules.
- User-invoked skills carry `disable-model-invocation: true` and a human-facing one-line
  description. Model-invoked skills carry trigger phrasing in the description.
- Skills reference other skills by telling the agent to call the Skill tool with the name,
  not by relative file links across skill folders.
- `views/<consumer>/workspace.josh` must be committed here before the consumer runs
  `josh clone`. Cloning a path that does not exist yet fails.
- Commit subjects are plain imperative sentences. No conventional-commit prefixes in this repo.
