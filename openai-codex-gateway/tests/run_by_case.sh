#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Backward-compatible wrapper:
# keep `tests/run_by_case.sh` as stable entrypoint while implementation lives in `tests/scripts/`.
exec "$SCRIPT_DIR/scripts/run_by_case.sh" "$@"
