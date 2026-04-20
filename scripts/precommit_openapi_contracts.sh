#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAST_MODE="${OPENAPI_PRECOMMIT_FAST_MODE:-false}"

run_step() {
  local title="$1"
  shift
  echo "[STEP] $title"
  "$@"
}

if [[ "$FAST_MODE" == "true" ]]; then
  run_step "OpenAPI diff-scoped fast checks" \
    bash "$ROOT_DIR/scripts/check_openapi_contract_fast_diff.sh"
  echo "[OK] OpenAPI precommit quick-run passed (fast mode)."
  exit 0
fi

run_step "Rebuild assembled OpenAPI contracts" \
  bash "$ROOT_DIR/scripts/update_openapi_contract_assembly.sh"
run_step "Verify assembled OpenAPI contracts" \
  bash "$ROOT_DIR/scripts/check_openapi_contract_assembly.sh"
run_step "OpenAPI fragment owner policy gate" \
  bash "$ROOT_DIR/scripts/check_openapi_fragment_owner_policy.sh"
run_step "OpenAPI ownership consistency gate" \
  bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh"
run_step "OpenAPI owner report schema gate" \
  bash "$ROOT_DIR/scripts/check_openapi_owner_report_schema.sh"

run_step "OpenAPI advisory coverage" \
  bash "$ROOT_DIR/scripts/check_openapi_contract_advisory.sh"
run_step "OpenAPI required coverage gate" \
  bash "$ROOT_DIR/scripts/check_openapi_contract_gate.sh"
run_step "OpenAPI method coverage gate" \
  bash "$ROOT_DIR/scripts/check_openapi_method_contract_gate.sh"
run_step "OpenAPI structural quality gate" \
  bash "$ROOT_DIR/scripts/check_openapi_contract_quality_gate.sh"
run_step "OpenAPI semantic quality gate" \
  bash "$ROOT_DIR/scripts/check_openapi_contract_semantic_gate.sh"

echo "[OK] OpenAPI precommit quick-run passed."
