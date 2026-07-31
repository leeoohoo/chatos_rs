#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

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

load_openapi_policy_file() {
  local policy_file="$1"
  if [[ ! -f "$policy_file" ]]; then
    echo "[WARN] Missing policy file: $policy_file"
    echo "[WARN] Falling back to advisory mode."
    return
  fi

  # shellcheck disable=SC1090
  source "$policy_file"
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

  echo "[WARN] ${OPENAPI_GATE_WAIVER_LABEL:-OpenAPI gate} is waived by emergency exception."
  echo "  waiver_id: $OPENAPI_GATE_WAIVER_ID"
  echo "  approver:  $OPENAPI_GATE_WAIVER_APPROVER"
  echo "  expires:   $OPENAPI_GATE_WAIVER_EXPIRES_AT"
  echo "  reason:    $OPENAPI_GATE_WAIVER_REASON"
  echo "  max_hours: $OPENAPI_GATE_WAIVER_MAX_HOURS"
  return 0
}
