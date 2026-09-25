#!/usr/bin/env bash
# Repository consistency check. Run before every commit (`just check`, part of `just ci`).
# Exits non-zero on any failure.
#
#   scripts/check.sh
#
# Checks:
#   1. shell syntax of scripts/*.sh and hooks/*.sh
#   2. every skills/*/SKILL.md has frontmatter, name equals directory, has a description
#   3. every skill directory carries the gist- prefix (docs/adr/0003)
#   4. every skill is listed in README.md
#   5. no local edits under experimental/ (scripts/vendor.sh check)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
fail=0
ok()   { printf 'ok    %s\n' "$*"; }
bad()  { printf 'FAIL  %s\n' "$*"; fail=1; }

# 1
for f in scripts/*.sh hooks/*.sh; do bash -n "$f" && ok "syntax $f" || bad "syntax $f"; done

# 2 + 3
for f in skills/*/SKILL.md; do
  d="$(basename "$(dirname "$f")")"
  [ "$(sed -n '1p' "$f")" = "---" ] || { bad "$f: no frontmatter"; continue; }
  n="$(awk 'NR>1&&/^---$/{exit} /^name:/{sub(/^name:[ ]*/,"");print}' "$f")"
  [ "$n" = "$d" ] && ok "$f name=$n" || bad "$f: name '$n' != dir '$d'"
  awk 'NR>1&&/^---$/{exit} /^description:[ ]*[^ ]/{found=1} END{exit !found}' "$f" || bad "$f: missing description"
  case "$d" in gist-*) ;; *) bad "$f: skill directories are prefixed gist- (docs/adr/0003)";; esac
done

# 4
for d in skills/*/; do
  n="$(basename "$d")"
  grep -qE "\`/?$n\`" README.md && ok "README lists $n" || bad "README.md does not list skill $n"
done

# 5
if [ -x scripts/vendor.sh ] && [ -d experimental ]; then
  out="$(scripts/vendor.sh check)"
  [ "$out" = "clean: no local edits under experimental/" ] && ok "experimental/ untouched" || { bad "local edits under experimental/:"; printf '%s\n' "$out" | sed 's/^/      /'; }
fi

[ $fail -eq 0 ] && echo "all checks passed" || { echo "checks failed"; exit 1; }
