#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
MEMORY_CONTRACT="$ROOT_DIR/.github/api-contract/memory_server.openapi.yaml"
POLICY_FILE="${OPENAPI_GATE_POLICY_FILE:-$ROOT_DIR/.github/api-contract/openapi-gate-policy.env}"

count_contract_paths() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "0"
    return
  fi

  awk '/^[[:space:]][[:space:]]\/[^:]*:/{count++} END {print count + 0}' "$file"
}

to_ratio() {
  local contract_count="$1"
  local baseline_count="$2"
  awk -v c="$contract_count" -v b="$baseline_count" \
    'BEGIN { if (b <= 0) { printf "0.00"; } else { printf "%.2f", (c * 100.0) / b; } }'
}

ratio_meets_threshold() {
  local ratio="$1"
  local threshold="$2"
  awk -v r="$ratio" -v t="$threshold" 'BEGIN { exit ((r + 0) >= (t + 0) ? 0 : 1) }'
}

resolve_repo_path() {
  local raw_path="$1"
  if [[ "$raw_path" = /* ]]; then
    echo "$raw_path"
  else
    echo "$ROOT_DIR/$raw_path"
  fi
}

waiver_expiry_is_future() {
  local expires_at="$1"
  python3 - "$expires_at" <<'PY'
import sys
from datetime import datetime, timezone

value = sys.argv[1]
try:
    dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
except Exception:
    sys.exit(1)

if dt.tzinfo is None:
    dt = dt.replace(tzinfo=timezone.utc)

if dt <= datetime.now(timezone.utc):
    sys.exit(1)
PY
}

waiver_expiry_within_max_hours() {
  local expires_at="$1"
  local max_hours="$2"
  python3 - "$expires_at" "$max_hours" <<'PY'
import sys
from datetime import datetime, timedelta, timezone

value = sys.argv[1]
try:
    max_hours = float(sys.argv[2])
except Exception:
    sys.exit(1)

if max_hours <= 0:
    sys.exit(0)

try:
    dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
except Exception:
    sys.exit(1)

if dt.tzinfo is None:
    dt = dt.replace(tzinfo=timezone.utc)

now = datetime.now(timezone.utc)
max_dt = now + timedelta(hours=max_hours)
if dt > max_dt:
    sys.exit(1)
PY
}

is_non_negative_number() {
  local value="$1"
  awk -v v="$value" 'BEGIN { exit (v + 0 >= 0 ? 0 : 1) }'
}

load_policy() {
  OPENAPI_GATE_MODE="advisory"
  OPENAPI_MAIN_MIN_COVERAGE_RATIO="0"
  OPENAPI_MEMORY_MIN_COVERAGE_RATIO="0"
  OPENAPI_GATE_WAIVER_FILE=".github/api-contract/waivers/openapi_gate_waiver.env"
  OPENAPI_GATE_WAIVER_MAX_HOURS="24"

  if [[ ! -f "$POLICY_FILE" ]]; then
    echo "[WARN] Missing policy file: $POLICY_FILE"
    echo "[WARN] Falling back to advisory mode."
    return
  fi

  # shellcheck disable=SC1090
  source "$POLICY_FILE"
}

validate_waiver_if_present() {
  local waiver_file="$1"

  if [[ ! -f "$waiver_file" ]]; then
    return 1
  fi

  OPENAPI_GATE_WAIVER_ENABLED=""
  OPENAPI_GATE_WAIVER_ID=""
  OPENAPI_GATE_WAIVER_REASON=""
  OPENAPI_GATE_WAIVER_APPROVER=""
  OPENAPI_GATE_WAIVER_EXPIRES_AT=""

  # shellcheck disable=SC1090
  source "$waiver_file"

  if [[ "${OPENAPI_GATE_WAIVER_ENABLED:-false}" != "true" ]]; then
    echo "[INFO] Waiver file is present but disabled: $waiver_file"
    return 1
  fi

  local required_keys=(
    "OPENAPI_GATE_WAIVER_ID"
    "OPENAPI_GATE_WAIVER_REASON"
    "OPENAPI_GATE_WAIVER_APPROVER"
    "OPENAPI_GATE_WAIVER_EXPIRES_AT"
  )

  local key
  for key in "${required_keys[@]}"; do
    if [[ -z "${!key:-}" ]]; then
      echo "[ERROR] Waiver field is required but missing: $key"
      return 1
    fi
  done

  if ! waiver_expiry_is_future "$OPENAPI_GATE_WAIVER_EXPIRES_AT"; then
    echo "[ERROR] Waiver timestamp is invalid or expired: $OPENAPI_GATE_WAIVER_EXPIRES_AT"
    return 1
  fi

  if ! is_non_negative_number "$OPENAPI_GATE_WAIVER_MAX_HOURS"; then
    echo "[ERROR] OPENAPI_GATE_WAIVER_MAX_HOURS must be a non-negative number."
    return 1
  fi

  if ! waiver_expiry_within_max_hours "$OPENAPI_GATE_WAIVER_EXPIRES_AT" "$OPENAPI_GATE_WAIVER_MAX_HOURS"; then
    echo "[ERROR] Waiver expiry exceeds allowed lifetime: ${OPENAPI_GATE_WAIVER_MAX_HOURS}h"
    return 1
  fi

  echo "[WARN] OpenAPI gate is waived by emergency exception."
  echo "  waiver_id: $OPENAPI_GATE_WAIVER_ID"
  echo "  approver:  $OPENAPI_GATE_WAIVER_APPROVER"
  echo "  expires:   $OPENAPI_GATE_WAIVER_EXPIRES_AT"
  echo "  reason:    $OPENAPI_GATE_WAIVER_REASON"
  echo "  max_hours: $OPENAPI_GATE_WAIVER_MAX_HOURS"
  return 0
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
memory_baseline_count="$(awk -F= '/^memory_backend_endpoint_count=/{print $2}' "$BASELINE_FILE")"

main_contract_count="$(count_contract_paths "$MAIN_CONTRACT")"
memory_contract_count="$(count_contract_paths "$MEMORY_CONTRACT")"

main_ratio="$(to_ratio "${main_contract_count:-0}" "${main_baseline_count:-0}")"
memory_ratio="$(to_ratio "${memory_contract_count:-0}" "${memory_baseline_count:-0}")"

echo "[INFO] OpenAPI gate snapshot:"
echo "  mode:                            $OPENAPI_GATE_MODE"
echo "  main backend baseline endpoints:   ${main_baseline_count:-0}"
echo "  main backend openapi paths:        ${main_contract_count:-0}"
echo "  main backend coverage ratio:       ${main_ratio}%"
echo "  main backend minimum ratio:        ${OPENAPI_MAIN_MIN_COVERAGE_RATIO}%"
echo "  memory backend baseline endpoints: ${memory_baseline_count:-0}"
echo "  memory backend openapi paths:      ${memory_contract_count:-0}"
echo "  memory backend coverage ratio:     ${memory_ratio}%"
echo "  memory backend minimum ratio:      ${OPENAPI_MEMORY_MIN_COVERAGE_RATIO}%"

if [[ "$OPENAPI_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="false"
memory_ok="false"

if ratio_meets_threshold "$main_ratio" "$OPENAPI_MAIN_MIN_COVERAGE_RATIO"; then
  main_ok="true"
fi

if ratio_meets_threshold "$memory_ratio" "$OPENAPI_MEMORY_MIN_COVERAGE_RATIO"; then
  memory_ok="true"
fi

if [[ "$main_ok" == "true" && "$memory_ok" == "true" ]]; then
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
if [[ "$memory_ok" != "true" ]]; then
  echo "  - memory backend ratio ${memory_ratio}% is below ${OPENAPI_MEMORY_MIN_COVERAGE_RATIO}%"
fi
echo "[INFO] Expand OpenAPI contracts or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
