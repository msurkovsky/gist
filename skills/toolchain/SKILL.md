---
name: toolchain
description: Run language-specific checks without knowing the language. Use when a task needs to install dependencies, typecheck, lint, run tests, measure coverage of changed lines, find dead code, or prove a new test fails without its fix. Resolves commands from languages/<lang>/toolchain.md and answers "unsupported" instead of guessing.
---

# Toolchain

You are the only place that knows language-specific commands. Every other skill asks you
with verbs. You never guess a command: you read it from an adapter file or you report
`unsupported`.

Verbs: `install`, `typecheck`, `lint`, `test`, `coverage`, `dead-code`, `prove-red`.

## 1. Locate the language directory

Resolve `languages/` in this order and use the first that exists:

1. `../../languages/` relative to this skill's directory (the my-skills checkout).
2. `.claude/languages/` in the project.
3. `~/.claude/languages/`.

If none exists, report `unsupported: no languages/ directory found` and stop.

## 2. Detect toolchains per folder group

1. Read the `## Detect` section of every `languages/*/toolchain.md` (skip `_template`).
2. Group the files in scope (the diff range when given, else the repo) by the nearest folder
   containing a manifest. That folder is the **folder group root**.
3. A folder group may match several languages. Run each separately against that folder.
4. Check the project's `CLAUDE.md` and `.claude/rules/` for pinned commands (for example a
   `just check` or `make test`). A pinned command replaces the adapter's default for that verb.

## 3. Resolve and run

For each `(folder group, language, verb)`:

- Adapter file missing: report `unsupported: <lang> for <verb>. Add languages/<lang>/toolchain.md (copy languages/_template).` Do not run anything for that pair.
- Verb is `none` in the adapter: report `unsupported verb: <verb> in <lang>.`
- Binary from the command is not on PATH: report `missing binary: <name>` and stop that verb.
- Otherwise run the command from the folder group root, non-interactive, and read the result
  the way the adapter says.

## 4. Verb specifics

**coverage**: run the adapter's coverage command, locate the report (LCOV or Cobertura), then
compute uncovered lines that intersect the diff of `origin/<base>..HEAD` only. Report
`file:line` per uncovered changed line. Total percentage is never the deliverable.

**prove-red**: for each new or changed test in the diff:
1. Keep the test files, remove the non-test changes (stash the non-test paths, or check out
   the base into a worktree and copy the test files in).
2. Run only those tests with the adapter's isolation command.
3. Expected: fail. A test that passes here is reported as `not a test: <file> <name>`.
4. Restore the working tree exactly. Verify with `git status` before reporting.

**dead-code**: report findings as given; do not filter them by judgement. Deciding is the
caller's job.

## 5. Report

One line per `(folder group, language, verb)`:

```
<folder>  <lang>  <verb>  ok | fail | unsupported | missing binary
```

Then details only for `fail`: the first error block for typecheck, failing test names for
test, `file:line` lists for lint, coverage, and dead-code. No summaries, no advice.
