#!/usr/bin/env bash
# Claude Code PostToolUse hook (matcher: Bash). After a `git rebase` or `git pull --rebase`
# completes, remind the agent to re-sync the toolchain before trusting any check.
#
# settings.json:
#   "PostToolUse": [{ "matcher": "Bash",
#     "hooks": [{ "type": "command", "command": "/path/to/my-skills/hooks/post-rebase-nag.sh" }] }]
#
# Reads the hook JSON from stdin. Uses node for parsing (present wherever Claude Code runs).
set -uo pipefail
cmd="$(node -e '
  let s = ""; process.stdin.on("data", d => s += d).on("end", () => {
    try { const j = JSON.parse(s); process.stdout.write(String((j.tool_input && j.tool_input.command) || "")); }
    catch (e) { process.stdout.write(""); }
  });' 2>/dev/null)"
if printf '%s' "$cmd" | grep -q -E 'git (rebase|pull[^|;&]*--rebase)' && ! printf '%s' "$cmd" | grep -q -E -- '--abort|--quit'; then
  cat <<'MSG'
Rebase detected. Before trusting typecheck, lint, or tests:
1. Re-install dependencies (lockfile may have moved).
2. Rebuild generated code; check whether a bumped generated client made a hand-written type obsolete.
3. Run typecheck and the full suite.
Use the toolchain skill for the language-specific commands.
MSG
fi
exit 0
