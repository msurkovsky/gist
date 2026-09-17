---
name: pr
description: "Compose a pull request body that makes human review fast: a summary drawn from the primary source (not the diff), a readable walkthrough of the shape of the change, a before/after pair of evidence for each claim (visual first, a failing-then-passing test run otherwise), what was deliberately left out, and a one-way/two-way door call on merge risk. Use when opening a PR, running `gh pr create`, or asked to write a PR description or body."
---

# PR

A pull request body has one job: let the reviewer approve or reject the change without re-deriving it from the diff. The failure mode this skill exists to fix is the generic body most harnesses default to: a bulleted rehash of the diff under "Summary", a "Test plan" checklist that is really a to-do list, and nothing about how carefully to read it or how hard it would be to undo. None of that tells the reviewer *why*, *how big*, *how risky*, or *where to look first*. This skill produces a body that does.

## Process

### 1. Pin the change

Same discipline as `code-review`'s step 1: capture `git diff <base>...HEAD` (three-dot, against the merge-base) and `git log <base>..HEAD --oneline`. Refuse to proceed on a dirty working tree or an empty diff; that fails here, not halfway through drafting.

Check the commits carry **one intent**. If the diff visibly does two unrelated things (a refactor bundled with a feature, an unrelated fix riding along), say so and offer to split it into two PRs before writing a body for either. A body cannot make an unfocused diff easy to review; it can only describe the mess accurately.

State the size up front, first line of the body: files touched, lines added/removed. A reviewer calibrates before starting, not halfway through.

### 2. Find the primary source, not just the diff

The diff answers "what changed." It cannot answer "why," and a body that only reads the diff back will confidently invent a "why" that sounds plausible and isn't. Find the actual source before writing the summary:

1. An issue or spec referenced in the commit messages or branch name (`#123`, `Closes #45`).
2. A path the user hands you.
3. A spec file under `docs/`, `specs/`, or `.scratch/` matching the branch or feature.
4. Failing all three, ask what prompted the change, or say plainly in the body that no upstream source was found and the summary is inferred from the diff.

Write the **Summary** section from this source. The diff stays evidence for the *how*, never the *why*.

### 3. Speak the repo's vocabulary

Read `CONTEXT.md` (or `CONTEXT-MAP.md` and the contexts the diff touches) if the repo has one. Name domain concepts in the body using the glossary's term, not a synonym it explicitly avoids: this is what makes the body legible to the reviewer who wrote that glossary, and it's the whole point of pairing this skill with `grill-with-docs` rather than treating vocabulary as decoration.

If the diff contradicts a documented ADR, do not silently describe the new behaviour as if the old decision never existed. Say so, in the body: "Contradicts ADR-0007 (event-sourced orders); worth reopening because ..." A reviewer who remembers the ADR and sees no mention of it will wonder whether you knew.

### 4. Pick a reading order

Diffs land alphabetically by path, which is rarely the order that makes them make sense. Decide the order a reviewer should open the changed files in so the diff arrives as a narrative (the type first, then the function that uses it, then the call site, then the test), and say so as a short numbered list with a one-line reason per file. Skip this section on a single-file or single-hunk diff; it only earns its place once there is an order worth choosing.

### 5. Draw the shape of the change

This step is `show-me`'s technique, aimed at a diff instead of a live conversation (credit and lineage in `CREDITS.md` beside this file). Pick the smallest visual that makes the shape of the change land, embedded directly in the body as Markdown, never a standalone HTML file:

- **A `diff`-shaped sketch** when the point is what changed and the surrounding shape already exists: a component tree, a file tree, or a call tree with `+`/`-` lines, matched to the topic.
- **A Mermaid diagram** when the point is control flow or data flow between components: a sequence diagram for a new interaction, a flowchart for new branching logic.
- **A before/after file tree** when the point is where responsibility moved.

Use one, occasionally two; never all three. If nothing embeds well for this particular change, a plain description is fine and forcing a diagram is worse than skipping it.

### 6. Show the before and after

Never write "tests pass." A single after-the-fact snapshot, whether it's a screenshot or a green test run, proves the current state works; it doesn't prove *this diff* is what changed it. Evidence is a pair: what it looked like before, and what it looks like after, for the same input.

Reach for a visual pair first: a before/after screenshot or terminal-output pair for the same action, a rendered diagram, a comparison of the actual output for the same input. Embed both sides directly in the body; a single "after" image is half the proof.

Where nothing visual exists, fall back to code execution: run the specific test red against the base commit (or temporarily revert the fix) and green on this branch, and quote both runs rather than describing them. Name the failure each pair rules out: "`orders.test.ts:42` failed on `main` with a double refund, passes on this branch" says something a bare "all tests green" doesn't, because a tautological test that asserts its own mock would pass both before and after and prove nothing. If you ran the repo's other checks (lint, typecheck, the full suite), say which ones and that they were run for real, not assumed.

If a feared failure has neither a visual nor a test covering it on either side, say that plainly instead of pointing at unrelated green output.

### 7. Say what was left out, on purpose

List what the diff deliberately does not handle, implement, or fix, even things adjacent to the change that a reviewer might expect. This is the part almost nobody writes and the one reviewers most want, because it aims their attention at the acknowledged gap instead of making them hunt for it. It is necessarily incomplete: an agent can reliably report what it considered and chose to skip, not what it never thought of. Say only the former; do not claim the section is exhaustive.

### 8. Call the door

State whether this change is a **one-way door** (expensive or impossible to reverse once merged: a schema migration, a deleted column, a public API change, anything with external callers) or a **two-way door** (cheap to revert or roll forward: additive, behind a flag, contained to one internal module), and the reason in one sentence. This is a judgement call the author is making explicitly, not leaving for the reviewer to infer from the diff's size. A big diff can be a two-way door (a mechanical rename) and a one-line diff can be a one-way one (dropping a column); size and reversibility are different axes, so don't conflate them.

### 9. Assemble and open

Fill this shape, dropping any section a step above said to skip:

```markdown
## Summary
<1-3 sentences, from the primary source, stating why>

**Size:** <N files, +A/-B lines>  **Door:** <one-way | two-way> (<reason>)

## Reading order
1. `path/to/file.ts`: <why this one first>
2. `path/to/other.ts`: <why next>

## The shape of the change
<diagram, diff-sketch, or tree>

## Evidence
- **Before:** <screenshot/output/failing test run>
  **After:** <screenshot/output/passing test run>
  Rules out: <specific failure>
- ...

## Left out, on purpose
- <thing deliberately not handled, and why>

## Not sure about
- <a real doubt about the change, if one exists; omit the section if none does>
```

Open the PR with `gh pr create --body-file <path>` (a heredoc inlining the body works too) rather than retyping the body into the command; a body with embedded fences and a Mermaid block is easy to mangle passed as a flag value. If `gh` is unavailable or the workflow calls for a human to open the PR, hand over the assembled body instead of the command.
