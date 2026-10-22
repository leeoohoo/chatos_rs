#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "ChatOS macOS benchmarks require a native arm64 shell; Intel is unsupported." >&2
  exit 2
fi
exec /usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/run_performance_baseline.py" "$@"
