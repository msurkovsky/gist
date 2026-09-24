# TODO

## Next: `gk init --claude --uninstall`

Plan written, not yet approved for implementation:
`.todo/gk-init-manifest-and-uninstall.md`

Summary:
- `init --claude` writes a manifest (`.claude/skills/.gist-manifest.json`,
  path + sha256 per file) so uninstall knows exactly what it installed,
  independent of whatever the running binary's embedded skills currently are
- New `--uninstall` flag on `init`: removes manifest-recorded files that are
  unmodified, leaves modified ones and reports them (reuses `--force` to
  override), prunes emptied directories, rewrites/deletes the manifest
- New `sha2` dependency; new ADR `0004-manifest-driven-uninstall.md`
- 7 new e2e tests in `tools/crates/gist-cli/tests/cli.rs`, plus unit tests
  for the manifest-decision logic — full list in the plan

Pick up here: read the plan file, confirm still wanted, implement (TDD).

## Later: `gk init --claude` vendors more than skills

- `languages/` adapters and `hooks/` are not embedded, so a target repo gets
  `gist-toolchain` with every verb `unsupported` until `~/.claude/languages/` or
  `.claude/languages/` exists, and installs the commit-msg hook by symlink to a
  gist checkout. Decide whether `init` should place them and whether a
  `gk hook install` subcommand replaces the symlink.
- First view: `views/claude/workspace.josh` for `~/.claude/skills` on this
  machine, then one per project that should receive the `gist-mr-*` family.
- Wire `hooks/post-rebase-nag.sh` into `~/.claude/settings.json`.

