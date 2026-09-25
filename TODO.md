# TODO

## Later: `gk init --claude` vendors more than skills

- Done: `gk hook install` and `gk hook commit-msg`, per
  `docs/adr/0009-install-git-hooks-with-gk.md`.
- Delete `hooks/commit-msg.sh`, now a forwarder that keeps old symlinks
  enforcing, once every repo installed by symlink has re-run
  `gk hook install --force`.
- `languages/` is not embedded, so a target repo gets `gist-toolchain` with
  every verb `unsupported` until `~/.claude/languages/` or `.claude/languages/`
  exists. Nothing to place yet: only `_template` exists. When the first real
  adapter lands, embed `languages/` and place it under `.claude/languages/`,
  which `gist-toolchain` already searches.
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
- `hooks/post-rebase-nag.sh` could become `gk hook post-rebase-nag`, which
  would also drop its `node` dependency. ADR 0009 left it out; wiring it into
  `settings.json` stays manual either way.

## Later: decisions left open by the init review

- `init --claude --codex --uninstall`: a refusal in one root leaves the other
  already removed, so ADR 0008's "a refusal removes nothing" holds per root only.
  Either reword it, or check every root before touching any, and pin the choice
  with a test.
- `init` output is unbounded: 202 lines / 20 KB with `--experimental=mattpocock`,
  and no `--limit`, against `docs/tool-contract.md`. Add `--limit` for the
  per-file listing (totals stay complete), or record the exception in the contract.
- `rules/code-and-comments.md` says a guard no test reaches is dead. I/O error
  arms (`could not inspect`, `could not read`) are reachable in production but not
  in tests. Decide on a carve-out; the rule is vendored into other projects.
- `gk doctor` (detect install drift, suggest fixes) was dropped: a second
  `gk init` already reports unchanged and conflict per file. Revisit with a real
  case.

## Later: verify on macOS

- CI is ubuntu-only, but `CLAUDE.md` promises bash 3.2 and the scripts lean on BSD
  tools. Run `just ci` on a Mac, in particular `scripts/vendor.sh update` (the
  empty-array merge path) and check 4 in `scripts/check.sh` (grep pattern). The
  fixes for those were only tested with GNU tools and bash 5. Then decide on a `macos-latest` CI job.

## Later: vendors that ship more than markdown

- `init` embeds all of `experimental/` (673 KB in 168 files, 265 KB of it under
  `*/skills/`) but installs only skills. Executables already break: 
  `block-dangerous-git.sh` is mode 775 under
  `experimental/mattpocock/skills/misc/git-guardrails-claude-code/scripts/` and
  664 once installed, because `place()` writes with `fs::write` and `include_dir`
  keeps no mode.
- Decide before a vendor ships a compiled binary, or needs files outside
  `skills/`: which files install, how modes survive, whether a binary is embedded
  per platform, downloaded, or left to the user, and how that fits the manifest
  hash and `--uninstall`. Probably an ADR. Consider embedding only `*/skills/`.

## Later: rough edges in the shell

- `scripts/vendor.sh` runs `git fetch -q "$url" "$branch"` without `--`, so a
  `vendors.conf` URL starting with `-` is read as a git option. Contributors only.
- `hooks/post-rebase-nag.sh` does not match `git -C <dir> rebase`.
