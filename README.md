# Gist

*Understand enough to steer.*

AI writes the code. You still own the call.

Gist is a set of skills and tooling for developers working this way — where the
model is the doer and the human stays at the altitude where judgment still
matters. Not line-by-line review. Enough understanding to participate: to scope
the work, read the shape of what came back, catch the wrong turn, and decide.

## Layout

```
skills/         the product — one directory per skill, embedded into gk
experimental/   third-party skill repos vendored with full history, never edited here
languages/      everything language-bound, one adapter per language; skills stay agnostic
rules/          always-on constraints to drop into a project's CLAUDE.md or .claude/rules/
hooks/          deterministic guards: a Claude Code hook, and a forwarder for the old commit-msg link
views/          Josh workspaces — what a consumer sees of this repo
tools/          cargo workspace; builds the gk binary
scripts/        repo glue: vendoring, local symlinks, the consistency check
docs/           tool contract, and decisions in docs/adr
TODO.md         what comes next
```

`CONTRIBUTING.md` has a recipe per kind of change, `CLAUDE.md` the rules agents must
not break, `just ci` the enforcement of both.

## Skills

| skill | for |
|---|---|
| `gist-outline` | the shape of a codebase — file counts, dominant languages, where the weight sits. Orienting in an unfamiliar repo |
| `gist-doc-review` | the comments and doc strings in a change — cut what restates the code, shorten the bloated, add the missing line on public API |
| `gist-mr-start` | a branch about to be reviewed — ticket id, branch name, commit style, hook install, all settled before the first commit |
| `gist-mr-ready` | the gate before opening or updating an MR — checks and review per folder group, prove-red on new tests, numbers against the target branch, a description that carries only what the page cannot show |
| `gist-mr-reply` | review comments — live discussion, skip resolved threads, fix then reply, outcome first, no longer than the comment |
| `gist-mr-close` | closing or superseding an MR — backup ref and empty diff before any rewrite; replies, close, delete branch, in that order |
| `gist-toolchain` | install, typecheck, lint, test, coverage of changed lines, dead code, prove-red — without knowing the language. Commands come only from `languages/`; anything else is `unsupported` |

The four `gist-mr-*` skills are slash commands only: once vendored with `gk init --claude`,
invoke them in Claude Code as `/gist-mr-start`, and so on. `gist-outline` and
`gist-doc-review` also run when the model sees a request that matches their description, and
`gist-toolchain` is called by the other skills and by the model whenever a task needs
language tooling.

The `gist-mr-*` family is distilled from review lessons across several repositories. It
reads the project's own conventions first and falls back to a generic style: short
capitalised subject, no trailing period, a body that says why. Typed conventions such as
`feat(ID):` are opt-in per project, see Hooks.

## Build

```bash
just ci        # fmt, clippy, test, scripts/check.sh
just install   # put gk on PATH
```

## Use

```bash
gk outline .                       # what shape is this repo
gk doc                             # how much of my change is documentation
gk init --claude                   # vendor the skills into ./.claude/skills/ of the repo you are in
gk init --codex                    # same skills, byte-identical, into ./.agents/skills/ for Codex CLI
gk init --claude --codex --experimental=mattpocock  # any combination, one call
gk init --claude --uninstall       # remove what a prior init --claude put there
gk hook install                    # put the commit-msg hook into the repo you are in
```

`init` is idempotent and never overwrites a file you edited — it reports a
conflict instead; `--force` overrides. `--claude` and `--codex` are two
independent targets, combinable in one call; either or both is required, and
picking neither is a refusal, not a guess (see
`docs/adr/0006-codex-as-a-second-init-target.md`). Skills land under their
`gist-` names so they cannot shadow one the target repo already has;
`--experimental` skills land under `<package>-` names for the same reason,
regardless of which target(s) they're installed into.

