#!/usr/bin/env bash
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
umask 077
cd "$HOME/Documents/Coding/binus-cli" || exit 1
exec > .agent/forums-success-assert.log 2>&1
finish() {
  local rc=$?
  trap - EXIT
  printf '%s\n' "$rc" > .agent/forums-success-assert.exit
  tmux wait-for -S binus-forum-final-assert-done || true
  exit "$rc"
}
trap finish EXIT
python3 -c 'from pathlib import Path; print(Path(".agent/forums-success-assert-task.md").read_text())' | codex exec --model gpt-6-luna --sandbox workspace-write -c sandbox_workspace_write.network_access=true --add-dir "$HOME/.cargo" --add-dir "$HOME/.rustup" --color never -o .agent/forums-success-assert-summary.md -
exit "$?"
