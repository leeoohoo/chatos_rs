#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "ChatOS macOS soak collection requires a native arm64 shell; Intel is unsupported." >&2
  exit 2
fi
exec /usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/run_process_group_soak.py" "$@"