`init` writes a manifest (`<target>/.gist-manifest.json`) recording the files it
placed and any it found already identical, also when a run fails partway. A run
refused before it starts, over a corrupt manifest or a symlink, writes none.
`--uninstall` reads that manifest and removes what it
recorded — files edited since install are left in place and reported, again
overridable with `--force` — instead of re-deriving a removal list from
whichever skills the running binary currently embeds (see
`docs/adr/0007-manifest-driven-uninstall.md`). `init` and `--uninstall` refuse
to read or write through a symlink under the target, such as one
`scripts/link.sh` made; remove the link first. That includes `.claude`,
`.claude/skills` and `.agents/skills` themselves, since a repo can commit a link
there; only the working directory may be one. Every root is checked before any
is written or emptied. A manifest is treated as untrusted input, since a repo can
carry one (see `docs/adr/0008-untrusted-manifests-and-symlinks.md` and
`docs/adr/0010-refuse-a-symlinked-root-and-check-every-root-first.md`).

## Composition

Third-party skill repositories are vendored with full history into
`experimental/<vendor>/`, tried, and adopted by copying into `skills/` where the copy is
adapted freely. No submodules, no subtrees. The history machinery is
[Josh](https://github.com/josh-project/josh), a reversible git history filter: an import
is a `:prefix=` filter over the upstream branch, a projection of this repo is a workspace,
and both directions round-trip hash for hash. `docs/adr/0004-compose-via-josh.md` has the
reasoning.

```bash
just vendor add <name> <url> [branch]   # first import, full history
just vendor update                      # re-import; clean merge, no duplicate commits
just vendor list                        # registry with the imported upstream SHA
```

Josh is two binaries, `josh` and `josh-filter`, built from source:

```bash
git clone https://github.com/josh-project/josh.git
cd josh
cargo build --release -p josh-cli --bin josh --bin josh-filter
cp target/release/josh target/release/josh-filter ~/.cargo/bin/
```

Only importing a vendor or cloning a view needs them. The filter cache lives in
`.git/josh/cache`; deleting it costs a recompute.

### Views

A view is `views/<consumer>/workspace.josh`. Each line maps a path in the view to a path
here, so vendored and owned skills appear side by side under flat names:

```
tdd = :/experimental/mattpocock/skills/engineering/tdd
gist-mr-reply = :/skills/gist-mr-reply
languages = :/languages
```

Commit the file here first, then clone the projection where the consumer expects skills:

```bash
josh clone <path-or-url-of-this-repo> ':workspace=views/<consumer>' <destination>
```

Inside the projection: `git commit` as usual, `josh push` to write back here, `josh changes
pull` to take this repo's changes. A view is still the only way to work on an
`experimental/` tree's own history; `gk init --experimental=<package>` gets a project its
skills without one, name-prefixed by package (see
`docs/adr/0005-prefix-experimental-skills-at-install.md`). Edits to vendored paths made
through a view violate the no-edit rule; adopt first.

## Languages

Skills never contain language commands. `gist-toolchain` resolves
`languages/<lang>/toolchain.md` per folder group, so a mixed repository runs each language
against its own folders, and answers `unsupported` when the language or the verb is
missing. Nothing is pre-seeded; a language is added when a project needs it, by copying
`languages/_template/`. `gk init --claude` does not vendor `languages/` yet (see
`TODO.md`); until then adapters live in `~/.claude/languages/` or the project's
`.claude/languages/`.

## Hooks

- `gk hook commit-msg`: git commit-msg hook. Short subject (72 max, note above 50),
  uppercase start, no trailing period, blank line, body wrapped at 72. A project with a
  typed convention opts in with `git config mr.commitPattern '<regex>'` (a Rust regex),
  and pins one type per branch with `git config branch.<name>.mrType`. Install per repo
  with `gk hook install`, which needs `gk` on PATH; `gk hook install --uninstall` removes
  it. It refuses when `core.hooksPath` is set. A repo that symlinked the old
  `hooks/commit-msg.sh` runs `gk hook install --force` once to replace the link.
- `hooks/post-rebase-nag.sh`: Claude Code `PostToolUse` hook on `Bash`. After a rebase,
  prints the re-install, regenerate, retest reminder. Wire it in `settings.json`; the
  snippet is in the file header.

## Vendors

Registry in `scripts/vendors.conf`. Current: `mattpocock` from
https://github.com/mattpocock/skills — promoted skills under
`experimental/mattpocock/skills/{engineering,productivity}`, betas under `in-progress/`.
Its `/setup-matt-pocock-skills` runs in a consuming project, not here.

## Status

Early. Seven skills, four tools, one vendored upstream, and a contract to keep the rest
honest.
