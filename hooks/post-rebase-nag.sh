#!/usr/bin/env bash
# Claude Code PostToolUse hook (matcher: Bash). After a `git rebase` or `git pull --rebase`
# completes, remind the agent to re-sync the toolchain before trusting any check.
#
# settings.json:
#   "PostToolUse": [{ "matcher": "Bash",
#     "hooks": [{ "type": "command", "command": "/path/to/gist/hooks/post-rebase-nag.sh" }] }]
#
# Reads the hook JSON from stdin and parses it with node. If node is missing or the input is
# not JSON it exits 1 with a message, so a hook that cannot work is seen, not silent.
set -uo pipefail
fail() { echo "post-rebase-nag: $*" >&2; exit 1; }
command -v node >/dev/null 2>&1 || fail "node not found; it is needed to read the hook input"
cmd="$(node -e '
  let s = ""; process.stdin.on("data", d => s += d).on("end", () => {
    try { const j = JSON.parse(s); process.stdout.write(String((j.tool_input && j.tool_input.command) || "")); }
    catch (e) { process.stderr.write("invalid hook input: " + e.message + "\n"); process.exit(1); }
  });')" || fail "could not read the hook input"
if printf '%s' "$cmd" | grep -q -E 'git (rebase|pull[^|;&]*--rebase)' && ! printf '%s' "$cmd" | grep -q -E -- '--abort|--quit'; then
  cat <<'MSG'
Rebase detected. Before trusting typecheck, lint, or tests:
1. Re-install dependencies (lockfile may have moved).
2. Rebuild generated code; check whether a bumped generated client made a hand-written type obsolete.
3. Run typecheck and the full suite.
Use the gist-toolchain skill for the language-specific commands.
MSG
fi
exit 0
