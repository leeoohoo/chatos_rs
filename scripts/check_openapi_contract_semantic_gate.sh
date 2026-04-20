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
  OPENAPI_SEMANTIC_GATE_MODE="advisory"
  OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO="0"
  OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO="0"
  OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO="0"
  OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO="0"
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

  echo "[WARN] OpenAPI semantic gate is waived by emergency exception."
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
import sys
import yaml

contract_file = sys.argv[1]
prefix = sys.argv[2]
methods = {"get", "post", "put", "patch", "delete", "head", "options"}

with open(contract_file, "r", encoding="utf-8") as f:
    doc = yaml.safe_load(f)

paths = (doc or {}).get("paths") or {}

total_ops = 0
operation_id_present = 0
operation_ids = []
success_json_schema_total = 0
success_json_schema_ok = 0
request_body_total = 0
request_body_json_schema_ok = 0

for _path, item in paths.items():
    if not isinstance(item, dict):
        continue
    for method, op in item.items():
        if method not in methods or not isinstance(op, dict):
            continue
        total_ops += 1

        op_id = op.get("operationId")
        if isinstance(op_id, str) and op_id.strip():
            operation_id_present += 1
            operation_ids.append(op_id.strip())

        responses = op.get("responses") or {}
        has_success_requiring_json_schema = False
        has_success_json_schema = False
        for code, response_obj in responses.items():
            code_s = str(code)
            if not code_s.startswith("2"):
                continue
            if code_s in {"101", "204"}:
                continue
            has_success_requiring_json_schema = True
            if not isinstance(response_obj, dict):
                continue
            content = response_obj.get("content")
            if not isinstance(content, dict):
                continue
            app_json = content.get("application/json")
            if not isinstance(app_json, dict):
                continue
            if "schema" in app_json:
                has_success_json_schema = True
                break

        if has_success_requiring_json_schema:
            success_json_schema_total += 1
            if has_success_json_schema:
                success_json_schema_ok += 1

        request_body = op.get("requestBody")
        if request_body is not None:
            request_body_total += 1
            if isinstance(request_body, dict):
                content = request_body.get("content")
                if isinstance(content, dict):
                    app_json = content.get("application/json")
                    if isinstance(app_json, dict) and "schema" in app_json:
                        request_body_json_schema_ok += 1


def ratio(ok, total):
    if total <= 0:
        return "100.00"
    return f"{(ok * 100.0) / total:.2f}"

unique_operation_ids = len(set(operation_ids))

print(f"{prefix}_total_ops={total_ops}")
print(f"{prefix}_operation_id_present={operation_id_present}")
print(f"{prefix}_operation_id_ratio={ratio(operation_id_present, total_ops)}")
print(f"{prefix}_operation_id_unique={unique_operation_ids}")
print(f"{prefix}_operation_id_uniqueness_ratio={ratio(unique_operation_ids, total_ops)}")
print(f"{prefix}_success_json_schema_ok={success_json_schema_ok}")
print(f"{prefix}_success_json_schema_total={success_json_schema_total}")
print(f"{prefix}_success_json_schema_ratio={ratio(success_json_schema_ok, success_json_schema_total)}")
print(f"{prefix}_request_body_json_schema_ok={request_body_json_schema_ok}")
print(f"{prefix}_request_body_total={request_body_total}")
print(f"{prefix}_request_body_json_schema_ratio={ratio(request_body_json_schema_ok, request_body_total)}")
PY
}

load_policy

if [[ "$OPENAPI_SEMANTIC_GATE_MODE" != "advisory" && "$OPENAPI_SEMANTIC_GATE_MODE" != "required" ]]; then
  echo "[ERROR] Invalid OPENAPI_SEMANTIC_GATE_MODE: $OPENAPI_SEMANTIC_GATE_MODE"
  echo "[INFO] Allowed values: advisory|required"
  exit 1
fi

