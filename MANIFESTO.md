# Manifesto

*Understand enough to steer.*

This repo assumes the model writes the code and the human keeps the call. That
assumption is not neutral. It changes what a project owes a contributor and what
a contributor owes the project, and the skills here are built on the change.
This file states it plainly so you can decide whether the rest is for you.

If you disagree with the premise, the rest will read as ceremony. That is fine.
It is not for you.

## Scope

This governs this repo. It is not a position on AI in general, not advice for
anyone else's project, and not a claim that the trade-offs here transfer.

It is the root. `CLAUDE.md`, `rules/`, the skills, and the hooks derive from
it: this file says why, `CLAUDE.md` and `rules/` say what, hooks enforce what
cannot be allowed to slip. Where they disagree, the lower one is wrong.

It describes the repo as it is meant to be, and the repo is held to it. Where
the repo falls short, `TODO.md` names the gap; the gap is a defect in the repo,
not a loosening of this document.

One folder is exempt. `experimental/` holds third-party skills vendored to be
tried, and trying needs room to not yet know. Nothing there owes an ADR, a
rule, or a test. The debt comes due at adoption: an adopted skill ships with
`gk init` and enters the regime below in full. The exemption is itself written
down — `CLAUDE.md` says what the folder is for and how things leave it,
`docs/adr/0004` says why it is built the way it is.

The shape is borrowed from the Rust project's LLM policy, which also governs one
repository and refuses to speak for a wider community. The conclusions differ:
it asks who wrote a line, this document does not. One thing is shared. Both
hold the human who submits a change accountable for it. Principle 4 says what
that means here.

## 1. Provenance is not the question

We do not ask who wrote a line. A rule about authorship cannot be checked, so it
degrades into self-disclosure, then into suspicion, then into a clause about not
harassing people over suspicion.

Ask instead what the text is for, and who has to trust it.

## 2. Prose is the source. Code is the build.

Before compilers, people read the assembly. Then the compiler earned a spec,
determinism, and millions of users a day, and nobody reads object code now — not
because it is long, because trust was delegated to a translator that earned it.

The model is the next translator. Its source is prose in a natural language:
the architecture, the design, the test cases, the decisions. Its output is
code. It has neither a spec nor determinism, so the delegation is not automatic.
It is bought, not declared — bought with checks derived from the source: tests
built from the test cases, types, lints, invariants at the boundaries.

We are where 1970 was with assembly. People still read the output. Less each
year, and only where they choose to.

## 3. Review moves up

Reading every line at the speed a model writes them does not scale. It cannot
move to the tests either; tests are code. It moves up, to the prose.

The order is fixed. A problem is stated. A plan is written and a human judges
it. The plan becomes a change to the docs — architecture, design, test cases,
decisions — and a human reads that change: what it said before, what it says
now, whether it makes sense. Only then is code written, and the checks run
against it: the tooling, and a coherence check that the code still says what
the docs say.

The human reads prose twice and code as much as they want to, which may be
never. That is the whole shift.

A plan expires when its branch merges. By then the docs, the test cases, and
the ADRs carry everything it said. A plan that survives merge is a second
source of truth, and there is only one.

The review is driven by the human, not run for them. They come with the docs
and their own picture, and they ask: is this code aligned with that section,
show me what changed, does this hold. A tool answers. The human judges the
answer against what they know. A tool that grades itself is not a reviewer.

## 4. Reading is a tool, not a duty

You may read any line. You need read none — if you can say why you are
confident the software does what you intend.

Confidence is a claim you can point at: the docs you approved, the test cases
you approved, the coherence answers you asked for, the lines you chose to
read. An author who cannot name what vouched has not delegated. They have
skipped.

Reading, when you do it, is a probe. Pick the aspect that matters to you and
check that. It is not coverage and nobody should mistake it for coverage.

Delegation changes what the author reads, not what the author owns. The author
is the human who opens the change; the model is not a party to this document. A
change that shipped green and broke is theirs, read or unread.

Some areas warrant low confidence by default — code that deletes, formats
written to disk and read back later, input from outside this repo, anything
whose cost is not reversible. Those are declared per area, so a hook can say
so when a change touches one. It is a hint about where to probe, not a gate.
When a probe catches something, the area it caught it in is declared.

## 5. Test cases are the spec

