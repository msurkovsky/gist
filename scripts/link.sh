#!/usr/bin/env bash
# Symlink a skill directory into a harness skill directory for local use.
#
#   scripts/link.sh <skill-dir> [as-name] [dest-dir]
#
# Examples:
#   scripts/link.sh skills/gist-outline
#   scripts/link.sh experimental/mattpocock/skills/engineering/tdd matt-tdd
#   scripts/link.sh skills/gist-doc-review gist-doc-review ~/projects/frontend/.claude/skills
#
# Default dest is ~/.claude/skills. Views (Josh workspaces) are the preferred way to
# project skills into projects; this is the quick path for trying one skill on this machine.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
src="$ROOT/${1:?usage: link.sh <skill-dir> [as-name] [dest-dir]}"
[ -f "$src/SKILL.md" ] || { echo "error: $src has no SKILL.md" >&2; exit 1; }
name="${2:-$(basename "$src")}"
dest="${3:-$HOME/.claude/skills}"
mkdir -p "$dest"
target="$dest/$name"
if [ -e "$target" ] && [ ! -L "$target" ]; then
  echo "error: $target exists and is not a symlink; refusing to replace" >&2; exit 1
fi
ln -sfn "$src" "$target"
echo "linked $target -> $src"
