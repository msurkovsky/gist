# Contributing

Recipes for every kind of change. `CLAUDE.md` holds the short list of rules; this file
says how to do things. Run `just ci` before every commit.

## Where things go

| Change | Location | Owner of the file |
|---|---|---|
| Workflow skill | `skills/gist-<name>/SKILL.md` | us, edit freely |
| Third-party skill to try | `experimental/<vendor>/...` | upstream, never edited here |
| `gk` subcommand | `tools/crates/gist-cli/` | us, read `docs/tool-contract.md` first |
| Language-specific command | `languages/<lang>/toolchain.md` | us |
| Always-on constraint | `rules/<topic>.md` | us |
| Deterministic guard | `hooks/<name>.sh` | us |
| What a consumer sees | `views/<consumer>/workspace.josh` | us |
| Decision worth not re-litigating | `docs/adr/NNNN-<slug>.md` | us |

## Add an owned skill

1. `mkdir skills/gist-<name>` and write `SKILL.md`. Frontmatter:
   ```yaml
   ---
   name: gist-<name>       # equals the directory name
   description: <one line> # human-facing for slash commands, trigger-rich for model-invoked
   disable-model-invocation: true   # only for slash-command skills
   ---
   ```
2. Keep it language-agnostic. Anything that needs a compiler, test runner, or package
   manager is a call to the Skill tool with `gist-toolchain` and verbs. `scripts/check.sh`
   greps for language tool names under `skills/` and fails on hits.
3. Reference other skills with `Call the Skill tool with "gist-<name>"`. No relative links
   into other skill folders. A user-invoked skill cannot be called this way; tell the user
   to run it.
4. Write steps as imperatives. One idea per line. Say `unsupported` or `stop` where the
   skill must not guess. No praise, no filler, no summaries of what was just said.
5. Add a row to the **Skills** table in `README.md`.
6. Rebuild and reinstall `gk` (`just install`); skills are embedded at compile time.

## Adopt a skill from `experimental/`

1. Copy the whole skill directory into `skills/gist-<name>/`; the frontmatter `name` gets
   the prefix too.
2. Add to its frontmatter:
   ```yaml
   source: experimental/<vendor>/<path>@<upstream sha>
   ```
   The SHA is the `Upstream:` value in the last import commit, shown by `just vendor list`.
3. Edit the copy. `experimental/` stays untouched. `scripts/vendor.sh check` finds
   violations.
4. When the upstream changes later, diff `experimental/<vendor>/<path>` against the copy by
   hand and port what you want. The `source:` line tells you where you started.

## Add or update a vendor

```
just vendor add <name> <git url> [branch]    # first import, full history
just vendor update [name...]                 # re-import, clean merge, no duplicate commits
just vendor list                             # registry with the imported upstream SHA
```

Needs `josh-filter` on PATH (README, section Composition). The script commits for you:
one `Register vendor <name>` commit, then one merge per import. Add the vendor to the
**Vendors** section of `README.md` with a one-line note on what is worth trying.

## Add a language

```
cp -r languages/_template languages/<lang>
```

Fill `## Detect` with the manifest file names, then each verb with a command and how to
read its output. A verb with no tool stays `none`. Nothing under `skills/` changes.

## Add a tool

A tool is a `gk` subcommand in Rust. `docs/tool-contract.md` lists the steps and the
output, exit-code, and behaviour rules. A bash script is acceptable only while it stays a
one-liner: one command or pipeline, no branching, no loops, no argument parsing. Put it
under `scripts/` behind a `just` recipe. When it outgrows that, port it to `gk` rather than
growing the script.

## Add a rule

`rules/<topic>.md`, language-agnostic, short imperative lines. If the rule only holds for
one project, it belongs in that project's `CLAUDE.md` or `.claude/rules/`, not here.

## Add a hook

`hooks/<name>.sh`, executable, bash 3.2 compatible, with a header comment that states what
it enforces and the exact install line or `settings.json` snippet. Prefer a hook over a
rule whenever the rule has already been broken more than once. Add a smoke test line to
`scripts/check.sh`.

## Define a view

1. `mkdir views/<consumer>` and write `workspace.josh`, one `dest = :/source` per line.
2. Commit here first. Josh cannot clone a workspace path that does not exist in central.
3. `josh clone <this repo> ':workspace=views/<consumer>' <destination>`.
4. In the projection: `git commit`, `josh push`, `josh changes pull`. Josh rewrites
   `workspace.josh` into canonical form on the first push; commit that as-is.

## Commits

- Subject: plain imperative sentence, capitalised, no trailing period, no
  conventional-commit prefix. Under 50 characters reads best, 72 is the hard limit.
- Blank line, then a body that says why, not what. The diff shows what. Wrap at 72.
  A one-line change with an obvious reason needs no body.
- `hooks/commit-msg.sh` enforces this. Install it here once:
  `ln -sf ../../hooks/commit-msg.sh .git/hooks/commit-msg`.
- One concern per commit. Vendor imports and registry entries are separate commits by
  construction; do not fold other changes into them.
- Run `just ci` first. It exits non-zero on any rule violation.

## Writing style inside skills and rules

- Imperative mood, present tense, second person implied.
- Name the tool, not the vibe: `Call the Skill tool with "gist-toolchain"`, not "run the
  checks".
- Prefer a stop condition over a caveat: `unsupported`, `stop`, `ask once`.
