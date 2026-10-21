#!/usr/bin/env bash
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
umask 077
cd "$HOME/Documents/Coding/binus-cli" || exit 1
exec > .agent/credential-size.log 2>&1
finish() {
  local rc=$?
  trap - EXIT
  printf '%s\n' "$rc" > .agent/credential-size.exit
  tmux wait-for -S binus-credential-size-done || true
  exit "$rc"
}
trap finish EXIT
codex exec --model gpt-6-luna --sandbox workspace-write -c sandbox_workspace_write.network_access=true --color never -o .agent/credential-size-summary.md - < .agent/credential-size-task.md
exit "$?"
