#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BASE_REF="${1:-${OPENAPI_DIFF_BASE:-}}"
HEAD_REF="${2:-${OPENAPI_DIFF_HEAD:-HEAD}}"

run_step() {
  local title="$1"
  shift
  echo "[STEP] $title"
  "$@"
}

resolve_base_ref() {
  local raw="$1"

  if [[ -n "$raw" && "$raw" != "0000000000000000000000000000000000000000" ]]; then
    if git rev-parse --verify "$raw^{commit}" >/dev/null 2>&1; then
      echo "$raw"
      return
    fi
    if git rev-parse --verify "origin/$raw^{commit}" >/dev/null 2>&1; then
      echo "origin/$raw"
      return
    fi
  fi

  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    git merge-base HEAD origin/main
    return
  fi

  if git rev-parse --verify origin/master >/dev/null 2>&1; then
    git merge-base HEAD origin/master
    return
  fi

  if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
    echo "HEAD~1"
    return
  fi

  git rev-parse HEAD
}

resolve_head_ref() {
  local raw="$1"
  if [[ -z "$raw" ]]; then
    echo "HEAD"
    return
  fi
  if git rev-parse --verify "$raw^{commit}" >/dev/null 2>&1; then
    echo "$raw"
    return
  fi
  if git rev-parse --verify "origin/$raw^{commit}" >/dev/null 2>&1; then
    echo "origin/$raw"
    return
  fi
  echo "HEAD"
}

run_full_bundle() {
  echo "[WARN] Falling back to conservative full OpenAPI checks."
  run_step "Verify OpenAPI assembly" \
    bash "$ROOT_DIR/scripts/check_openapi_contract_assembly.sh"
  run_step "OpenAPI fragment owner policy" \
    bash "$ROOT_DIR/scripts/check_openapi_fragment_owner_policy.sh" "$BASE_REF" "$HEAD_REF"
  run_step "OpenAPI ownership consistency" \
    bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh"
  run_step "OpenAPI owner report schema" \
    bash "$ROOT_DIR/scripts/check_openapi_owner_report_schema.sh" "$BASE_REF" "$HEAD_REF"
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
  run_step "API surface baseline" \
    bash "$ROOT_DIR/scripts/check_api_surface.sh"
  run_step "API path baseline" \
    bash "$ROOT_DIR/scripts/check_api_path_baseline.sh"
}

BASE_REF="$(resolve_base_ref "$BASE_REF")"
HEAD_REF="$(resolve_head_ref "$HEAD_REF")"

echo "[INFO] OpenAPI fast diff check range: $BASE_REF..$HEAD_REF"

CHANGED_FILES=()
if ! while IFS= read -r line; do CHANGED_FILES+=("$line"); done < <(git diff --name-only "$BASE_REF" "$HEAD_REF"); then
  run_full_bundle
  echo "[OK] OpenAPI fast diff check passed via full fallback."
  exit 0
fi

if [[ "${#CHANGED_FILES[@]}" -eq 0 ]]; then
  echo "[OK] No changes detected in diff range; skipping OpenAPI fast checks."
  exit 0
fi

touches_openapi_files="false"
touches_fragments="false"
touches_gate_scripts="false"
touches_policy="false"
touches_owner_governance="false"
touches_baseline_scripts="false"
touches_backend_code="false"
touches_docs_only="true"
unknown_or_high_risk="false"

