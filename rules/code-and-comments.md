# Code and comments

Language-agnostic rules for code that will be reviewed. Project-specific conventions
(test file naming, formatter, commit types, base branch) live in the project's own
agent instructions (`AGENTS.md`, `CLAUDE.md`, or host-specific rules), not here.

## Comments

- Comment only what the code cannot say. A clear private helper gets nothing.
- Every public export gets one line: what it is for, not how it works.
- A comment restating the line below it is deleted.

## Dead code

- Dead code is deleted, not documented.
- A guard that no test can reach is dead. Either write the test that reaches it or delete it.
- When asked about missing coverage, the answer is a deletion or a test, never an essay.

## Tests

- A test that still passes with the guarded line deleted is not a test.
- Every new test is proven red at least once: run it without the fix and watch it fail.
- Never claim a test covers a change without having seen it fail.

## Before finishing

1. Typecheck, lint, tests, with the commands the project's agent instructions name. None named: say so, do not guess or skip silently.
2. Re-read the project's rule files for anything new you touched: test file names, imports, signals.
3. After any rebase: re-install dependencies, rebuild generated code, rerun everything. A green run against a stale client is not green.

## Numbers

- Measure against the target branch on origin, never against a mid-branch commit.
- Post a number only after reproducing it twice.
