## What it does

`pr` writes the body of a pull request. Its defining constraint is where the "why" comes from: it is drawn from the [primary source](https://www.aihero.dev/ai-coding-dictionary/primary-source), the issue or spec the branch closes, never inferred by reading the diff back. A body built only from the diff can describe *what* changed in convincing detail while getting *why* completely wrong, because the diff never carried that information in the first place.

Around that summary, the body states size up front, a reading order through the changed files, the smallest diagram or diff-sketch that shows the shape of the change, evidence tied to the specific failure each check rules out (never a bare "tests pass"), what was deliberately left out of scope, and a one-way/two-way door call on how expensive the change is to reverse.

## When to reach for it

Type `/pr`, or the agent reaches for it automatically when you ask it to open a pull request, run `gh pr create`, or write a PR description.

| Your situation | Reach for |
| --- | --- |
| A diff is committed and ready to send for review | `pr` |
| You want the diff itself checked for standards or spec compliance before it goes out | [code-review](https://aihero.dev/skills-code-review), then `pr` |
| Nothing is committed yet | [implement](https://aihero.dev/skills-implement) or [tdd](https://aihero.dev/skills-tdd) first |
| Comments have come back on an open PR and need working through | Not this skill; there is no shipped skill for that yet (see Common questions) |

## Prerequisites

None. `pr` is stateless: it reads the diff and whatever primary source it can find, and writes a body. It runs `gh pr create` to open the PR when `gh` is available and configured; without it, it hands over the assembled body for you to paste in by hand.

## The four things a generic body skips

A harness's default PR body is a rehash of the diff under "Summary" and a "Test plan" checklist. `pr` adds the parts that default shape structurally cannot produce:

- **The reading order.** Diffs land alphabetically by path; `pr` picks the order that turns the diff into a narrative instead.
- **Evidence tied to a named fear.** Not "tests pass," but which test covers which failure you were actually worried about, and an honest note where no test covers the one that matters.
- **What was left out, on purpose.** A list of what the diff does not handle, aimed at the gap a reviewer would otherwise have to find by reading closely enough to notice its absence.
- **The door.** A one-way (expensive to reverse: a migration, a deleted column, a public API change) or two-way (cheap to revert: additive, flagged, internal) call, with the reason, so the reviewer knows how hard to look before they start rather than discovering it partway through.

## Common questions

**Does it open the PR, or just write the body?**

Both, when it can. It runs `gh pr create --body-file` with the assembled body once `gh` is available and authenticated. Where the workflow calls for a human to open the PR by hand, or `gh` is not set up, it hands over the assembled body instead of the command.

**What about review comments that come back afterward?**

Out of scope. Several people have asked for a skill on the other side of this seam, one that sorts which review comments are worth acting on and which are noise; nothing shipped here does that yet.

**Does this replace `code-review`?**

No. `code-review` checks whether the diff is built right and is the right thing; `pr` writes up a diff that has already passed that bar (or one that hasn't been reviewed yet, if you skip straight here). Running `code-review` first means the evidence section in the PR body has something to report beyond "I wrote it."

## Where it fits

`pr` is the last step of the main build chain, after the diff is committed and reviewed: `grill-with-docs → to-spec → to-tickets → implement → code-review → pr`. It also stands alone whenever a diff already exists and needs a body.

- [code-review](https://aihero.dev/skills-code-review) is the closest neighbour and the step immediately before this one: it checks the diff is right, this skill writes it up for the person about to read it.
- [to-spec](https://aihero.dev/skills-to-spec) and [to-tickets](https://aihero.dev/skills-to-tickets) are usually the primary source this skill's summary is drawn from.

[ask-matt](https://aihero.dev/skills-ask-matt) routes across the whole set when you are unsure which skill the situation wants.
