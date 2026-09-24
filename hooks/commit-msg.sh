#!/usr/bin/env bash
# Git commit-msg hook: enforce "<type>(<ID>): <subject>" and one type per branch.
#
# Install into a project:  ln -sf /path/to/my-skills/hooks/commit-msg.sh .git/hooks/commit-msg
#
# Configuration (git config, project-local):
#   mr.commitPattern   regex the first line must match; group 1 is the type.
#                      default: ^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+
#   branch.<name>.mrType   type fixed for this branch (set by /mr-start). If set, a
#                          commit with another type is rejected.
# Merge commits and fixup!/squash! commits are exempt.
set -euo pipefail
msg_file="$1"
first="$(grep -v '^#' "$msg_file" | sed -n '1p')"
case "$first" in
  Merge\ *|fixup!\ *|squash!\ *|Revert\ *) exit 0 ;;
esac
pattern="$(git config --get mr.commitPattern || echo '^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+')"
if ! [[ "$first" =~ $pattern ]]; then
  echo "commit-msg: first line must match $pattern" >&2
  echo "commit-msg: got: $first" >&2
  exit 1
fi
type="${BASH_REMATCH[1]}"
branch="$(git symbolic-ref --short -q HEAD || true)"
if [ -n "$branch" ]; then
  fixed="$(git config --get "branch.$branch.mrType" || true)"
  if [ -n "$fixed" ] && [ "$type" != "$fixed" ]; then
    echo "commit-msg: branch $branch is fixed to type '$fixed', got '$type'" >&2
    echo "commit-msg: one type per branch; split into another branch or use '$fixed'" >&2
    exit 1
  fi
fi
