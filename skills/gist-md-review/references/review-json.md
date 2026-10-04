# What `gk md-review wait` prints

Reviewer text is always indented under its thread or heading. What to do
with each line is in the skill.

## A submitted round

```
review submitted · docs/plan.md · round 2 · 3 threads
redelivered · first 14:02 UTC · replied t7
warning: docs/plan.md changed since round 2 was read; line numbers refer to v2

summary
  Good structure. Cut the security section by half.

[t7] comment · lines 41–44 · Loop › Anchors
  > A comment stores the quoted text, surrounding context, and the source line range
  Too long; one sentence.

[t8] comment · orphaned, was line 60 in v1 · Security
  > Check the Host header
  Name the attack.
  ↳ applied: Named DNS rebinding.
  Still unclear which header.

showing 2 of 3 threads; see the rest with gk md-review status docs/plan.md --round 2 --offset 2

next: gk md-review reply docs/plan.md <id> --outcome applied|declined --note "…", then gk md-review next docs/plan.md
```

| Line | Meaning |
|---|---|
| header | the file, the round, how many threads the submit holds |
| `redelivered` | this round was delivered before; the threads listed were replied to |
| `warning` | the file changed after the reviewer read it; line numbers refer to the version read |
| `summary` | the overall comment, on the whole document; absent when empty |
| `[id] comment · …` | a thread: its id, where its quote is in the version read, and the heading path |
| `orphaned, was …` | the quote is no longer in the file; the place is where it last was |
| `applied in round N`, `resolved` | on a redelivery, you already applied it; or the reviewer resolved it after commenting |
| `  > …` | the quoted passage, cut when long |
| indented text | a reviewer message, oldest first |
| `  ↳ applied: …` | your earlier reply on the thread |
| `showing X of Y` | only X of the submit's threads are shown |
| `next:` | a reminder of the commands; the steps are in the skill |

A thread appears when the reviewer wrote in it this round, with its whole
history.

## Approval

```
review approved · docs/plan.md · round 3 · v3
warning: docs/plan.md changed after v3 was approved; the record holds the approved text and the changes since

note
  Ship it.

record .md-review/records/docs-plan-md-<time>.md
4 threads: 3 applied, 1 resolved, 0 open
discarded at approve: t11
```

The record, relative to the repository root, holds every round's threads
and outcomes. `discarded` lists comments the reviewer wrote but did not
submit before approving.

## Timeout

```
timeout after 110m; nothing submitted
next: gk md-review wait docs/plan.md --timeout 110m
```

## Server not running

```
the review server for docs/plan.md is not running; the review is kept
resume: gk md-review serve docs/plan.md --detach
```
