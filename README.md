# Gist

*Understand enough to steer.*

AI writes the code. You still own the call.

Gist is a set of skills and tooling for developers working this way — where the
model is the doer and the human stays at the altitude where judgment still
matters. Not line-by-line review. Enough understanding to participate: to scope
the work, read the shape of what came back, catch the wrong turn, and decide.

## Layout

```
skills/     the product — one directory per skill, embedded into gk
tools/      cargo workspace; builds the gk binary
docs/       tool contract, and decisions in docs/adr
TODO.md     what comes next; the plans behind it live in .todo/
```

## Skills

| skill | for |
|---|---|
| `gist-outline` | the shape of a codebase — file counts, dominant languages, where the weight sits. Orienting in an unfamiliar repo |
| `gist-doc-review` | the comments and doc strings in a change — cut what restates the code, shorten the bloated, add the missing line on public API |

Once vendored with `gk init --claude`, invoke them in Claude Code as
`/gist-outline` and `/gist-doc-review`.

## Build

```bash
just ci        # fmt, clippy, test
just install   # put gk on PATH
```

## Use

```bash
gk outline .        # what shape is this repo
gk doc              # how much of my change is documentation
gk init --claude    # vendor the skills into ./.claude/skills/ of the repo you are in
```

`init` is idempotent and never overwrites a file you edited — it reports a
conflict instead; `--force` overrides. Skills land as `gist-outline` and
`gist-doc-review`, prefixed so they cannot shadow one the target repo already
has.

## Status

Early. Two skills, three tools, and a contract to keep the rest honest.

<!-- Imported from my-skills. Consolidated with the sections above in a follow-up commit. -->

# my-skills

One central repository for agent skills, rules, and hooks, composed from many third-party
repositories without submodules or subtrees. Third-party work is vendored with full history
into `experimental/`, tried, and then adopted into owned skills that are adapted freely.
Consumers (a machine's `~/.claude/skills`, a project's `.claude/`) get small projections of
this repo, never the whole thing.

The history machinery is [Josh](https://github.com/josh-project/josh): fast, reversible git
history filtering. Imports are `:prefix=` filters, projections are workspaces, and both
directions round-trip hash for hash.

## Layout

```
skills/<name>/            owned skills, language-agnostic, edited freely
experimental/<vendor>/    vendored upstreams, verbatim, never edited here
languages/<lang>/         everything language-bound; one adapter file per tool
languages/_template/      the adapter contract to copy for a new language
rules/                    always-on constraints to drop into a project's CLAUDE.md or .claude/rules/
hooks/                    deterministic guards: git hooks and Claude Code hooks
views/<consumer>/         workspace.josh files defining what a consumer sees
tools/                    vendor.sh (import/update), link.sh (quick local symlink), check.sh (pre-commit checks)
```

Contributing: `CONTRIBUTING.md` has a recipe per kind of change, `CLAUDE.md` the rules
agents must not break, `tools/check.sh` the mechanical enforcement of both.

## Lifecycle

1. **Include**: `tools/vendor.sh add <name> <url> [branch]` imports an upstream under
   `experimental/<name>` with full history. `tools/vendor.sh update` pulls upstream
   changes; repeated imports merge cleanly because the filter is deterministic.
2. **Try**: link a vendored skill for local use with `tools/link.sh experimental/<path> [as-name]`,
   or map it into a view.
3. **Adopt**: copy the skill into `skills/<name>/`, add a `source:` line to its frontmatter
   with the upstream path and commit, then edit. `experimental/` stays untouched;
   `tools/vendor.sh check` lists violations.

## Conventions

- **Skills are language-agnostic.** Anything language-bound lives under `languages/<lang>/`.
  A skill that needs language tooling asks the `toolchain` skill, which resolves
  `languages/<lang>/toolchain.md` and answers `unsupported` when the file or verb is missing.
  Adding a language is adding a file; no skill changes.
- **Detection is per folder group**, not per repository. Mixed repos run each language
  against its own folders.
- **Project specifics stay in the project.** Test file naming, commit types, base branch,
  package manager live in that project's `CLAUDE.md` or `.claude/rules/`. Skills read them.
- **Prompt rules that keep getting broken become hooks.** See `hooks/`.
- **User-invoked skills** carry `disable-model-invocation: true` and are slash commands.
  Model-invoked skills have trigger-rich descriptions.

## Skills

User-invoked, the merge request family, distilled from review lessons across several repos:

- `/mr-start`: ticket id, type, branch name, commit convention, hook install, before the first commit.
- `/mr-ready`: gate before opening or updating an MR. Checks and review per folder group, prove-red on new tests, numbers against the target branch, description that carries only what the page cannot show.
- `/mr-reply`: answer review comments. Live discussion, skip resolved and last-word-mine threads, fix then reply, outcome first, no longer than the comment, hashes that exist on origin.
- `/mr-close`: close or supersede. Backup ref and empty diff before rewrites; replies, close, delete branch, in that order.

Model-invoked:

- `toolchain`: install, typecheck, lint, test, coverage of changed lines, dead-code, prove-red. Language commands come only from `languages/`.

## Josh

Two binaries, `josh` and `josh-filter`. Build from source (Rust toolchain required):

```
git clone https://github.com/josh-project/josh.git
cd josh
cargo build --release -p josh-cli --bin josh --bin josh-filter
cp target/release/josh target/release/josh-filter ~/.cargo/bin/
```

The filter cache lives in `.git/josh/cache` of this repo. Deleting it only costs a recompute.

## Views

A view is a `views/<consumer>/workspace.josh` in this repo. Each line maps a path in the
view to a path in central, so vendored and owned skills appear side by side under flat names:

```
tdd = :/experimental/mattpocock/skills/engineering/tdd
mr-reply = :/skills/mr-reply
languages = :/languages
```

Commit the file here first, then clone the projection where the consumer expects it:

```
josh clone <path-or-url-of-this-repo> ':workspace=views/<consumer>' <destination>
```

Inside the projection: `git commit` as usual, `josh push` to write back into central,
`josh changes pull` to take central's changes. Edits to vendored paths made through a view
land in `experimental/` and violate the no-edit rule; adopt first.

## Hooks

- `hooks/commit-msg.sh`: git hook. Enforces `<type>(<ID>): subject` and one type per branch
  via `git config branch.<name>.mrType`. Install per project:
  `ln -sf <this repo>/hooks/commit-msg.sh .git/hooks/commit-msg`.
- `hooks/post-rebase-nag.sh`: Claude Code `PostToolUse` hook on `Bash`. After a rebase,
  prints the re-install, regenerate, retest reminder. Wire it in `settings.json`.

## Vendors

See `tools/vendors.conf`. Current: `mattpocock` from https://github.com/mattpocock/skills,
promoted skills under `experimental/mattpocock/skills/{engineering,productivity}`, betas
under `in-progress/`. Run `/setup-matt-pocock-skills` in a consuming project, not here.