metrics_file="$(mktemp)"
trap 'rm -f "$metrics_file"' EXIT
collect_metrics "$MAIN_CONTRACT" "main" > "$metrics_file"
collect_metrics "$MEMORY_CONTRACT" "memory" >> "$metrics_file"

# shellcheck disable=SC1090
source "$metrics_file"

echo "[INFO] OpenAPI semantic gate snapshot:"
echo "  mode:                                      $OPENAPI_SEMANTIC_GATE_MODE"
echo "  main operationId ratio:                    ${main_operation_id_ratio:-0}%"
echo "  main operationId uniqueness ratio:         ${main_operation_id_uniqueness_ratio:-0}%"
echo "  main success-json-schema ratio:            ${main_success_json_schema_ratio:-0}%"
echo "  main requestBody-json-schema ratio:        ${main_request_body_json_schema_ratio:-0}%"
echo "  memory operationId ratio:                  ${memory_operation_id_ratio:-0}%"
echo "  memory operationId uniqueness ratio:       ${memory_operation_id_uniqueness_ratio:-0}%"
echo "  memory success-json-schema ratio:          ${memory_success_json_schema_ratio:-0}%"
echo "  memory requestBody-json-schema ratio:      ${memory_request_body_json_schema_ratio:-0}%"
echo "  thresholds:"
echo "    operationId ratio >= ${OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO}%"
echo "    operationId uniqueness ratio >= ${OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO}%"
echo "    success-json-schema ratio >= ${OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO}%"
echo "    requestBody-json-schema ratio >= ${OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO}%"

if [[ "$OPENAPI_SEMANTIC_GATE_MODE" == "advisory" ]]; then
  echo "[OK] OpenAPI semantic gate is in advisory mode (non-blocking)."
  exit 0
fi

main_ok="true"
memory_ok="true"

check_service_metrics() {
  local service="$1"
  local operation_id_ratio="$2"
  local operation_id_uniqueness_ratio="$3"
  local success_json_schema_ratio="$4"
  local request_body_json_schema_ratio="$5"

  if ! ratio_meets_threshold "$operation_id_ratio" "$OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO"; then
    echo "  - $service operationId ratio ${operation_id_ratio}% is below ${OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$operation_id_uniqueness_ratio" "$OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO"; then
    echo "  - $service operationId uniqueness ratio ${operation_id_uniqueness_ratio}% is below ${OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$success_json_schema_ratio" "$OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO"; then
    echo "  - $service success-json-schema ratio ${success_json_schema_ratio}% is below ${OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO}%"
    return 1
  fi
  if ! ratio_meets_threshold "$request_body_json_schema_ratio" "$OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO"; then
    echo "  - $service requestBody-json-schema ratio ${request_body_json_schema_ratio}% is below ${OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO}%"
    return 1
  fi
  return 0
}

if ! check_service_metrics \
  "main backend" \
  "${main_operation_id_ratio:-0}" \
  "${main_operation_id_uniqueness_ratio:-0}" \
  "${main_success_json_schema_ratio:-0}" \
  "${main_request_body_json_schema_ratio:-0}"; then
  main_ok="false"
fi

if ! check_service_metrics \
  "memory backend" \
  "${memory_operation_id_ratio:-0}" \
  "${memory_operation_id_uniqueness_ratio:-0}" \
  "${memory_success_json_schema_ratio:-0}" \
  "${memory_request_body_json_schema_ratio:-0}"; then
  memory_ok="false"
fi

if [[ "$main_ok" == "true" && "$memory_ok" == "true" ]]; then
  echo "[OK] OpenAPI semantic required gate passed."
  exit 0
fi

waiver_file="$(resolve_repo_path "$OPENAPI_GATE_WAIVER_FILE")"
if validate_waiver_if_present "$waiver_file"; then
  echo "[WARN] OpenAPI semantic required gate bypassed by active waiver."
  exit 0
fi

echo "[ERROR] OpenAPI semantic required gate failed."
echo "[INFO] Fix semantic gaps or apply a time-bounded emergency waiver:"
echo "       $waiver_file"
exit 1
