# Merge requests

Language- and forge-agnostic rules for work that goes through review. A project's own
branch, commit, and description conventions live in its `CLAUDE.md` and win over these.

## History

- Before any history rewrite: `git branch backup/<branch>-<yyyymmdd>`. After it,
  `git diff backup/<branch>-<yyyymmdd> HEAD` must be empty; a non-empty diff is explained
  or reverted before anything is pushed. Keep the backup until the MR merges.

## Description

- Carry only what the page cannot show: what the MR supersedes, other MRs that must land
  with it, the commits new since the last review round. No diff summary, no review order,
  no design essay.

## Replies to review

- Sentence one is the outcome: `Done in abc1234.`, `Not done: <reason>.`, or
  `Wrong: <reason>.` Never hand the decision back to the reviewer.
- No longer than the comment it answers. Do not restate the finding.
- A hash in a reply is on origin first: `git merge-base --is-ancestor <sha> origin/<branch>`.
- Fetch the discussion right before drafting. Skip resolved threads and threads where the
  last word is already yours.

## Superseding

- A new source branch means a new MR; no forge can change an MR's source branch.
- Order: open the new MR, post one closing comment on the old one with the new number and
  answers to its open threads, close it, delete the old branch last. Deleting the branch
  first closes the MR before the comment lands.

## Outward actions

- Opening, updating, commenting on, or closing an MR, and deleting a remote branch, reach
  other people. Show the exact commands, ask once, then run them in order.
