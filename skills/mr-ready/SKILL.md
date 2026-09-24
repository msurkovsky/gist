---
name: mr-ready
description: Gate a branch before opening or updating a merge request. Runs checks per folder group, reviews per folder group, proves tests, measures against the target branch, and drafts a description that carries only what the page cannot show.
disable-model-invocation: true
---

# MR ready

Run before `git push` that opens or updates an MR. Every step reports `ok`, `fail`, or
`unsupported`. Never skip a step silently.

## 1. Fix the baseline

```
git fetch origin
```

Base is `origin/<target branch>`, never a mid-branch commit. Range for everything below is
`origin/<base>..HEAD`. State the base and the range once.

## 2. Commit hygiene

- Every commit in the range matches the project's commit pattern.
- One type across the branch. Read `git config branch.<name>.mrType` when set.
- No `fixup!`, `squash!`, or WIP commits left. If history must be rewritten, follow the
  backup-ref rule from `/mr-close` first.

## 3. Toolchain gate

Call the Skill tool with "toolchain" for verbs `install`, `typecheck`, `lint`, `test`, and
`coverage` over the range. Then `prove-red` for every new or changed test.

- `unsupported` lines are reported verbatim in the final checklist. Do not paper over them.
- Coverage findings are handled by deleting dead code or adding a test, per
  `rules/code-and-comments.md`. Not by explaining.

## 4. Review per folder group

Group changed files by folder group (same grouping the toolchain used). For each group,
review that group alone: call the Skill tool with "code-review" if available, otherwise
read the diff of that group and review it yourself. One review over the whole MR misses
what a per-folder pass finds.

For each finding: fix it, or decline it in one line. No third option.

## 5. Doc review

Call the Skill tool with "gist-doc-review" over the range when available. Comment runs of
four or more lines get read individually. Ratio numbers are for triage, never a target.

## 6. Numbers

Any performance or size claim in the description or a reply: measured against the base,
reproduced twice, both runs shown. Otherwise the number is not posted.

## 7. Description

The MR page already shows the diff, commits, and files. The description carries only what
the page cannot:

- what this MR supersedes, if anything
- pairing constraints: other MRs or repos that must land together
- which commits are new since the last review round (short range or list)

No review order, no design essay, no summary of the diff.

## 8. Checklist and hand-off

Print the checklist:

```
baseline     origin/<base>..HEAD
commits      ok | fail: <reason>
toolchain    <one line per folder/lang/verb>
review       <n> findings, <fixed> fixed, <declined> declined
docs         ok | <n> comments changed | unsupported
numbers      none | reproduced
description  drafted
```

Opening or updating the MR on the forge is an outward action. Show the description and
ask once before running `glab mr create` / `glab mr update` or the `gh pr` equivalent.
