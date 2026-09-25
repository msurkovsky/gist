#!/usr/bin/env bash
# Git commit-msg hook: clear, short subject; blank line; body wrapped at a sane width.
#
# Install into any repo:  ln -sf /path/to/gist/hooks/commit-msg.sh .git/hooks/commit-msg
#
# Default rules (all repos):
#   - subject non-empty, starts with an uppercase letter, no trailing period
#   - subject <= commit.subjectMax chars (default 72); a note is printed above 50
#   - if a body exists, line 2 is blank
#   - body lines <= commit.bodyMax chars (default 72); URLs, trailers, indented lines exempt
#
# Opt-in project convention (only when the project sets it):
#   git config mr.commitPattern '^([a-z]+)\(([A-Za-z]+-?[0-9]+)\): .+'
#     subject must match; capture group 1 is the type
#   git config branch.<name>.mrType <type>    (set by /gist-mr-start when the project uses types)
#     commits on that branch must use that type
#
# Merge, fixup!, squash!, and Revert commits are exempt.
set -euo pipefail
msg_file="$1"
lines=()
while IFS= read -r l || [ -n "$l" ]; do lines+=("$l"); done < <(grep -v '^#' "$msg_file")
subject="${lines[0]:-}"

case "$subject" in
  Merge\ *|fixup!\ *|squash!\ *|Revert\ *) exit 0 ;;
esac

err() { echo "commit-msg: $*" >&2; exit 1; }
# Characters, not bytes, in any locale: a byte locale counts an em-dash as three, so drop
# the UTF-8 continuation bytes (0x80-0xBF) before counting.
len() { printf '%s' "$1" | LC_ALL=C tr -d '\200-\277' | wc -c | tr -d ' '; }
cfg() { git config --get "$1" 2>/dev/null || echo "$2"; }

subject_max="$(cfg commit.subjectMax 72)"
body_max="$(cfg commit.bodyMax 72)"

pattern="$(cfg mr.commitPattern '')"

[ -n "$subject" ] || err "empty subject"
if [ -z "$pattern" ]; then
  # Only when no project pattern governs the subject shape; type prefixes are lowercase.
  [[ "$subject" =~ ^[A-Z] ]] || err "subject must start with an uppercase letter: $subject"
fi
[[ "$subject" != *. ]] || err "subject must not end with a period: $subject"
subject_len="$(len "$subject")"
[ "$subject_len" -le "$subject_max" ] || err "subject is $subject_len chars, max $subject_max: $subject"
[ "$subject_len" -le 50 ] || echo "commit-msg: note: subject is $subject_len chars; under 50 reads better" >&2

if [ "${#lines[@]}" -gt 1 ]; then
  [ -z "${lines[1]}" ] || err "line 2 must be blank (subject, blank line, body)"
  for ((i = 2; i < ${#lines[@]}; i++)); do
    l="${lines[$i]}"
    n="$(len "$l")"
    [ "$n" -le "$body_max" ] && continue
    [[ "$l" =~ https?:// ]] && continue            # URLs
    [[ "$l" =~ ^[A-Za-z-]+:\  ]] && continue        # trailers like Co-Authored-By:
    [[ "$l" =~ ^[[:space:]] ]] && continue          # indented code or quotes
    err "body line $((i + 1)) is $n chars, max $body_max"
  done
fi

if [ -n "$pattern" ]; then
  [[ "$subject" =~ $pattern ]] || err "project requires subject matching $pattern"
  type="${BASH_REMATCH[1]:-}"
  branch="$(git symbolic-ref --short -q HEAD || true)"
  fixed="$(cfg "branch.$branch.mrType" '')"
  if [ -n "$type" ] && [ -n "$fixed" ] && [ "$type" != "$fixed" ]; then
    err "branch $branch is fixed to type '$fixed', got '$type'; one type per branch"
  fi
fi
