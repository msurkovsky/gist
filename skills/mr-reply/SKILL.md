---
name: mr-reply
description: Answer review comments on a merge request. Fetches the live discussion, drops threads that need no reply, fixes first, then posts short outcome-first replies with hashes that exist on origin.
disable-model-invocation: true
---

# MR reply

Reviewers read long replies as not listening. Fix, then say what you did, in fewer words
than they used.

## 1. Fetch the live discussion

Never work from a snapshot older than the last push. Fetch right before drafting and again
right before posting.

- GitLab: `glab mr view <id> --comments`, or the discussions API via `glab api` for thread
  state (`resolved`, last author).
- GitHub: `gh pr view <n> --comments` and `gh api` for review threads.

Also `git fetch origin` so hash checks below are against the real remote.

## 2. Drop what needs no reply

- Resolved threads: nothing.
- Threads where the last word is already yours: nothing.
- Threads already answered by a commit the reviewer has seen: nothing.

What remains is the work list. Show it as `thread -> planned action`, one line each.

## 3. Decide per thread

- A nit from a maintainer is done, not debated.
- A finding that is correct is fixed.
- A finding that is wrong gets one sentence saying why. Only then. Not "your call", not
  "point me at the blocks". The decision is never handed back.
- If the ask will not be done, that is sentence one of the reply, not paragraph three.

## 4. Fix, push, then reply

Work thread by thread: fix, commit, push, reply. Not twelve replies at 22:00 after one big
push.

Before writing a hash into a reply, prove it is on origin:

```
git merge-base --is-ancestor <sha> origin/<branch>
```

Renaming a commit message changes the hash; renaming the branch does not.

## 5. Reply format

- Sentence one is the outcome: `Done in abc1234.` / `Not done: <one reason>.` / `Wrong: <one sentence>.`
- No longer than the comment it answers.
- Do not restate the reviewer's finding.
- No tables, no bold, no headers, no praise, no "worth flagging", no "happy to".
- A number appears only if measured against the target branch and reproduced twice.

## 6. Post

Posting is outward-facing. Show every draft as `thread: reply`, ask once, then post one
reply per thread with `glab mr note` / `gh pr comment` or the thread-specific API call.
Leave resolving to the reviewer unless the project says otherwise.
