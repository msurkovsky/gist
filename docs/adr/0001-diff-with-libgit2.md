# 1. Diff with libgit2

Accepted — 2026-09-23. Implemented in `b9ea9a0`. The scope is `gk doc`; `gk hook`
runs `git` itself, for the reasons in `docs/adr/0009`.

## Context

`gk doc` measures how much of a change is documentation. To do that it needs,
for every changed file, **the added lines with their line numbers in the
post-image** — the numbers matter, because block comment state cannot be
recovered from a `-U0` diff alone, so every added line is classified against
the whole file it lands in, not against the diff text.

It needs that for four sources: the worktree (including untracked files), the
index, an arbitrary revision range, and the merge base with a branch. And for
three of those it needs the post-image content itself, which does not exist on
disk.

The first implementation shelled out to `git` and parsed the unified diff by
hand: `git diff --unified=0`, `git show <rev>:<path>`, `git ls-files --others`,
`git rev-parse --show-toplevel`, plus ~80 lines parsing `@@` headers, `+++
/dev/null`, and git's path quoting. It worked and was well tested. The question
was whether a crate could carry it.

## Options

### Parse the diff text with a crate — `patch`, `unidiff`

`patch` (0.7, 1.2M downloads) and `unidiff` (0.4, 916k) both parse unified
diffs into hunks and lines.

Dismissed. They solve the half of the problem that was already cheap. Hunk
headers are about forty lines to parse; the expensive half is everything
around the diff — four ways to invoke it, post-images for three of them,
untracked files, repository discovery. A text parser leaves all of that
shelling out to `git`, so the subprocess layer survives and the dependency
buys one deleted function.

### Pure-Rust git — `gix`

`gix` (0.87, 48M downloads) is a full git implementation with no C in it.

Dismissed, with regret — the absent C dependency is the one thing it clearly
wins on. But per-line additions are not a first-class API there: it means
wiring `gix-diff::blob` to imara-diff and assembling the line records, and the
worktree-status side is younger than the rest. That is more code written to
delete code, which inverts the point of the exercise. Worth revisiting if the
C dependency ever becomes a problem or the API rises to the task.

### Keep the hand-rolled parser

Dismissed. It worked, but every git behaviour it did not implement was a
silent wrong answer rather than an error — renames counted as whole new files,
and nothing in the design would ever have surfaced that.

### libgit2 bindings — `git2` (chosen)

`git2` (0.21, 115M downloads) hands back a diff callback carrying the origin
character and `new_lineno()` per line — precisely the input the metric wants —
plus the object database for post-images, `merge_base`, and a `revparse` that
already understands `a..b` and `a...b`.

## Decision

Use `git2`, with `default-features = false`.

One dependency covers the diff, all four sources, and the post-image lookup.
It removes the unified-diff parser, the hunk arithmetic, the path unquoting,
the untracked-files pass, and every `Command::new("git")` in the crate.

Turning off default features drops https and ssh, so no OpenSSL: this tool
only ever reads local repositories. The cost was measured before committing —
15 crates, 8.7s clean build.

## Consequences

**libgit2 is C.** `libgit2-sys` vendors and compiles it. No cmake, only `cc`,
but it is C in the dependency tree and in the build time.

**libgit2's defaults are not git's**, and the differences are silent. Two were
found by building the old binary in a worktree and running both over the same
ranges of a real repository:

- `indent_heuristic` has been on in git since 2.14 and is off in libgit2. Left
  off, a hunk anchors a line early, a `/**` opener falls outside the change,
  and one documentation run becomes two.
- Rename detection is on by default for `git diff`. libgit2 needs it asked for,
  and untracked files must be opted in as rename targets separately.

A diff-text parser fed by `git diff` would have inherited both for free. This
is the real price of the choice, and it argues for keeping the old-versus-new
parity run in mind for any future change to this layer.

**Rename similarity still differs.** libgit2 scores it its own way and pairs a
few files git's 50% threshold leaves alone. Kept deliberately: for this metric
a moved file reading as 500 new lines is worse than counting the lines that
actually changed. Measured at 0.0001 of the ratio over 600 commits of a real
repository.

## Verification

Old and new binaries, same ranges, same repository:

| range | old | new |
|-------|-----|-----|
| `HEAD~10..HEAD` | 123 doc / 198 code | identical |
| `HEAD~30...HEAD` | 199 / 2331 | identical |
| `HEAD~600..HEAD` | ratio 0.04099 | 0.04110 |
