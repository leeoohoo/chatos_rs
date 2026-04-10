#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
GEN_SCRIPT="$ROOT_DIR/scripts/generate_api_path_snapshot.sh"

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[ERROR] Missing baseline file: $BASELINE_FILE"
  echo "[INFO] Run: bash scripts/update_api_path_baseline.sh"
  exit 1
fi

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

bash "$GEN_SCRIPT" > "$tmp_file"

if ! diff -u "$BASELINE_FILE" "$tmp_file"; then
  echo
  echo "[ERROR] API path baseline mismatch."
  echo "[INFO] If changes are intentional, run: bash scripts/update_api_path_baseline.sh"
  exit 1
fi

echo "[OK] API path baseline check passed."
