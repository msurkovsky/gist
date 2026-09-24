# Contributing

Recipes for every kind of change. `CLAUDE.md` holds the short list of rules; this file
says how to do things. Run `tools/check.sh` before every commit.

## Where things go

| Change | Location | Owner of the file |
|---|---|---|
| Workflow skill | `skills/<name>/SKILL.md` | us, edit freely |
| Third-party skill to try | `experimental/<vendor>/...` | upstream, never edited here |
| Language-specific command | `languages/<lang>/toolchain.md` | us |
| Always-on constraint | `rules/<topic>.md` | us |
| Deterministic guard | `hooks/<name>.sh` | us |
| What a consumer sees | `views/<consumer>/workspace.josh` | us |

## Add an owned skill

1. `mkdir skills/<name>` and write `SKILL.md`. Frontmatter:
   ```yaml
   ---
   name: <name>            # equals the directory name
   description: <one line> # human-facing for slash commands, trigger-rich for model-invoked
   disable-model-invocation: true   # only for slash-command skills
   ---
   ```
2. Keep it language-agnostic. Anything that needs a compiler, test runner, or package
   manager is a call to the `toolchain` skill with verbs. `tools/check.sh` greps for
   language tool names under `skills/` and fails on hits.
3. Reference other skills with `Call the Skill tool with "<name>"`. No relative links into
   other skill folders. A user-invoked skill cannot be called this way; tell the user to run it.
4. Write steps as imperatives. One idea per line. Say `unsupported` or `stop` where the
   skill must not guess. No praise, no filler, no summaries of what was just said.
5. Add one line to the **Skills** section of `README.md`.

## Adopt a skill from `experimental/`

1. Copy the whole skill directory into `skills/<name>/`. Rename if the name collides.
2. Add to its frontmatter:
   ```yaml
   source: experimental/<vendor>/<path>@<upstream sha>
   ```
   The SHA is the `Upstream:` value in the last import commit, shown by `tools/vendor.sh list`.
3. Edit the copy. `experimental/` stays untouched. `tools/vendor.sh check` finds violations.
4. When the upstream changes later, diff `experimental/<vendor>/<path>` against the copy by
   hand and port what you want. The `source:` line tells you where you started.

## Add or update a vendor

```
tools/vendor.sh add <name> <git url> [branch]    # first import, full history
tools/vendor.sh update [name...]                 # re-import, clean merge, no duplicate commits
tools/vendor.sh list                             # registry with the imported upstream SHA
```

The script commits for you: one `Register vendor <name>` commit, then one merge per
import. Add the vendor to the **Vendors** section of `README.md` with a one-line note on
what is worth trying.

## Add a language

```
cp -r languages/_template languages/<lang>
```

Fill `## Detect` with the manifest file names, then each verb with a command and how to
read its output. A verb with no tool stays `none`. Nothing under `skills/` changes.

## Add a rule

`rules/<topic>.md`, language-agnostic, short imperative lines. If the rule only holds for
one project, it belongs in that project's `CLAUDE.md` or `.claude/rules/`, not here.

## Add a hook

`hooks/<name>.sh`, executable, with a header comment that states what it enforces and the
exact install line or `settings.json` snippet. Prefer a hook over a rule whenever the rule
has already been broken more than once. Add a smoke test line to `tools/check.sh`.

## Define a view

1. `mkdir views/<consumer>` and write `workspace.josh`, one `dest = :/source` per line.
2. Commit here first. Josh cannot clone a workspace path that does not exist in central.
3. `josh clone <this repo> ':workspace=views/<consumer>' <destination>`.
4. In the projection: `git commit`, `josh push`, `josh changes pull`. Josh rewrites
   `workspace.josh` into canonical form on the first push; commit that as-is.

## Commits

- Subject: plain imperative sentence, capitalised, no conventional-commit prefix.
- Body: why, not what. The diff shows what.
- One concern per commit. Vendor imports and registry entries are separate commits by
  construction; do not fold other changes into them.
- Run `tools/check.sh` first. It exits non-zero on any rule violation.

## Writing style inside skills and rules

- Imperative mood, present tense, second person implied.
- No em-dashes. Use a comma, colon, or a new sentence.
- Name the tool, not the vibe: `Call the Skill tool with "toolchain"`, not "run the checks".
- Prefer a stop condition over a caveat: `unsupported`, `stop`, `ask once`.
