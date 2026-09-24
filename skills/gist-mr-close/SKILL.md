---
name: gist-mr-close
description: Close or supersede a merge request without losing replies or history. Backs up before any rewrite, proves the tree unchanged, posts before closing, deletes the branch last.
disable-model-invocation: true
---

# MR close

Order matters here more than wording. Deleting a branch first auto-closes the MR before
your replies land.

## 1. Decide: close or supersede

- **Supersede**: the source branch must change. Neither GitLab nor GitHub can change an
  MR's source branch. New branch means new MR.
- **Close**: the work is dropped or landed elsewhere.

## 2. Before any history rewrite

```
git branch backup/<branch>-<yyyymmdd> <branch>
```

After the rewrite, prove the tree is unchanged:

```
git diff backup/<branch>-<yyyymmdd> HEAD
```

Empty output or stop. A non-empty diff means the rewrite changed content; do not continue
until it is explained or reverted. Keep the backup ref until the new MR is merged.

## 3. Supersede sequence

1. Push the new branch, open the new MR (`/gist-mr-ready` first).
2. Fetch the old MR's discussion live. Anything still open gets its answer in the closing
   comment, following `/gist-mr-reply` rules.
3. Post one closing comment on the old MR: the new MR number, then the answers to the still
   open threads. One comment, no essay.
4. Close the old MR.
5. Delete the old remote branch.

## 4. Plain close sequence

1. Post the closing comment with the reason in one sentence.
2. Close.
3. Delete the remote branch.

## 5. Confirm before acting

Closing and deleting are outward and hard to undo. Show the exact sequence of commands with
MR numbers and branch names, ask once, then run them in that order. Verify each step landed
before running the next.
