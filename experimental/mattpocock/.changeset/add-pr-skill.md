---
"mattpocock-skills": patch
---

Add the `pr` skill (in-progress bucket, model-invoked). It writes a pull request body built for fast human review: the summary comes from the primary source (the issue or spec), never inferred from the diff; the body states size up front, a reading order through the changed files, the smallest diagram or diff-sketch that shows the shape of the change (`show-me`'s technique, credited in the skill's `CREDITS.md`), a before/after pair of evidence for each claim (a visual pair first, a failing-then-passing test run where no visual exists), what was deliberately left out, and a one-way/two-way door call on merge risk. It opens the PR with `gh pr create` where available, and hands over the assembled body otherwise. Relates to #521, #938, #509, and #915.
