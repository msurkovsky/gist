# Contributing

Recipes for every kind of change. `CLAUDE.md` holds the short list of rules; this file
says how to do things. Run `just ci` before every commit.

## Where things go

| Change | Location | Owner of the file |
|---|---|---|
| Workflow skill | `skills/gist-<name>/SKILL.md` | us, edit freely |
| Third-party skill to try | `experimental/<vendor>/...` | upstream, never edited here |
| `gk` subcommand | `tools/crates/gist-cli/` | us, read `docs/tool-contract.md` first |
| Always-on constraint | `rules/<topic>.md` | us |
| Deterministic guard | `gk hook <name>` (git), `hooks/<name>.sh` (Claude Code) | us |
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
2. Keep it language-agnostic. A skill that needs checks run points at the commands the
   project's `CLAUDE.md` names; it never names a compiler or test runner itself.
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

A git hook is a `gk hook <name>` subcommand in `tools/crates/gist-cli/src/hook.rs`: a
`HookCommand` variant, an arm in `run`, and the name in `HOOKS`, which is what
`gk hook install` writes shims for. A unit test fails if `HOOKS` names a hook with no
subcommand. Add end-to-end tests in `tests/cli.rs` (see `docs/adr/0009`).

A Claude Code hook is `hooks/<name>.sh`, executable, bash 3.2 compatible, with a header
comment that states what it enforces and the exact `settings.json` snippet. Add a smoke
test line to `scripts/check.sh`.

Prefer a hook over a rule whenever the rule has already been broken more than once.

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
- `gk hook commit-msg` enforces this. Install it here once with `just install`
  (puts `gk` on PATH) and `gk hook install`.
- One concern per commit. Vendor imports and registry entries are separate commits by
  construction; do not fold other changes into them.
- Run `just ci` first. It exits non-zero on any rule violation.

## Writing style inside skills and rules

- Imperative mood, present tense, second person implied.
- Name the tool, not the vibe: `Call the Skill tool with "gist-doc-review"`, not "check
  the comments".
- Prefer a stop condition over a caveat: `unsupported`, `stop`, `ask once`.
