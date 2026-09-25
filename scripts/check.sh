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
#   4. no language tool names under skills/ (language commands belong in languages/)
#   5. every languages/<lang>/toolchain.md has Detect and Verbs sections
#   6. every skill is listed in README.md
#   7. no local edits under experimental/ (scripts/vendor.sh check)
#   8. hook smoke tests
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

# 4  language tool names must not appear in skills/ (word-bounded, case-sensitive).
# Boundaries are spelled out: \b is a GNU extension that BSD grep on macOS lacks.
langtools='(^|[^A-Za-z0-9_])(npm|npx|pnpm|yarn|node_modules|tsc|eslint|vitest|jest|cargo|rustc|clippy|pytest|pip|uv sync|poetry|mypy|ruff|go test|go build|go vet|golangci|mvn|gradle|dotnet|ctest|cmake|clang-tidy|gcov|lcov\.info)($|[^A-Za-z0-9_])'
hits="$(grep -rnE "$langtools" skills/ || true)"
[ -z "$hits" ] && ok "skills/ free of language tool names" || { bad "language tool names under skills/ (move to languages/):"; printf '%s\n' "$hits" | sed 's/^/      /'; }

# 5
for f in languages/*/toolchain.md; do
  grep -q '^## Detect' "$f" && grep -q '^## Verbs' "$f" && ok "$f sections" || bad "$f: needs '## Detect' and '## Verbs'"
done

# 6
for d in skills/*/; do
  n="$(basename "$d")"
  grep -qE "\`/?$n\`" README.md && ok "README lists $n" || bad "README.md does not list skill $n"
done

# 7
if [ -x scripts/vendor.sh ] && [ -d experimental ]; then
  out="$(scripts/vendor.sh check)"
  [ "$out" = "clean: no local edits under experimental/" ] && ok "experimental/ untouched" || { bad "local edits under experimental/:"; printf '%s\n' "$out" | sed 's/^/      /'; }
fi

# 8  hook smoke tests
t="$(mktemp)"
printf 'Add thing\n\nBecause it was missing.\n' > "$t"; hooks/commit-msg.sh "$t" 2>/dev/null && ok "commit-msg accepts generic style" || bad "commit-msg rejects generic style"
printf 'add thing.\n' > "$t"; hooks/commit-msg.sh "$t" 2>/dev/null && bad "commit-msg accepts lowercase with period" || ok "commit-msg rejects lowercase with period"
printf 'Add thing\nno blank line\n' > "$t"; hooks/commit-msg.sh "$t" 2>/dev/null && bad "commit-msg accepts missing blank line" || ok "commit-msg rejects missing blank line"
printf 'feat(ABC-1): add thing\n' > "$t"
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=mr.commitPattern GIT_CONFIG_VALUE_0='^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+' hooks/commit-msg.sh "$t" 2>/dev/null && ok "commit-msg honours opt-in project pattern" || bad "commit-msg rejects opt-in pattern match"
rm -f "$t"
echo '{"tool_input":{"command":"git pull --rebase origin main"}}' | hooks/post-rebase-nag.sh | grep -q 'Rebase detected' && ok "post-rebase-nag fires" || bad "post-rebase-nag silent on rebase"
[ -z "$(echo '{"tool_input":{"command":"git rebase --abort"}}' | hooks/post-rebase-nag.sh)" ] && ok "post-rebase-nag silent on abort" || bad "post-rebase-nag fires on abort"
echo 'not json' | hooks/post-rebase-nag.sh >/dev/null 2>&1 && bad "post-rebase-nag swallows unparseable input" || ok "post-rebase-nag fails on unparseable input"
case "$(echo '{}' | env PATH=/nonexistent "$BASH" hooks/post-rebase-nag.sh 2>&1)" in
  *node*) ok "post-rebase-nag says so when node is missing" ;;
  *) bad "post-rebase-nag is silent when node is missing" ;;
esac

[ $fail -eq 0 ] && echo "all checks passed" || { echo "checks failed"; exit 1; }
