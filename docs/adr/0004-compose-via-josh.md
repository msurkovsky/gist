# 4. Compose third-party skills with Josh

Accepted — 2026-09-24. Partly superseded by `docs/adr/0005`: `gk init
--experimental=<package>` installs an `experimental/` tree's skills, so a view is no
longer the only way to try one in a project.

## Context

Skills worth trying live in other people's repositories; the first is
mattpocock/skills. The lifecycle is: include verbatim, try, adopt a copy into
`skills/`, adapt the copy freely, and keep taking upstream changes on the
vendored original for as long as they are useful. The upstream history should
survive the import so `git log` and `git blame` under `experimental/` answer
questions about the original, re-importing a moved upstream must not duplicate
commits or conflict with itself, and the files must be in this repo's tree
because `include_dir!` and `grep` need them there.

## Options

### Submodules

Dismissed. A pointer, not content: the files are absent from this tree until a
second clone step, `include_dir!` cannot see them, and every consumer of a view
would need the submodule too. Editing across the boundary is a two-repo
operation.

### `git subtree`

Dismissed, narrowly. It vendors content with history and is the closest
alternative. Its `pull` uses the subtree merge strategy, which guesses the
directory mapping per merge; its `split` rebuilds that mapping by walking the
whole history each run. The mapping is a heuristic, not a declared filter, so
an upstream rename or a second contributor updating the same vendor can produce
a history that no longer merges cleanly.

### Plain copy without history

Dismissed. Cheap, but updating means diffing two trees by hand, and the question
"what did upstream change since we adopted this" has no answer in git.

### Josh `:prefix=` import (chosen)

`josh-filter ':prefix=experimental/<vendor>' FETCH_HEAD` rewrites the upstream
branch under a directory. The filter is declared, deterministic, and cached: the
same upstream commit yields the same filtered commit on every machine, so a
re-import is an ordinary merge of only the new upstream commits and never
duplicates what is already there. It is reversible: `:/experimental/<vendor>`
applied to this repo yields the upstream hashes back. Verified on 2026-09-24:
a re-import after the upstream moved merged cleanly, and extraction reproduced
the upstream SHA.

## Decision

Vendor into `experimental/<vendor>/` only through `scripts/vendor.sh` (`just
vendor`), which registers the vendor in `scripts/vendors.conf`, imports with
`:prefix=`, and merges with a message naming the upstream URL and SHA. Nothing
under `experimental/` is edited here; adoption is a copy into
`skills/gist-<name>/` with a `source:` frontmatter line naming the vendored path
and upstream commit.

The same machinery gives consumers projections: `views/<consumer>/workspace.josh`
maps flat skill names onto paths of this repo and is cloned with
`josh clone ':workspace=views/<consumer>'`, with `josh push` writing edits back.
This is the only way to try a skill from `experimental/` in a project, since
`gk init --claude` vendors `skills/` alone.

## Consequences

Contributors need `josh` and `josh-filter` on PATH, built from source, only to
import or update a vendor or to clone a view; everyone else sees ordinary git.
Upstream history enters this repo's object database in full: mattpocock/skills
added 472 commits. The first `josh-filter` run on a machine recomputes the
filter cache in `.git/josh/cache`; deleting it costs only time. A view must be
committed here before it can be cloned; cloning a missing path yields an
unrelated empty root whose push is rejected. `josh clone` still fetches the
whole object database; only josh-proxy offers partial transfer, and nothing here
depends on that.

## Verification

`scripts/check.sh` fails when `scripts/vendor.sh check` finds a non-merge
first-parent commit touching `experimental/`. `just vendor update` on an
unchanged upstream reports `up to date` and creates no commit.
