---
name: gist-doc-review
description: Review the comments and doc strings in a change — cut the ones restating the code, shorten the bloated ones, add the missing line on public API. Use when asked to "review the docs", "check my comments", "am I overdocumenting", "doc review", or before shipping a change that added a lot of prose.
---

# Doc review

## The rules

1. **Brief and easy to follow.**
2. **Only non-obvious behaviour.** If the code says it, leave it out.
3. **Clear private helpers need no comment at all.**
4. **Every public function, type and exported const gets at least a brief line**,
   so it is described wherever the docs are read.

Rules 1–3 delete. Rule 4 adds. Most changes need both directions at once —
a change with too much prose in the internals and none on its public surface
is the normal case, not the exception.

## Procedure

**1. Measure.**

```bash
gk doc --base main --json     # what this branch added
gk doc --json                 # uncommitted, including new files
gk doc --range A..B --json
```

Read `totals.ratio`, `files[]`, and `runs[]` (comment blocks ≥2 lines, longest
first). One-line asides are excluded by default — that is the point.
If the tool or repository is unavailable, identify the missing measurement.
Continue from a supplied diff when sufficient; never invent a ratio or install
tools as a side effect of review. When `gk` is missing, give the user its
install command:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/msurkovsky/gist/releases/latest/download/gist-cli-installer.sh | sh
```

**2. Triage with the runs, not the ratio.**

Read every run of 4+ lines. Spot-check two or three shorter ones. The runs list
is sorted longest-first because length is the cheapest signal of a rule 1 or
rule 2 violation.

**3. Judge each comment, in this order.**

| Ask | If yes |
|---|---|
| Does the code below already say this? | **cut** |
| Is it a private helper with a clear name and signature? | **cut** |
| Is it three sentences where one would do? | **shorten** |
| Does it explain *why*, a constraint, or a consequence? | **keep** |

**4. Check the other direction.**

Find public items in the change with no doc line at all. Per-language patterns:
[references/public-surface.md](references/public-surface.md). Every one of them is a rule 4 finding.

**5. Report.** Do not edit yet. Findings first, fixes when asked.

## Keep vs cut

**Keep** — the reader cannot get this from the code:
- why this approach and not the obvious one
- a constraint imposed from somewhere else: a wire format, an ordering another
  file asserts, a caller that breaks
- what happens if someone changes it
- invariants, units, or ranges the type does not carry

**Cut** — the code already says it:
- restating the signature in prose
- narrating the steps the body performs
- paraphrasing the function name
- history: what it used to do, when it was added
- a block comment on a three-line private helper

## Reading the ratio

Triage only. Never a target, and never something to optimise toward.

| ratio | usual meaning |
|---|---|
| under 5% with new public API | rule 4 is being skipped |
| 10–25% | unremarkable |
| over 35% | read every run; usually rule 1 and 2 |

Rule 4 work does not move the ratio at all: a one-line doc on a public item is
not a run, so `doc` stays flat while `code` grows. That is deliberate. Measure
rule 4 with the detector in `references/public-surface.md`, never with the
number.

Tests skew high and that is often fine — a header explaining what a fixture
proves is non-obvious behaviour. Judge test files separately from source.

## Output

```
doc review · <source> · <ratio>% documentation

cut      src/store.ts:41   (6 lines)  restates the reducer below
shorten  src/api.ts:12     (5 → 1)    one line covers it
add      src/models.ts:88             exported const, no doc line

verdict: <one sentence — what to do before this ships>
```

Order findings by how much they cost a reader: bloat in a public doc first,
private-helper noise last. Cite `file:line` for every one, so the fix is one
jump away. If a change is clean, say so in one line and stop — a doc review
that invents findings to look thorough is itself the failure mode.
