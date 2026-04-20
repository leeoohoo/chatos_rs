#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-path-baseline.txt"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
MEMORY_CONTRACT="$ROOT_DIR/.github/api-contract/memory_server.openapi.yaml"
POLICY_FILE="${OPENAPI_GATE_POLICY_FILE:-$ROOT_DIR/.github/api-contract/openapi-gate-policy.env}"

to_ratio() {
  local covered="$1"
  local total="$2"
  awk -v c="$covered" -v t="$total" \
    'BEGIN { if (t <= 0) { printf "0.00"; } else { printf "%.2f", (c * 100.0) / t; } }'
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
  OPENAPI_METHOD_GATE_MODE="advisory"
  OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO="0"
  OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO="0"
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

  echo "[WARN] OpenAPI method gate is waived by emergency exception."
  echo "  waiver_id: $OPENAPI_GATE_WAIVER_ID"
  echo "  approver:  $OPENAPI_GATE_WAIVER_APPROVER"
  echo "  expires:   $OPENAPI_GATE_WAIVER_EXPIRES_AT"
  echo "  reason:    $OPENAPI_GATE_WAIVER_REASON"
  echo "  max_hours: $OPENAPI_GATE_WAIVER_MAX_HOURS"
  return 0
}

extract_baseline_methods() {
  local section="$1"
  awk -v section="$section" '
    /^## chat_app_server_rs endpoints/ { active=(section=="main"); next }
    /^## memory_server endpoints/ { active=(section=="memory"); next }
    /^## / { active=0 }
    active && /^[A-Z]+ / { print }
  ' "$BASELINE_FILE" \
    | sed -E 's/:([A-Za-z0-9_]+)/{\1}/g' \
    | sort -u
}

extract_openapi_methods() {
  local file="$1"
  awk '
    /^[[:space:]]{2}\/[^:]*:/ {
      path=$1
      sub(/:$/, "", path)
      next
    }
    path != "" && /^[[:space:]]{4}(get|post|put|patch|delete|head|options):/ {
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
memory_baseline_file="$(mktemp)"
memory_contract_file="$(mktemp)"
trap 'rm -f "$main_baseline_file" "$main_contract_file" "$memory_baseline_file" "$memory_contract_file"' EXIT

extract_baseline_methods "main" > "$main_baseline_file"
extract_baseline_methods "memory" > "$memory_baseline_file"
extract_openapi_methods "$MAIN_CONTRACT" > "$main_contract_file"
extract_openapi_methods "$MEMORY_CONTRACT" > "$memory_contract_file"

main_baseline_count="$(wc -l < "$main_baseline_file" | tr -d ' ')"
memory_baseline_count="$(wc -l < "$memory_baseline_file" | tr -d ' ')"
main_contract_count="$(wc -l < "$main_contract_file" | tr -d ' ')"
memory_contract_count="$(wc -l < "$memory_contract_file" | tr -d ' ')"

main_covered_count="$(comm -12 "$main_baseline_file" "$main_contract_file" | wc -l | tr -d ' ')"
memory_covered_count="$(comm -12 "$memory_baseline_file" "$memory_contract_file" | wc -l | tr -d ' ')"

main_ratio="$(to_ratio "$main_covered_count" "$main_baseline_count")"
memory_ratio="$(to_ratio "$memory_covered_count" "$memory_baseline_count")"

echo "[INFO] OpenAPI method gate snapshot:"
echo "  mode:                                 $OPENAPI_METHOD_GATE_MODE"
echo "  main backend baseline method-endpoints:   $main_baseline_count"
echo "  main backend contract method-endpoints:   $main_contract_count"
echo "  main backend covered method-endpoints:    $main_covered_count"
echo "  main backend method coverage ratio:       ${main_ratio}%"
echo "  main backend minimum ratio:               ${OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO}%"
echo "  memory backend baseline method-endpoints: $memory_baseline_count"
echo "  memory backend contract method-endpoints: $memory_contract_count"
echo "  memory backend covered method-endpoints:  $memory_covered_count"
echo "  memory backend method coverage ratio:     ${memory_ratio}%"
echo "  memory backend minimum ratio:             ${OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO}%"

if [[ "$OPENAPI_METHOD_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI method gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="false"
memory_ok="false"

if ratio_meets_threshold "$main_ratio" "$OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO"; then
  main_ok="true"
fi

if ratio_meets_threshold "$memory_ratio" "$OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO"; then
  memory_ok="true"
fi

if [[ "$main_ok" == "true" && "$memory_ok" == "true" ]]; then
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
if [[ "$memory_ok" != "true" ]]; then
  echo "  - memory backend method coverage ${memory_ratio}% is below ${OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO}%"
fi
echo "[INFO] Add missing OpenAPI operations or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
