#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
POLICY_FILE="${OPENAPI_GATE_POLICY_FILE:-$ROOT_DIR/.github/api-contract/openapi-gate-policy.env}"
OPENAPI_GATE_WAIVER_LABEL="OpenAPI gate"

# shellcheck source=scripts/openapi_contract_common.sh
source "$ROOT_DIR/scripts/openapi_contract_common.sh"

count_contract_paths() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "0"
    return
  fi

  awk '/^[[:space:]][[:space:]]\/[^:]*:/{count++} END {print count + 0}' "$file"
}

load_policy() {
  OPENAPI_GATE_MODE="advisory"
  OPENAPI_MAIN_MIN_COVERAGE_RATIO="0"
  OPENAPI_GATE_WAIVER_FILE=".github/api-contract/waivers/openapi_gate_waiver.env"
  OPENAPI_GATE_WAIVER_MAX_HOURS="24"

  load_openapi_policy_file "$POLICY_FILE"
}

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[ERROR] Missing API path baseline: $BASELINE_FILE"
  echo "[INFO] Run: bash scripts/update_api_path_baseline.sh"
  exit 1
fi

load_policy

if [[ "$OPENAPI_GATE_MODE" != "advisory" && "$OPENAPI_GATE_MODE" != "required" ]]; then
  echo "[ERROR] Invalid OPENAPI_GATE_MODE: $OPENAPI_GATE_MODE"
  echo "[INFO] Allowed values: advisory|required"
  exit 1
fi

main_baseline_count="$(awk -F= '/^main_backend_endpoint_count=/{print $2}' "$BASELINE_FILE")"
main_contract_count="$(count_contract_paths "$MAIN_CONTRACT")"

main_ratio="$(to_ratio "${main_contract_count:-0}" "${main_baseline_count:-0}")"

echo "[INFO] OpenAPI gate snapshot:"
echo "  mode:                            $OPENAPI_GATE_MODE"
echo "  main backend baseline endpoints:   ${main_baseline_count:-0}"
echo "  main backend openapi paths:        ${main_contract_count:-0}"
echo "  main backend coverage ratio:       ${main_ratio}%"
echo "  main backend minimum ratio:        ${OPENAPI_MAIN_MIN_COVERAGE_RATIO}%"

if [[ "$OPENAPI_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="false"

if ratio_meets_threshold "$main_ratio" "$OPENAPI_MAIN_MIN_COVERAGE_RATIO"; then
  main_ok="true"
fi

if [[ "$main_ok" == "true" ]]; then
  echo "[OK] OpenAPI required gate passed."
  exit 0
fi

waiver_file="$(resolve_repo_path "$OPENAPI_GATE_WAIVER_FILE")"
if validate_waiver_if_present "$waiver_file"; then
  echo "[WARN] OpenAPI required gate bypassed by active waiver."
  exit 0
fi

echo "[ERROR] OpenAPI required gate failed."
if [[ "$main_ok" != "true" ]]; then
  echo "  - main backend ratio ${main_ratio}% is below ${OPENAPI_MAIN_MIN_COVERAGE_RATIO}%"
fi
echo "[INFO] Expand OpenAPI contracts or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
