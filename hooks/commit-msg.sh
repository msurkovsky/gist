#!/usr/bin/env bash
# Forwarder to `gk hook commit-msg`, kept for repos that symlinked this file as
# .git/hooks/commit-msg before `gk hook install` existed. Deleting it would leave
# those links dangling, and git skips a missing hook without a word. Install the
# hook with `gk hook install`; remove this file once no repo links to it.
# See docs/adr/0009-install-git-hooks-with-gk.md.
exec gk hook commit-msg "$@"
