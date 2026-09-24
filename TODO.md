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
