# 9. Install git hooks with `gk hook install`

Accepted — 2026-09-25.

## Context

The commit-msg hook is installed by symlink:
`ln -sf <gist checkout>/hooks/commit-msg.sh .git/hooks/commit-msg`. That line is
in `README.md`, `CONTRIBUTING.md`, and `CLAUDE.md`.
Four things are wrong with it.

- It needs a gist checkout at a known path on the machine, the same premise
  `docs/adr/0002` rejected for `init`.
- It fails silently. Reproduced in a scratch repo: with the link dangling,
  `git commit -m 'lowercase with period.'` succeeds with no output. Git treats
  a hook that does not exist as absent and only warns about one that exists
  but is not executable. Move or delete the checkout and enforcement is off in
  every repo installed that way, with nothing to say so.
- There is no uninstall and no way to ask what is installed.
- `hooks/commit-msg.sh` is 60 lines with loops and branching, past the
  one-liner limit in `CLAUDE.md`. It already cost a portability fix and an open
  macOS check for `len()` (BSD `tr` and `wc`), both in `TODO.md`.

`CLAUDE.md` keeps `hooks/` in shell because "target repos install by symlink
without `gk` on PATH". Once installing is a `gk` command that premise is false
at install time. What is left is run time. `.git/hooks` is per clone and never
cloned, so a hook runs only on a machine where someone installed it on purpose,
and that machine had to get `gk` first.

## Options

### Keep the symlink and document it better

Dismissed. The silent failure is the problem, and documentation does not make a
dangling link say anything.

### `gk hook install` copies the embedded `commit-msg.sh`

Dismissed. It fixes the checkout dependency and gives uninstall a target, and
the hook would run without `gk` on PATH, which is a real advantage. But the 60
lines of bash stay, with the bash 3.2 and BSD tool constraints, and a copy goes
stale when `gk` is upgraded, so behaviour depends on when each repo last ran
install. This is the option to come back to if a missing `gk` at commit time
turns out to bite often.

### Port the logic to `gk hook commit-msg`, install a shim with an absolute path to `gk`

Dismissed. It survives a GUI git client whose PATH lacks `~/.cargo/bin`, but the
path is baked into a file the user may keep in dotfiles, and it breaks when the
binary moves. Both variants fail loudly. Re-running install is the same fix for
either, and a path-free file has nothing machine-specific to go stale.

### Read config and find the repository through libgit2

Dismissed after review of the first implementation, which did this. libgit2
does not read `GIT_CONFIG_PARAMETERS` or `GIT_CONFIG_COUNT`, so
`git -c commit.subjectMax=10 commit` reached the hook with the default limit.
`Repository::discover` ignores `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_NOSYSTEM`, so
install could read a different `core.hooksPath` than git did. And libgit2 1.9
cannot open a sha256 repository that git commits in without complaint. Each
fails quietly or with a misleading message. The hook only ever runs under git,
so `git` is on PATH by construction, and asking it costs a few milliseconds a
commit, about what the shell hook spent on the same questions.

### Port the logic to `gk hook commit-msg`, install a generated shim that finds `gk` on PATH (chosen)

The rules live once, in Rust, tested end to end. The file in `.git/hooks` is a
few lines of generated POSIX `sh` that calls `gk`.

## Decision

**`gk hook commit-msg <file>`** is the hook, ported rule for rule from the shell
version: subject and body checks, `commit.subjectMax`, `commit.bodyMax`,
`mr.commitPattern`, exempt Merge/fixup!/squash!/Revert. One deliberate
difference: "uppercase" means Unicode uppercase, so `Šablona` passes. The shell's
`[A-Z]` meant whatever the locale's collation said. It reads config by running
`git config`, not libgit2 (see Options). Exit 0 accepts, exit 1 rejects with the rule on stderr. Advice on an
accepted message, such as the subject-length note, also goes to stderr, as it did
in the shell version, and stdout stays empty. It is a public subcommand, so a
hook manager can call it directly.

**`gk hook install [--force] [--uninstall]`** finds the repository from the
current directory with `git rev-parse` and writes one shim per git hook `gk` provides, today only
`commit-msg`. Flags mirror `init`. The shim is written by the binary, not
embedded from `hooks/`:

```sh
#!/bin/sh
# gk-hook: commit-msg
command -v gk >/dev/null 2>&1 || { echo "commit-msg: gk is not on PATH; install gk, or delete this hook ($0)" >&2; exit 1; }
exec gk hook commit-msg "$@"
```

A missing `gk` blocks the commit and says why. Exiting 0 there would recreate
the silent failure this ADR exists to remove.

**Hooks directory** is the common dir's `hooks/`, so linked worktrees share it as
git does. If `core.hooksPath` is set at any scope, install refuses, exits 1, and
names the value. That path is usually shared by many repos or owned by husky,
lefthook, or pre-commit. Writing there changes repos the user did not name or
fights the manager. No override for now. `--uninstall` ignores the setting and
cleans the default directory: it only removes a file carrying gk's marker, and a
hook installed before the setting existed should stay removable.

**Ownership is a marker, not a manifest.** ADR 0007 needs a manifest because a
skill tree is many files whose content changes with the binary. A shim is one
small template. A file whose second line is `# gk-hook: <name>` is ours:
install rewrites it, `--uninstall` removes it. Anything else at that path (a
user's own hook, another tool's, the old symlink) is a conflict: reported,
untouched, exit 1. `--force` replaces it on install. `--uninstall` removes only
marked files and reports the rest as kept; `--force` with `--uninstall` is a
usage error. Edits made under the marker are overwritten by the next install and
removed by `--uninstall`; delete the marker line to keep a customised hook, and
install then reports a conflict. Identical bytes that lost the executable bit are
not "unchanged": git skips such a hook without a word, so install rewrites it and
reports it overwritten.

