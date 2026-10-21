#!/usr/bin/env bash
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
umask 077
cd "$HOME/Documents/Coding/binus-cli" || exit 1
exec > .agent/windows-store-impl.log 2>&1
finish() {
  local rc=$?
  trap - EXIT
  printf '%s\n' "$rc" > .agent/windows-store-impl.exit
  tmux wait-for -S binus-windows-store-impl-done || true
  exit "$rc"
}
trap finish EXIT
codex exec --model gpt-6-luna --sandbox workspace-write -c sandbox_workspace_write.network_access=true --color never -o .agent/windows-store-impl-summary.md - < .agent/windows-store-impl-task.md
exit "$?"
