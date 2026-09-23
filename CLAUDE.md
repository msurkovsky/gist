# Gist — working notes for agents

Skills are the product. Tools exist to serve them.

## Layout

- `skills/<name>/SKILL.md` — one skill per directory, frontmatter has `name` and
  `description`. Put anything long in `references/` so it loads on demand.
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
