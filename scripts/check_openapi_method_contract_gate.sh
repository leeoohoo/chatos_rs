#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
POLICY_FILE="${OPENAPI_GATE_POLICY_FILE:-$ROOT_DIR/.github/api-contract/openapi-gate-policy.env}"
OPENAPI_GATE_WAIVER_LABEL="OpenAPI method gate"

# shellcheck source=scripts/openapi_contract_common.sh
source "$ROOT_DIR/scripts/openapi_contract_common.sh"

load_policy() {
  OPENAPI_METHOD_GATE_MODE="advisory"
  OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO="0"
  OPENAPI_GATE_WAIVER_FILE=".github/api-contract/waivers/openapi_gate_waiver.env"
  OPENAPI_GATE_WAIVER_MAX_HOURS="24"

  load_openapi_policy_file "$POLICY_FILE"
}

extract_baseline_methods() {
  local section="$1"
  awk -v section="$section" '
    /^## chatos\/backend endpoints/ { active=(section=="main"); next }
    /^## / { active=0 }
    active && /^[A-Z]+ / { print }
  ' "$BASELINE_FILE" \
    | sed -E 's/:([A-Za-z0-9_]+)/{\1}/g' \
    | sort -u
}

extract_openapi_methods() {
  local file="$1"
  awk '
    /^[[:space:]][[:space:]]\/[^:]*:/ {
      path=$1
      sub(/:$/, "", path)
      next
    }
    path != "" && /^[[:space:]][[:space:]][[:space:]][[:space:]](get|post|put|patch|delete|head|options):/ {
      method=toupper($1)
      sub(/:$/, "", method)
      print method " " path
    }
  ' "$file" | sort -u
}

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[ERROR] Missing API path baseline: $BASELINE_FILE"
  echo "[INFO] Run: bash scripts/update_api_path_baseline.sh"
  exit 1
fi

load_policy

if [[ "$OPENAPI_METHOD_GATE_MODE" != "advisory" && "$OPENAPI_METHOD_GATE_MODE" != "required" ]]; then
  echo "[ERROR] Invalid OPENAPI_METHOD_GATE_MODE: $OPENAPI_METHOD_GATE_MODE"
  echo "[INFO] Allowed values: advisory|required"
  exit 1
fi

main_baseline_file="$(mktemp)"
main_contract_file="$(mktemp)"
trap 'rm -f "$main_baseline_file" "$main_contract_file"' EXIT

extract_baseline_methods "main" > "$main_baseline_file"
extract_openapi_methods "$MAIN_CONTRACT" > "$main_contract_file"
main_baseline_count="$(wc -l < "$main_baseline_file" | tr -d ' ')"
main_contract_count="$(wc -l < "$main_contract_file" | tr -d ' ')"

main_covered_count="$(comm -12 "$main_baseline_file" "$main_contract_file" | wc -l | tr -d ' ')"

main_ratio="$(to_ratio "$main_covered_count" "$main_baseline_count")"

echo "[INFO] OpenAPI method gate snapshot:"
echo "  mode:                                 $OPENAPI_METHOD_GATE_MODE"
echo "  main backend baseline method-endpoints:   $main_baseline_count"
echo "  main backend contract method-endpoints:   $main_contract_count"
echo "  main backend covered method-endpoints:    $main_covered_count"
echo "  main backend method coverage ratio:       ${main_ratio}%"
echo "  main backend minimum ratio:               ${OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO}%"

if [[ "$OPENAPI_METHOD_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI method gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="false"

if ratio_meets_threshold "$main_ratio" "$OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO"; then
  main_ok="true"
fi

if [[ "$main_ok" == "true" ]]; then
  echo "[OK] OpenAPI method required gate passed."
  exit 0
fi

waiver_file="$(resolve_repo_path "$OPENAPI_GATE_WAIVER_FILE")"
if validate_waiver_if_present "$waiver_file"; then
  echo "[WARN] OpenAPI method required gate bypassed by active waiver."
  exit 0
fi

echo "[ERROR] OpenAPI method required gate failed."
if [[ "$main_ok" != "true" ]]; then
  echo "  - main backend method coverage ${main_ratio}% is below ${OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO}%"
fi
echo "[INFO] Add missing OpenAPI operations or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