If lines are not the review target, something has to make "does what I intend"
checkable. That is the test cases: prose, one behaviour each, at the boundaries
that matter, approved by the human at the docs gate. The tests are their build.
The model writes them, each names the case it implements, and each is proven
red once.

This is where the bar cannot slip. Slop cases produce a green pipeline that
vouches for nothing, silently. Hence `rules/code-and-comments.md`: a test that
passes with the guarded line deleted is not a test, and every new test is
proven red at least once.

The cutoff is the same as for comments. Public behaviour and boundaries get
cases. A clear private helper gets none.

## 6. No hidden knowledge

The bare minimum for participation is not the code. It is the architecture, the
decisions, and the trade-offs, and those live beside the code. In it they are
rarely recorded and never surfaced. Some projects record them well. Where they
are not recorded, they move by conversation instead, which is most of what
onboarding actually is.

Conversation is the channel this repo refuses to depend on. It does not scale
past the people in the room, and it leaves nothing an agent can read.

What makes tribal knowledge tribal is negative space. How it works is readable.
What is not readable: why not the obvious alternative, what was tried and
failed, what this must never do. Code records the decision it embodies and
nothing surfaces the ones it rejected. Hence the rule for `docs/adr/`: add one
when the rejected options were real.

The source has altitudes. Conceptual: the system and its parts, as a diagram
kept as text so it diffs and a tool can read it. Architecture: the shape and
why. Design: how each part does its job. Test cases: what each part must do at
its edges. Decisions, alongside: what was rejected and why. Levels, not files —
a small repo puts the top three on one page. What matters is that each altitude
exists and can be found.

A model fails on this repo for the same reason a new hire does, and mostly on
the same question — changing an architecture it was never told the shape of.
Shape lives at the top altitudes. ADRs record what changed it; they are not
where it lives.

## 7. Three detectors

"No hidden knowledge" is unfalsifiable on its own. Absence cannot be grepped.
It needs events, and there are three, all cheap and all frequent:

1. An agent asks a question answered from your head. That answer was a missing
   artifact.
2. You correct an agent's architectural move. The constraint it broke was
   unwritten.
3. The coherence check finds prose and code disagreeing. Either the code
   drifted and is rebuilt from the prose, or the prose was incomplete and the
   artifact is written. Prose wins. The answer is never "read more lines".

Each one is a leak with a location. Write the artifact, not the reply. A
constraint that leaks twice graduates — to a rule, then to a hook, the same
escalation `CLAUDE.md` applies to rules. A fact that leaks stays an artifact;
it just has to be reachable, which is principle 8.

The agent is therefore the onboarding test. Human onboarding is rare, the
newcomer hides confusion for social reasons, and the gaps seldom come back as
signal. Agent onboarding runs every session, on cold context, at no social
cost, and when it guesses, the guess lands in a diff you can read. Fix the
knowledge base for the agent and human onboarding improves as a byproduct.

## 8. Compression and accessibility

There is no free lunch. The system has to be described somewhere. The only
questions are where, how many times, and whether the description arrives when
it is needed.

Once, at its altitude. Architecture to design to test cases to tests to code is
refinement, not repetition: each level says one thing the level above did not.
Two hundred ADRs nobody can hold are hidden knowledge again, hidden by volume.

And reachable at the decision. `docs/` is storage; retrieval is what makes it
knowledge. `CLAUDE.md` points from each rule to the decision behind it and loads
every session. `README.md` points from the layout. A module's doc comment
points at the section that sources it — `init.rs` already does this for its
ADRs, and the coherence check walks those same links to know which prose to
hold which code against. A skill surfaces a decision at the step where it
applies and keeps its own long material in `references/`, loaded on demand and
not before. A decision no pointer reaches is tribal knowledge with extra steps.

## What participation means

Four things, in the order the work runs:

- state a problem so it survives contact with an implementer
- judge a plan, and catch the wrong decomposition before it is built
- hold the source honest: read the docs change, before and after, and say
  whether it still makes sense
- know when a green pipeline is not enough, and know what to ask

The last is the scarce one, and it is what this repo exists to transfer: skills
carry how we decide, rules carry what never, hooks carry what cannot be allowed
to slip.

## What this is not

Not a claim that models are good enough. Not a claim that review is obsolete —
it moved up. Not a ban on reading code: read what matters to you, and say what
you were checking. Not a licence to ship what you cannot vouch for: principle 4
says reading is optional, and nothing here says owning is.
