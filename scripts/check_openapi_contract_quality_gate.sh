#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MAIN_CONTRACT="$ROOT_DIR/.github/api-contract/chat_app_server_rs.openapi.yaml"
MEMORY_CONTRACT="$ROOT_DIR/.github/api-contract/memory_server.openapi.yaml"
POLICY_FILE="${OPENAPI_GATE_POLICY_FILE:-$ROOT_DIR/.github/api-contract/openapi-gate-policy.env}"

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
  OPENAPI_QUALITY_GATE_MODE="advisory"
  OPENAPI_QUALITY_MIN_SUMMARY_RATIO="0"
  OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO="0"
  OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO="0"
  OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO="0"
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

  echo "[WARN] OpenAPI quality gate is waived by emergency exception."
  echo "  waiver_id: $OPENAPI_GATE_WAIVER_ID"
  echo "  approver:  $OPENAPI_GATE_WAIVER_APPROVER"
  echo "  expires:   $OPENAPI_GATE_WAIVER_EXPIRES_AT"
  echo "  reason:    $OPENAPI_GATE_WAIVER_REASON"
  echo "  max_hours: $OPENAPI_GATE_WAIVER_MAX_HOURS"
  return 0
}

collect_metrics() {
  local contract_file="$1"
  local prefix="$2"
  python3 - "$contract_file" "$prefix" <<'PY'
import re
import sys
import yaml

contract_file = sys.argv[1]
prefix = sys.argv[2]
methods = {"get", "post", "put", "patch", "delete", "head", "options"}

with open(contract_file, "r", encoding="utf-8") as f:
    doc = yaml.safe_load(f)

paths = (doc or {}).get("paths") or {}

total_ops = 0
summary_ok = 0
path_param_ok = 0
path_param_total = 0
response_desc_ok = 0
response_desc_total = 0
success_response_ok = 0

for path, item in paths.items():
    if not isinstance(item, dict):
        continue

    path_template_params = set(re.findall(r"\{([^}]+)\}", path))
    path_level_params = set()
    for param in item.get("parameters") or []:
        if isinstance(param, dict) and param.get("in") == "path" and param.get("name"):
            path_level_params.add(param["name"])

    for method, op in item.items():
        if method not in methods or not isinstance(op, dict):
            continue
        total_ops += 1

        if op.get("summary"):
            summary_ok += 1

        op_level_params = set()
        for param in op.get("parameters") or []:
            if isinstance(param, dict) and param.get("in") == "path" and param.get("name"):
                op_level_params.add(param["name"])

        declared_params = path_level_params | op_level_params
        path_param_total += 1
        if path_template_params.issubset(declared_params):
            path_param_ok += 1

        responses = op.get("responses") or {}
        has_success = False
        for code, response_obj in responses.items():
            code_s = str(code)
            if code_s.startswith("2") or code_s == "101":
                has_success = True
            response_desc_total += 1
            if isinstance(response_obj, dict) and response_obj.get("description"):
                response_desc_ok += 1
        if has_success:
            success_response_ok += 1

def ratio(ok, total):
    if total <= 0:
        return "0.00"
    return f"{(ok * 100.0) / total:.2f}"

print(f"{prefix}_total_ops={total_ops}")
print(f"{prefix}_summary_ok={summary_ok}")
print(f"{prefix}_summary_ratio={ratio(summary_ok, total_ops)}")
print(f"{prefix}_path_param_ok={path_param_ok}")
print(f"{prefix}_path_param_total={path_param_total}")
print(f"{prefix}_path_param_ratio={ratio(path_param_ok, path_param_total)}")
print(f"{prefix}_response_desc_ok={response_desc_ok}")
print(f"{prefix}_response_desc_total={response_desc_total}")
print(f"{prefix}_response_desc_ratio={ratio(response_desc_ok, response_desc_total)}")
print(f"{prefix}_success_response_ok={success_response_ok}")
print(f"{prefix}_success_response_ratio={ratio(success_response_ok, total_ops)}")
PY
}

