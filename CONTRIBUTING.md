# Contributing

Recipes for every kind of change. `CLAUDE.md` holds the short list of rules; this file
says how to do things. Run `just ci` before every commit.

## Workflow and approval

`MANIFESTO.md` governs this workflow. Start from `docs/architecture.md`; use
`docs/skill-contract.md` for skills and `docs/tool-contract.md` for tooling.

1. State the problem, scope, and intended outcome. Propose a plan for human
   judgment before implementation. Existing explicit approval counts; do not
   request the same decision again.
2. Update the relevant design, behavior cases, and decisions. Present the prose
   change for approval before building the changed behavior. For an unchanged
   behavior, cite the existing case and explain why it still applies. A human's
   explicit request to implement reviewed findings authorizes those corrections;
   new behavior outside that scope needs a new decision.
3. Implement the approved behavior. Exploration may precede approval, but a spike
   is disposable evidence, not the implementation to ship.
4. Prove every new test red, then green, following `rules/code-and-comments.md`.
   The red run must fail on the behavior under test; a compilation error or
   broken fixture does not count. Run `just ci` and the relevant behavior scenarios.
5. Prepare a review map: changed module or skill, source section/case, verification
   result, and remaining uncertainty. Until automated coherence checks exist, list
   these pairs in the review handoff. Have a human or a separate agent context
   inspect them against the original problem. An implementer's self-review is
   useful but does not satisfy independent review; label it pending if unavailable.
6. The human weighs that evidence before merge. Resolve findings, update lasting
   docs, and remove completed plan entries from `TODO.md`. Keep evidence in the
   review record; keep enduring behavior and cases in the tree.

For small changes the plan and prose review can share one short exchange, provided
both decisions are explicit. Mechanical checks cannot establish human approval,
requirement correctness, or a skill's decision quality.

## Where things go

| Change | Location | Owner of the file |
|---|---|---|
| Workflow skill | `skills/gist-<name>/SKILL.md` | us, edit freely |
| Third-party skill to try | `experimental/<vendor>/...` | upstream, never edited here |
| `gk` subcommand | `tools/crates/gist-cli/` | us, read `docs/tool-contract.md` first |
| Always-on constraint | `rules/<topic>.md` | us |
| Deterministic guard | `gk hook <name>` (git), `hooks/<name>.sh` (Claude Code) | us |
| What a consumer sees | `views/<consumer>/workspace.josh` | us |
| Decision worth not re-litigating | `docs/adr/NNNN-<slug>.md`, kept current, each change logged in its Changelog | us |

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
   project's agent instructions name; it never names a compiler or test runner itself.
3. Name dependencies with `Use the gist-<name> skill through the host's skill mechanism`.
   No relative links into other skill folders. For an explicit-only dependency, ask the
   user to invoke it. Declare missing-dependency behavior instead of guessing.
   For explicit-only skills also add `policy.allow_implicit_invocation: false` to
   `agents/openai.yaml`; the frontmatter flag alone does not cover Codex.
4. Write steps as imperatives. One idea per line. Say `unsupported` or `stop` where the
   skill must not guess. No praise, no filler, no summaries of what was just said.
5. Add a row to the **Skills** table in `README.md`.
6. Add cases and evaluation evidence under `docs/cases/`, using
   `docs/skill-contract.md`. Exercise trigger, non-trigger, and missing-prerequisite
   scenarios. Record which host was evaluated and what remains untested.
7. Run `just ci`; it rebuilds the embedded skills for the tests. Install a new binary
   or project skills only when that installation is requested. Packaging tests use
   temporary directories and do not change a working project's setup.

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
- Name the capability: `Use the gist-doc-review skill through the host's skill
  mechanism`. Do not assume a host-specific tool; see `docs/skill-contract.md`.
- Prefer a stop condition over a caveat: `unsupported`, `stop`, `ask once`.
