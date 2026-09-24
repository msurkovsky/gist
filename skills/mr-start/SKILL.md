---
name: mr-start
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

- branch: `<type>/<ID>-<subject>`
- commit: `<type>(<ID>): <subject>`, one type for the whole branch

## 2. Ask once

One question with two answers: ticket id and type. Do not proceed without both. Never use a
`private/` prefix for work meant to be reviewed.

## 3. Create the branch

```
git fetch origin
git switch -c <type>/<ID>-<subject> origin/<base>
git config branch.<type>/<ID>-<subject>.mrType <type>
```

The `mrType` config fixes the type for this branch. The `commit-msg` hook from my-skills
rejects any other type. Offer to install the hook if `.git/hooks/commit-msg` is absent:

```
ln -sf <my-skills>/hooks/commit-msg.sh .git/hooks/commit-msg
```

## 4. State the rules for this branch

Tell the user, in four lines, what the branch is bound to:

- branch name and base
- commit format and the fixed type
- after any rebase: run the `toolchain` skill with `install`, `typecheck`, `test`, and check
  whether regenerated code made a hand-written type obsolete
- before opening the MR: run `/mr-ready`

No further prose.