load_policy

if [[ "$OPENAPI_QUALITY_GATE_MODE" != "advisory" && "$OPENAPI_QUALITY_GATE_MODE" != "required" ]]; then
  echo "[ERROR] Invalid OPENAPI_QUALITY_GATE_MODE: $OPENAPI_QUALITY_GATE_MODE"
  echo "[INFO] Allowed values: advisory|required"
  exit 1
fi

metrics_file="$(mktemp)"
trap 'rm -f "$metrics_file"' EXIT
collect_metrics "$MAIN_CONTRACT" "main" > "$metrics_file"
collect_metrics "$MEMORY_CONTRACT" "memory" >> "$metrics_file"

# shellcheck disable=SC1090
source "$metrics_file"

echo "[INFO] OpenAPI quality gate snapshot:"
echo "  mode:                                      $OPENAPI_QUALITY_GATE_MODE"
echo "  main total operations:                     ${main_total_ops:-0}"
echo "  main summary ratio:                        ${main_summary_ratio:-0}%"
echo "  main path-parameter completeness ratio:    ${main_path_param_ratio:-0}%"
echo "  main response-description ratio:           ${main_response_desc_ratio:-0}%"
echo "  main success-response ratio:               ${main_success_response_ratio:-0}%"
echo "  memory total operations:                   ${memory_total_ops:-0}"
echo "  memory summary ratio:                      ${memory_summary_ratio:-0}%"
echo "  memory path-parameter completeness ratio:  ${memory_path_param_ratio:-0}%"
echo "  memory response-description ratio:         ${memory_response_desc_ratio:-0}%"
echo "  memory success-response ratio:             ${memory_success_response_ratio:-0}%"
echo "  thresholds:"
echo "    summary >= ${OPENAPI_QUALITY_MIN_SUMMARY_RATIO}%"
echo "    path-parameter completeness >= ${OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO}%"
echo "    response-description >= ${OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO}%"
echo "    success-response >= ${OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO}%"

if [[ "$OPENAPI_QUALITY_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI quality gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="true"
memory_ok="true"

check_service_metrics() {
  local service="$1"
  local summary_ratio="$2"
  local path_param_ratio="$3"
  local response_desc_ratio="$4"
  local success_response_ratio="$5"

  if ! ratio_meets_threshold "$summary_ratio" "$OPENAPI_QUALITY_MIN_SUMMARY_RATIO"; then
    echo "  - $service summary ratio ${summary_ratio}% is below ${OPENAPI_QUALITY_MIN_SUMMARY_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$path_param_ratio" "$OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO"; then
    echo "  - $service path-parameter completeness ratio ${path_param_ratio}% is below ${OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$response_desc_ratio" "$OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO"; then
    echo "  - $service response-description ratio ${response_desc_ratio}% is below ${OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$success_response_ratio" "$OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO"; then
    echo "  - $service success-response ratio ${success_response_ratio}% is below ${OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO}%"
    return 1
  fi
  return 0
}

if ! check_service_metrics "main backend" "${main_summary_ratio:-0}" "${main_path_param_ratio:-0}" "${main_response_desc_ratio:-0}" "${main_success_response_ratio:-0}"; then
  main_ok="false"
fi
if ! check_service_metrics "memory backend" "${memory_summary_ratio:-0}" "${memory_path_param_ratio:-0}" "${memory_response_desc_ratio:-0}" "${memory_success_response_ratio:-0}"; then
  memory_ok="false"
fi

if [[ "$main_ok" == "true" && "$memory_ok" == "true" ]]; then
  echo "[OK] OpenAPI quality required gate passed."
  exit 0
fi

waiver_file="$(resolve_repo_path "$OPENAPI_GATE_WAIVER_FILE")"
if validate_waiver_if_present "$waiver_file"; then
  echo "[WARN] OpenAPI quality required gate bypassed by active waiver."
  exit 0
fi

echo "[ERROR] OpenAPI quality required gate failed."
echo "[INFO] Fix quality gaps or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
