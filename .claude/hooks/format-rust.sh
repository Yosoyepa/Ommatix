#!/usr/bin/env bash
# PostToolUse hook: format any Rust file an agent just wrote or edited.
# Reads the hook payload (JSON) on stdin; never blocks the agent.
set -uo pipefail
file=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null)
if [[ "$file" == *.rs && -f "$file" ]] && command -v rustfmt >/dev/null 2>&1; then
  rustfmt --edition 2024 --config-path "${CLAUDE_PROJECT_DIR:-.}/rustfmt.toml" "$file" >/dev/null 2>&1 || true
fi
exit 0
