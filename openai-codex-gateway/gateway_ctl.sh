#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Backward-compatible wrapper:
# keep `./gateway_ctl.sh ...` working while implementation lives in `scripts/`.
exec "$SCRIPT_DIR/scripts/gateway_ctl.sh" "$@"
