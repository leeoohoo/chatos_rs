#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
GEN_SCRIPT="$ROOT_DIR/scripts/generate_api_path_snapshot.sh"

bash "$GEN_SCRIPT" > "$BASELINE_FILE"
echo "[OK] Updated API path baseline: $BASELINE_FILE"
