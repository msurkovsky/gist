# Repository check cases

## check-metadata

Given a Gist source fixture, `gk check` accepts a skill with closed, valid YAML
frontmatter and nonempty string name/description, whose name matches its prefixed
directory. It rejects unclosed or invalid YAML, duplicate keys, missing fields,
wrong field types, and inconsistent explicit-only policies between hosts.

## check-resources

An inline relative Markdown resource link must resolve to an existing path inside
the skill. Missing paths and escaping paths fail; external links and same-document
anchors are not filesystem resources. Fenced and inline code examples are not
resource links. Decode percent-encoded paths after removing the fragment and
before resolving the resource; decoded paths must still stay inside the skill.

## check-repository

Shell syntax errors, skills omitted from README, missing case documents, and ADRs
without a final dated Changelog fail the repository check. Results are sorted and
bounded by `--limit`, with total failures and `truncated` preserving the full verdict.
Operational errors (including absent Git history) fail rather than claim cleanliness.

## check-vendor-tree

Given a registered vendor and its reachable import merge, compare the vendor subtree
with the import's second-parent subtree. Local committed edits fail even when they
arrived through a normal or synthetic PR merge. A clean imported tree passes.
An update imported on a feature branch and merged later uses the newer import.
Missing, ambiguous, or invalid import records fail; no network fetch is performed.
Registry entries accept `name url` (branch defaults to `main`) or `name url branch`,
matching the vendor importer. Other field counts fail.

## check-vendor-worktree

Staged, unstaged, and non-ignored untracked changes under `experimental/` fail.
Ignored untracked files and directories, including editor artifacts, are excluded.
Ignore patterns never excuse changes to tracked vendor files. Tracked or non-ignored
untracked content for unknown vendors fails. A staged edit restored only in the
worktree still fails. Files outside `experimental/` do not affect the vendor-only check.
