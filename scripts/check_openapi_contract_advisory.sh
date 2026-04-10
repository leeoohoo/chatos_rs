#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
MEMORY_CONTRACT="$ROOT_DIR/.github/api-contract/memory_server.openapi.yaml"

count_contract_paths() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "0"
    return
  fi
  # Count path entries under `paths:` by matching two-space indented `/...:`.
  # Use awk to avoid grep(1) non-zero exit when count is zero under pipefail.
  awk '/^[[:space:]][[:space:]]\/[^:]*:/{count++} END {print count + 0}' "$file"
}

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[WARN] Missing API path baseline: $BASELINE_FILE"
  echo "[WARN] Skipping OpenAPI advisory check."
  exit 0
fi

main_baseline_count="$(awk -F= '/^main_backend_endpoint_count=/{print $2}' "$BASELINE_FILE")"
memory_baseline_count="$(awk -F= '/^memory_backend_endpoint_count=/{print $2}' "$BASELINE_FILE")"

main_contract_count="$(count_contract_paths "$MAIN_CONTRACT")"
memory_contract_count="$(count_contract_paths "$MEMORY_CONTRACT")"

main_ratio="0.00"
memory_ratio="0.00"
if [[ "${main_baseline_count:-0}" -gt 0 ]]; then
  main_ratio="$(awk "BEGIN { printf \"%.2f\", (${main_contract_count:-0} * 100.0) / ${main_baseline_count} }")"
fi
if [[ "${memory_baseline_count:-0}" -gt 0 ]]; then
  memory_ratio="$(awk "BEGIN { printf \"%.2f\", (${memory_contract_count:-0} * 100.0) / ${memory_baseline_count} }")"
fi

echo "[INFO] OpenAPI advisory coverage snapshot:"
echo "  main backend baseline endpoints: ${main_baseline_count:-0}"
echo "  main backend openapi paths:      ${main_contract_count:-0}"
echo "  main backend coverage ratio:     ${main_ratio}%"
echo "  memory backend baseline endpoints: ${memory_baseline_count:-0}"
echo "  memory backend openapi paths:      ${memory_contract_count:-0}"
echo "  memory backend coverage ratio:     ${memory_ratio}%"

if [[ "${main_contract_count:-0}" -eq 0 || "${memory_contract_count:-0}" -eq 0 ]]; then
  echo "[WARN] OpenAPI contracts are still bootstrap-level."
fi

echo "[OK] OpenAPI advisory check completed (non-blocking)."
