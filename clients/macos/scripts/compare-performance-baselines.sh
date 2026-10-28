#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  print -u2 -- "Performance comparison requires native Apple Silicon macOS."
  exit 1
fi

exec /usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/compare_performance_baselines.py" "$@"
