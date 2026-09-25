# TODO

## Later: `gk init --claude` vendors more than skills

- Done: `gk hook install` and `gk hook commit-msg`, per
  `docs/adr/0009-install-git-hooks-with-gk.md`.
- Delete `hooks/commit-msg.sh`, now a forwarder that keeps old symlinks
  enforcing, once every repo installed by symlink has re-run
  `gk hook install --force`.
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
  machine.

## Later: port the shell under `scripts/` to `gk`

- `scripts/check.sh`, `scripts/vendor.sh`, and `scripts/link.sh` predate the
  rule that anything beyond a bash one-liner is a `gk` subcommand. Candidates:
  `gk check`, `gk vendor`, `gk link`; the `just` recipes keep their names.

## Later: decisions left open by the init review

- `rules/code-and-comments.md` says a guard no test reaches is dead. I/O error
  arms (`could not inspect`, `could not read`) are reachable in production but not
  in tests. Decide on a carve-out; the rule is vendored into other projects.
- `gk doctor` (detect install drift, suggest fixes) was dropped: a second
  `gk init` already reports unchanged and conflict per file. Revisit with a real
  case.

## Later: verify on macOS

- CI is ubuntu-only, but `CLAUDE.md` promises bash 3.2 and the scripts lean on BSD
  tools. Run `just ci` on a Mac, in particular `scripts/vendor.sh update` (the
  empty-array merge path). The fix for it was only tested with GNU tools and
  bash 5. Then decide on a `macos-latest` CI job.

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

## Later: close the gaps between `MANIFESTO.md` and the repo

The manifesto describes the repo as it is meant to be. Each line here is a
place where the repo does not follow it yet.

- Principle 6 names the source altitudes. This repo has ADRs and the layout
  block in `README.md`; no conceptual diagram, no architecture page, no design
  notes per part. Write them as text in the tree, small enough that the top
  three share one page. Diagram notation undecided; it must diff.
- Principle 5 says test cases are prose the human approves and every test names
  its case. No test cases exist. `tests/cli.rs` names describe behaviour but
  link to nothing. Decide the format and the link, then backfill.
- Principle 3 says review is human-driven with a tool that answers "is this code
  aligned with that section" and "show me what changed in the docs". No such
  skill. The deterministic half — which prose sources which changed file, the
  test-to-case link, red-on-parent — is a `gk` subcommand; the judgement half
  is a skill.
- Principle 3 says a plan expires when its branch merges. `README.md` says plans
  live in `.todo/`; nothing is there. Decide where plans live while open and
  write the expiry into `CONTRIBUTING.md`.
- Principle 4 says low-confidence areas are declared per area so a hook can hint
  when a change touches one. Nothing declares them. Decide the format and what
  the hook emits. A hint, not a gate.
- Principle 8 says every decision has a pointer that reaches it. `docs/adr/0001`
  has none outside `docs/adr/`. Module docs in `init.rs` and `hook.rs` already
  point at their ADRs; name that convention and check it in `scripts/check.sh`
  or `gk`.
