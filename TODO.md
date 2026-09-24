# TODO

## Later: `gk init --claude` vendors more than skills

- `languages/` adapters and `hooks/` are not embedded, so a target repo gets
  `gist-toolchain` with every verb `unsupported` until `~/.claude/languages/` or
  `.claude/languages/` exists, and installs the commit-msg hook by symlink to a
  gist checkout. Decide whether `init` should place them and whether a
  `gk hook install` subcommand replaces the symlink.
- Done: `gk init --claude --uninstall` (and `--codex --uninstall`) removes
  what a prior `init` run placed, per a `<root>/.gist-manifest.json` manifest
  each `init` run now writes; a locally modified file is left in place and
  reported unless `--force`. See
  `docs/adr/0007-manifest-driven-uninstall.md`.
- Done: `gk init --experimental=<package>` vendors an `experimental/<name>`
  tree's skills into `.claude/skills/<name>-<skill>/`, prefixed at install
  time so a bare upstream name (`tdd`, `retro`) can't collide. See
  `docs/adr/0005-prefix-experimental-skills-at-install.md`.
- Done: `gk init --codex` vendors the same skills, byte-identical, into
  `.agents/skills/` for Codex CLI. Combinable with `--claude` and
  `--experimental` in one call; at least one of `--claude`/`--codex` is
  required. See `docs/adr/0006-codex-as-a-second-init-target.md`.
- First view: `views/claude/workspace.josh` for `~/.claude/skills` on this
  machine, then one per project that should receive the `gist-mr-*` family.
- Wire `hooks/post-rebase-nag.sh` into `~/.claude/settings.json`.

## Later: port the shell under `scripts/` to `gk`

- `scripts/check.sh`, `scripts/vendor.sh`, and `scripts/link.sh` predate the
  rule that anything beyond a bash one-liner is a `gk` subcommand. Candidates:
  `gk check`, `gk vendor`, `gk link`; the `just` recipes keep their names.
- `hooks/*.sh` follow once `gk` is on PATH in target repos, which is the
  `gk hook install` question above.