for file in "${CHANGED_FILES[@]}"; do
  case "$file" in
    .github/api-contract/fragments/*)
      touches_openapi_files="true"
      touches_fragments="true"
      touches_docs_only="false"
      ;;
    .github/api-contract/*.openapi.yaml)
      touches_openapi_files="true"
      touches_docs_only="false"
      ;;
    .github/api-contract/openapi-gate-policy.env)
      touches_openapi_files="true"
      touches_policy="true"
      touches_docs_only="false"
      ;;
    .github/api-contract/OWNERSHIP_MAP.md|.github/CODEOWNERS.openapi|.github/api-contract/ownership/manifest.yaml|.github/api-contract/ownership/owner-report.schema.json|.github/api-contract/ownership/governance-bundle-index.schema.json)
      touches_openapi_files="true"
      touches_owner_governance="true"
      touches_docs_only="false"
      ;;
    .github/api-contract/*.md|.github/api-contract/fragments/*.md|.github/api-contract/ownership/*.md)
      ;;
    .github/api-path-baseline.txt|.github/api-surface-baseline.txt)
      touches_baseline_scripts="true"
      touches_docs_only="false"
      ;;
    scripts/check_openapi_contract_*|scripts/check_openapi_method_contract_gate.sh|scripts/assemble_openapi_contracts.py|scripts/update_openapi_contract_assembly.sh|scripts/check_openapi_contract_assembly.sh|scripts/check_openapi_fragment_owner_policy.sh|scripts/check_openapi_ownership_map_consistency.sh|scripts/check_openapi_owner_report_schema.sh|scripts/check_openapi_governance_bundle_integrity.sh|scripts/report_openapi_fragment_owners.sh|scripts/generate_openapi_contract_change_summary.sh|scripts/generate_openapi_ownership_drift_trend.sh|scripts/generate_openapi_governance_bundle.sh|scripts/precommit_openapi_contracts.sh|scripts/check_openapi_contract_fast_diff.sh)
      touches_openapi_files="true"
      touches_gate_scripts="true"
      touches_owner_governance="true"
      touches_docs_only="false"
      ;;
    scripts/check_api_surface.sh|scripts/check_api_path_baseline.sh|scripts/generate_api_surface_snapshot.sh|scripts/generate_api_path_snapshot.sh|scripts/update_api_surface_baseline.sh|scripts/update_api_path_baseline.sh)
      touches_baseline_scripts="true"
      touches_docs_only="false"
      ;;
    chat_app_server_rs/src/*|memory_server/backend/src/*|chat_app_server_rs/Cargo.toml|chat_app_server_rs/Cargo.lock|memory_server/backend/Cargo.toml|memory_server/backend/Cargo.lock)
      touches_backend_code="true"
      touches_docs_only="false"
      ;;
    .github/workflows/ci.yml|.github/pull_request_template.md|PROJECT_HIGH_VALUE_OPTIMIZATION_PLAN_2026-04-10.md)
      touches_docs_only="false"
      ;;
    *)
      unknown_or_high_risk="true"
      touches_docs_only="false"
      ;;
  esac
done

if [[ "$touches_docs_only" == "true" ]]; then
  echo "[OK] Diff touches only OpenAPI documentation markdown; skipping fast checks."
  exit 0
fi

if [[ "$unknown_or_high_risk" == "true" ]]; then
  run_full_bundle
  echo "[OK] OpenAPI fast diff check passed via high-risk fallback."
  exit 0
fi

if [[ "$touches_fragments" == "true" || "$touches_openapi_files" == "true" ]]; then
  run_step "Verify OpenAPI assembly" \
    bash "$ROOT_DIR/scripts/check_openapi_contract_assembly.sh"
fi

if [[ "$touches_fragments" == "true" || "$touches_owner_governance" == "true" ]]; then
  run_step "OpenAPI fragment owner policy" \
    bash "$ROOT_DIR/scripts/check_openapi_fragment_owner_policy.sh" "$BASE_REF" "$HEAD_REF"
  run_step "OpenAPI ownership consistency" \
    bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh"
  run_step "OpenAPI owner report schema" \
    bash "$ROOT_DIR/scripts/check_openapi_owner_report_schema.sh" "$BASE_REF" "$HEAD_REF"
fi

if [[ "$touches_openapi_files" == "true" || "$touches_gate_scripts" == "true" || "$touches_policy" == "true" || "$touches_backend_code" == "true" ]]; then
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
fi

if [[ "$touches_backend_code" == "true" || "$touches_baseline_scripts" == "true" ]]; then
  run_step "API surface baseline" \
    bash "$ROOT_DIR/scripts/check_api_surface.sh"
  run_step "API path baseline" \
    bash "$ROOT_DIR/scripts/check_api_path_baseline.sh"
fi

echo "[OK] OpenAPI fast diff check passed."
