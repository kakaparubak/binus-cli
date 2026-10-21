#!/usr/bin/env bash
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
umask 077
cd "$HOME/Documents/Coding/binus-cli" || exit 1
exec > .agent/forums-transport-durable.log 2>&1
finish() {
  local rc=$?
  trap - EXIT
  printf '%s\n' "$rc" > .agent/forums-transport-durable.exit
  tmux wait-for -S binus-forum-tests-done || true
  exit "$rc"
}
trap finish EXIT
python3 -c 'from pathlib import Path; print(Path(".agent/forums-transport-durable-task.md").read_text())' | codex exec --model gpt-6-luna --sandbox workspace-write -c sandbox_workspace_write.network_access=true --add-dir "$HOME/.cargo" --add-dir "$HOME/.rustup" --color never -o .agent/forums-transport-test-summary.md -
exit "$?"
