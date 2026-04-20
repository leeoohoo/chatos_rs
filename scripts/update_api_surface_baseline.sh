#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-surface-baseline.txt"
GEN_SCRIPT="$ROOT_DIR/scripts/generate_api_surface_snapshot.sh"

bash "$GEN_SCRIPT" > "$BASELINE_FILE"
echo "[OK] Updated API surface baseline: $BASELINE_FILE"
