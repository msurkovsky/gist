# Outline: worked example

Problem: a contributor needs a quick map of an unfamiliar tree before choosing
what to inspect. Counting files should help their task without forcing an extra
conversation when they already specified the next step.

Design: `gk outline` provides counts and bounded extension totals. The skill
interprets that result and follows the user's requested level of detail.

## outline-shape

Request: "Give me a quick overview of this directory."
Fixture: an isolated directory with two `.rs` files and one `.md` file of known sizes.
Expected: report three files, correct total bytes and per-extension counts. Do not
infer module responsibilities from extensions alone. With a report limit of one,
show one type and mark the result truncated.

Deterministic evidence: `outline_reports_the_shape_of_a_tree` and
`limit_truncates_and_says_so` in `tools/crates/gist-cli/tests/cli.rs` cover counts
and bounded output using temporary fixtures. They do not test the agent's prose.

## outline-continue

Request: "Orient yourself, then review the architecture in README.md."
Fixture: the same tree with a README describing its parts.
Expected: use the summary if useful, then read the requested file and continue the
review. Do not stop to ask what altitude is wanted when the request already says.

## outline-non-trigger

Request: "Fix the typo on the supplied line in README.md."
Expected: the catalog should not select this skill solely because a repository is
present; a whole-tree survey does not help the specified edit.

## outline-missing-tool

Request: "Summarize this directory using Gist."
Fixture: `gk` is absent from PATH, or the supplied directory is missing.
Expected: report the missing prerequisite or command error; do not invent counts
or install tools implicitly.

## Evidence and review

The case IDs above are the acceptance checklist. A source inspection can establish
that the instructions permit continuation and describe missing-tool behavior.
It cannot establish selection or execution by either host. Actual Claude and Codex
scenario runs are pending and must be recorded before claiming behavioral validation.
This is the worked mapping from problem to cases to available evidence, including
what that evidence does not prove.