**Symlinks, per ADR 0008.** The hooks directory may itself be a symlink; the
user chose it. The hook file being a symlink is a conflict, and `--force`
replaces the link entry. It departs from 0008's "`--force` must not override a
symlink" on purpose. That rule stops a write landing in the link's target. Here
the old install is a link, replacing it is the migration, and the write is a
temporary file created with `create_new` and renamed over the entry, which
replaces the link and never follows it. Mode is 0755. The write is the same
`write_atomic` routine the manifest uses, not a second copy of it.

**`hooks/commit-msg.sh` becomes a two-line forwarder**, `exec gk hook commit-msg
"$@"`, and is not deleted. Deleting it leaves every existing symlink dangling,
and by the reproduction above that turns enforcement off without a word. With
the forwarder those repos keep working, and fail loudly if `gk` is absent.
`TODO.md` tracks removing it once they have re-run `gk hook install --force`.

**Out of scope.** Claude Code hooks need a `settings.json` edit, which `init`
does not do and this does not either.

## Consequences

`gk` must be on PATH at commit time. In this repo `just install` provides it. A
GUI client with a narrow PATH gets the error above, not a silent pass. `git` 2.31
or newer is needed, for `rev-parse --path-format`.

The gist repo commits through the hook it is changing, so a bug in
`gk hook commit-msg` blocks commits here until fixed. `git commit --no-verify`
or `gk hook install --uninstall` gets out. Rule changes reach a repo only after
`gk` is reinstalled, as with ADR 0002, but they now reach every repo at once.

`mr.commitPattern` is now a Rust `regex`, not POSIX ERE, and `regex` becomes a
dependency. Two differences matter. Alternation is leftmost-first where ERE is
leftmost-longest: with `^(feat|feature)` the type captured from `feature: x` is
`feat` here and was `feature`, so write the longer alternative first. And
backreferences do not compile. A typical typed pattern,
`^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+`, behaves the same in both. A pattern
that does not compile fails the hook with the compiler's message, not silently.

The bash-only property of `hooks/` goes. `CLAUDE.md` says hooks target repos
without `gk` on PATH and needs rewording. `README.md`, `CONTRIBUTING.md`, and
`CLAUDE.md` swap the `ln -sf` line for `gk hook install`. The commit-msg smoke lines in `scripts/check.sh` move to
`tests/cli.rs`. The macOS `len()` item in `TODO.md` goes away, since Rust counts
characters. Windows was not considered; mode bits are ignored there and Git for
Windows ships `sh`, so the shim probably works.

## Verification

`tests/cli.rs`, end to end against a scratch repo with the user's global and
system git config disabled. Written first against a stub that accepted every
message and installed nothing: 27 of the 33 then written failed, and the other
six, the ones that check nothing valid is rejected, passed as expected.

- The ported rules, one test each: accepts the generic style and prints nothing;
  rejects a lowercase subject, a trailing period, an empty subject, a missing
  blank second line, a 73-character subject, a 73-character body line; accepts a
  non-ASCII capital; ignores comment lines; exempts long URL, trailer and
  indented lines, and Merge/fixup!/squash!/Revert; counts characters, not
  bytes; notes a subject over 50; honours `commit.subjectMax` and
  `commit.bodyMax`; enforces `mr.commitPattern`; fails loudly on a limit that
  is not a number, on a pattern that does not compile, and on a message file it cannot read; speaks the JSON
  envelope.
- Config is read the way git reads it: `git -c commit.subjectMax=10 commit`
  reaches the hook; a `GIT_CONFIG_GLOBAL` file that sets `core.hooksPath` makes
  install refuse; a `~/.gitconfig` that git is told to ignore does not; a sha256
  repository installs and checks messages. Advice goes to stderr and stdout stays
  empty.
- `install_writes_an_executable_shim_that_git_runs` runs a real `git commit`
  through the shim: bad subject rejected, good one accepted.
- Install: second run unchanged; a foreign hook is a conflict and survives until
  `--force`; its own stale shim is rewritten without `--force`; a symlink is a
  conflict, and `--force` replaces the link without touching its target;
  `core.hooksPath` refuses and writes nothing; a linked worktree installs into
  the shared directory; outside a repository fails; a directory where the hook
  goes is reported, not clobbered.
- Uninstall: removes a shim, reports a missing one, keeps a foreign file with
  exit 1, still works with `core.hooksPath` set; `--force` with `--uninstall` is
  exit 2.
- The shim exits 1 naming the fix when `gk` is off PATH, and
  `hooks/commit-msg.sh` enforces only through `gk`.

A review of the first implementation found the config-resolution, sha256,
stdout-advice and lost-executable-bit problems above; each got a test that failed
before its fix. Every `Repo` in the test file now runs git and `gk` with the
user's global and system config disabled, so a developer's `init.templateDir` or
`core.hooksPath` cannot change a result. `main.rs` has a unit test that every name
in `HOOKS` has a `gk hook <name>` subcommand, since a shim for a name that does
not exist would block every commit.

`install_replaces_a_symlink_only_when_forced_and_never_writes_through_it` was
also checked by mutation: with the atomic write swapped for a plain `fs::write`
it fails. The `could not inspect` and `could not remove` error arms have no
test, for the reason ADR 0008 gives: they need an unreadable directory.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-09-25 18:53 | Martin Surkovsky | Removed `mrType` and `gist-mr-start`, which ADR 0011 dropped, so the ADR matches the hook |
| 2026-09-25 06:51 | Martin Surkovsky | Created |
