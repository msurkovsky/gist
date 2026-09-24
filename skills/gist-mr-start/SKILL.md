---
name: gist-mr-start
description: Start a branch that will be reviewed. Fixes ticket id, type, branch name, and commit convention before the first commit, and installs the guards that keep them.
disable-model-invocation: true
---

# MR start

A rule deferred at the first commit becomes a pattern the reviewer watches. Settle
everything before the first commit.

## 1. Read the project's conventions

Look in `CLAUDE.md`, `.claude/rules/`, `CONTRIBUTING*`, and the forge config for:

- branch name pattern
- commit message pattern and the allowed types
- base branch (default: the remote HEAD, `git symbolic-ref refs/remotes/origin/HEAD`)
- forge: GitLab if the origin URL contains `gitlab`, GitHub if `github`

Defaults when the project says nothing:

- branch: `<ID>-<subject>` when there is a ticket, else `<subject>`
- commit: short subject (under 50 characters, 72 hard limit), uppercase start, no period,
  blank line, then a body that says why, wrapped at 72

Only when the project defines a typed convention (for example `<type>(<ID>): subject`):

- branch: `<type>/<ID>-<subject>`
- one type for the whole branch
- the pattern is recorded for the hook: `git config mr.commitPattern '<regex>'`

## 2. Ask once

One question: ticket id, and the type if the project uses types. Do not proceed without
what the project requires. Never use a `private/` prefix for work meant to be reviewed.

## 3. Create the branch

```
git fetch origin
git switch -c <branch> origin/<base>
```

Typed convention only:

```
git config branch.<branch>.mrType <type>
```

The `commit-msg` hook from gist enforces the generic style everywhere and the typed
pattern where `mr.commitPattern` is set. Offer to install it if `.git/hooks/commit-msg` is
absent:

```
ln -sf <gist>/hooks/commit-msg.sh .git/hooks/commit-msg
```

## 4. State the rules for this branch

Tell the user, in four lines, what the branch is bound to:

- branch name and base
- commit style, and the fixed type if the project uses one
- after any rebase: run the `gist-toolchain` skill with `install`, `typecheck`, `test`, and check
  whether regenerated code made a hand-written type obsolete
- before opening the MR: run `/gist-mr-ready`

No further prose.
