#!/usr/bin/env bash
# Repository consistency check. Run before every commit. Exits non-zero on any failure.
#
#   tools/check.sh
#
# Checks:
#   1. shell syntax of tools/*.sh and hooks/*.sh
#   2. every skills/*/SKILL.md has frontmatter, name equals directory, has a description
#   3. no language tool names under skills/ (language commands belong in languages/)
#   4. every languages/<lang>/toolchain.md has Detect and Verbs sections
#   5. every skill is listed in README.md
#   6. no local edits under experimental/ (tools/vendor.sh check)
#   7. no em-dashes in prose we own
#   8. hook smoke tests
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
fail=0
ok()   { printf 'ok    %s\n' "$*"; }
bad()  { printf 'FAIL  %s\n' "$*"; fail=1; }

# 1
for f in tools/*.sh hooks/*.sh; do bash -n "$f" && ok "syntax $f" || bad "syntax $f"; done

# 2
for f in skills/*/SKILL.md; do
  d="$(basename "$(dirname "$f")")"
  [ "$(sed -n '1p' "$f")" = "---" ] || { bad "$f: no frontmatter"; continue; }
  n="$(awk 'NR>1&&/^---$/{exit} /^name:/{sub(/^name:[ ]*/,"");print}' "$f")"
  [ "$n" = "$d" ] && ok "$f name=$n" || bad "$f: name '$n' != dir '$d'"
  awk 'NR>1&&/^---$/{exit} /^description:[ ]*[^ ]/{found=1} END{exit !found}' "$f" || bad "$f: missing description"
done

# 3  language tool names must not appear in skills/ (word-bounded, case-sensitive)
langtools='\b(npm|npx|pnpm|yarn|node_modules|tsc|eslint|vitest|jest|cargo|rustc|clippy|pytest|pip|uv sync|poetry|mypy|ruff|go test|go build|go vet|golangci|mvn|gradle|dotnet|ctest|cmake|clang-tidy|gcov|lcov\.info)\b'
hits="$(grep -rnE "$langtools" skills/ || true)"
[ -z "$hits" ] && ok "skills/ free of language tool names" || { bad "language tool names under skills/ (move to languages/):"; printf '%s\n' "$hits" | sed 's/^/      /'; }

# 4
for f in languages/*/toolchain.md; do
  grep -q '^## Detect' "$f" && grep -q '^## Verbs' "$f" && ok "$f sections" || bad "$f: needs '## Detect' and '## Verbs'"
done

# 5
for d in skills/*/; do
  n="$(basename "$d")"
  grep -qE "\`/?$n\`" README.md && ok "README lists $n" || bad "README.md does not list skill $n"
done

# 6
if [ -x tools/vendor.sh ] && [ -d experimental ]; then
  out="$(tools/vendor.sh check)"
  [ "$out" = "clean: no local edits under experimental/" ] && ok "experimental/ untouched" || { bad "local edits under experimental/:"; printf '%s\n' "$out" | sed 's/^/      /'; }
fi

# 7  em-dashes in owned prose
dash="$(grep -rn -- '—' README.md CLAUDE.md CONTRIBUTING.md rules skills languages views 2>/dev/null || true)"
[ -z "$dash" ] && ok "no em-dashes in owned prose" || { bad "em-dashes found:"; printf '%s\n' "$dash" | sed 's/^/      /'; }

# 8  hook smoke tests
t="$(mktemp)"; printf 'feat(ABC-123): x\n' > "$t"; hooks/commit-msg.sh "$t" 2>/dev/null && ok "commit-msg accepts valid" || bad "commit-msg rejects valid message"
printf 'added thing\n' > "$t"; hooks/commit-msg.sh "$t" 2>/dev/null && bad "commit-msg accepts invalid" || ok "commit-msg rejects invalid"
rm -f "$t"
echo '{"tool_input":{"command":"git pull --rebase origin main"}}' | hooks/post-rebase-nag.sh | grep -q 'Rebase detected' && ok "post-rebase-nag fires" || bad "post-rebase-nag silent on rebase"
[ -z "$(echo '{"tool_input":{"command":"git rebase --abort"}}' | hooks/post-rebase-nag.sh)" ] && ok "post-rebase-nag silent on abort" || bad "post-rebase-nag fires on abort"

[ $fail -eq 0 ] && echo "all checks passed" || { echo "checks failed"; exit 1; }
