#!/usr/bin/env bash
# Vendor third-party repositories into experimental/<name> with full history, via Josh.
#
# Import is a reversible history filter: ':prefix=experimental/<name>' applied to the
# upstream branch. Re-running on a moved upstream yields the same commits for the shared
# part of history, so repeated updates merge cleanly and never duplicate commits.
#
#   tools/vendor.sh add <name> <url> [branch]   register and import
#   tools/vendor.sh update [name...]            re-import registered vendors (default: all)
#   tools/vendor.sh list                        show registry with local/upstream state
#   tools/vendor.sh check                       list commits that touched experimental/ outside imports
#
# Registry: tools/vendors.conf, one "name url branch" per line.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONF="$ROOT/tools/vendors.conf"
cd "$ROOT"

die() { echo "error: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing binary: $1 (see README, section Josh)"; }

import_one() {
  local name="$1" url="$2" branch="$3"
  local prefix="experimental/$name"
  need josh-filter
  [ -z "$(git status --porcelain)" ] || die "working tree not clean; commit or stash first"

  echo "== $name <- $url ($branch)"
  git fetch -q "$url" "$branch"
  local up; up="$(git rev-parse FETCH_HEAD)"
  josh-filter ":prefix=$prefix" FETCH_HEAD >/dev/null
  local filtered; filtered="$(git rev-parse FILTERED_HEAD)"

  if git merge-base --is-ancestor "$filtered" HEAD 2>/dev/null; then
    echo "   up to date at ${up:0:7}"
    return 0
  fi

  local msg extra=()
  if git ls-tree -d HEAD "$prefix" >/dev/null 2>&1 && [ -n "$(git ls-tree -d HEAD "$prefix")" ]; then
    msg="Update $name to ${up:0:7}"
  else
    msg="Import $name at ${up:0:7}"
    extra+=(--allow-unrelated-histories)
  fi
  git merge --no-ff "${extra[@]}" -m "$msg" -m "Upstream: $url@$up" -m "Filter: :prefix=$prefix" FILTERED_HEAD
  echo "   merged $(git rev-list --count HEAD^..HEAD^2 2>/dev/null || echo '?') commits"
}

cmd_add() {
  local name="${1:-}" url="${2:-}" branch="${3:-main}"
  [ -n "$name" ] && [ -n "$url" ] || die "usage: vendor.sh add <name> <url> [branch]"
  [[ "$name" =~ ^[a-z0-9._-]+$ ]] || die "name must be [a-z0-9._-]"
  touch "$CONF"
  grep -q "^$name " "$CONF" && die "$name already registered"
  echo "$name $url $branch" >> "$CONF"
  import_one "$name" "$url" "$branch"
  git add "$CONF" && git commit -q -m "Register vendor $name" && echo "   registered in tools/vendors.conf"
}

cmd_update() {
  [ -f "$CONF" ] || die "no vendors registered"
  local want=("$@") name url branch
  while read -r name url branch; do
    [ -z "$name" ] || [[ "$name" == \#* ]] && continue
    if [ ${#want[@]} -gt 0 ]; then
      printf '%s\n' "${want[@]}" | grep -qx "$name" || continue
    fi
    import_one "$name" "$url" "${branch:-main}"
  done < "$CONF"
}

cmd_list() {
  [ -f "$CONF" ] || { echo "no vendors registered"; return 0; }
  local name url branch
  while read -r name url branch; do
    [ -z "$name" ] || [[ "$name" == \#* ]] && continue
    local local_sha
    local_sha="$(git log -1 --format=%b -- "experimental/$name" 2>/dev/null | sed -n 's/^Upstream: .*@//p' | head -1)"
    echo "$name  $url  $branch  local:${local_sha:0:7}"
  done < "$CONF"
}

cmd_check() {
  # Any commit touching experimental/ that is not an import/update merge is a local edit
  # of vendored content, which will conflict on the next update. Adopt into skills/ instead.
  git log --format='%h %s' -- experimental/ | grep -v -E ' (Import|Update) [a-z0-9._-]+ (at|to) [0-9a-f]{7}$' || echo "clean: no local edits under experimental/"
}

case "${1:-}" in
  add) shift; cmd_add "$@" ;;
  update) shift; cmd_update "$@" ;;
  list) cmd_list ;;
  check) cmd_check ;;
  *) sed -n '2,13p' "$0"; exit 1 ;;
esac
