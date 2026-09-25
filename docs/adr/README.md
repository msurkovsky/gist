# Decisions

One file per decision that would otherwise be re-litigated from scratch in six
months. Numbered, never renumbered. An ADR states the decision as it holds at
that commit: when a later change alters it, update the ADR in the same change.
Git keeps the history.

Every change to an ADR adds a row to its `## Changelog` table at the end, newest
first, in the same commit: `YYYY-MM-DD HH:MM` local time, the author's name, and
one line on why. The diff already shows what.

Write one when the choice was not obvious, the rejected options were real, and
the reasoning would not survive in a commit message.
