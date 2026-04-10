#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BASE_REF="${1:-${OPENAPI_DIFF_BASE:-}}"
HEAD_REF="${2:-${OPENAPI_DIFF_HEAD:-HEAD}}"
OUTPUT_FILE="${3:-${OPENAPI_CHANGE_SUMMARY_OUTPUT:-$ROOT_DIR/.github/api-contract/OPENAPI_CHANGE_SUMMARY.md}}"
JSON_OUTPUT_FILE="${4:-${OPENAPI_CHANGE_SUMMARY_JSON_OUTPUT:-$ROOT_DIR/.github/api-contract/OPENAPI_CHANGE_SUMMARY.json}}"
PR_COMMENT_OUTPUT_FILE="${5:-${OPENAPI_PR_COMMENT_BODY_OUTPUT:-}}"

resolve_base_ref() {
  local raw="$1"

  if [[ -z "$raw" || "$raw" == "0000000000000000000000000000000000000000" ]]; then
    if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
      echo "HEAD~1"
    else
      git rev-parse HEAD
    fi
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

  if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
    echo "HEAD~1"
  else
    git rev-parse HEAD
  fi
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

BASE_REF="$(resolve_base_ref "$BASE_REF")"
HEAD_REF="$(resolve_head_ref "$HEAD_REF")"

mkdir -p "$(dirname "$OUTPUT_FILE")"
mkdir -p "$(dirname "$JSON_OUTPUT_FILE")"
if [[ -n "$PR_COMMENT_OUTPUT_FILE" ]]; then
  mkdir -p "$(dirname "$PR_COMMENT_OUTPUT_FILE")"
fi

CHANGED_GOV_FILES=()
while IFS= read -r line; do
  CHANGED_GOV_FILES+=("$line")
done < <(
  git diff --name-status "$BASE_REF" "$HEAD_REF" -- .github/api-contract scripts \
    | awk '/api-contract|openapi|api_surface|api_path/ {print}' \
    | sort -u
)

POLICY_KEYS=(
  OPENAPI_GATE_MODE
  OPENAPI_MAIN_MIN_COVERAGE_RATIO
  OPENAPI_MEMORY_MIN_COVERAGE_RATIO
  OPENAPI_METHOD_GATE_MODE
  OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO
  OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO
  OPENAPI_QUALITY_GATE_MODE
  OPENAPI_QUALITY_MIN_SUMMARY_RATIO
  OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO
  OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO
  OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO
  OPENAPI_SEMANTIC_GATE_MODE
  OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO
  OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO
  OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO
  OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO
  OPENAPI_GATE_WAIVER_MAX_HOURS
)

bash "$ROOT_DIR/scripts/report_openapi_fragment_owners.sh" "$BASE_REF" "$HEAD_REF" --json > "$JSON_OUTPUT_FILE"

map_lines=()
while IFS= read -r line; do
  map_lines+=("$line")
done < <(bash "$ROOT_DIR/scripts/report_openapi_fragment_owners.sh" "$BASE_REF" "$HEAD_REF" --tsv)

{
  echo "# OpenAPI Contract Change Summary"
  echo
  echo "- Generated at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo "- Diff range: \`$BASE_REF..$HEAD_REF\`"
  echo
  echo "## Changed Governance Files"
  echo
  if [[ "${#CHANGED_GOV_FILES[@]}" -eq 0 ]]; then
    echo "- No OpenAPI governance file changes detected."
  else
    for line in "${CHANGED_GOV_FILES[@]}"; do
      echo "- \`$line\`"
    done
  fi
  echo
  echo "## Changed Fragments And Owner Hints"
  echo
  bash "$ROOT_DIR/scripts/report_openapi_fragment_owners.sh" "$BASE_REF" "$HEAD_REF" --markdown
  echo
  echo "## Reviewer Owner Confirmation Checklist"
  echo
  if [[ "${#map_lines[@]}" -eq 0 ]]; then
    echo "- [ ] N/A: No OpenAPI fragment changes in this diff."
  else
    for row in "${map_lines[@]}"; do
      IFS=$'\t' read -r file owners owner_status codeowners_status <<<"$row"
      echo "- [ ] \`$file\` owner confirmation: $owners"
      echo "  - footprint: owner=$owner_status, codeowners=$codeowners_status"
    done
  fi
  echo
  echo "## Machine-Readable Owner Hints (JSON)"
  echo
  echo "- Artifact: \`$JSON_OUTPUT_FILE\`"
  echo
  echo '```json'
  cat "$JSON_OUTPUT_FILE"
  echo '```'
  echo
  echo "## Current Contract Snapshot (HEAD)"
  echo
  ROOT_DIR="$ROOT_DIR" python3 - <<'PY'
from pathlib import Path
import os
import yaml

ROOT = Path(os.environ['ROOT_DIR'])
files = [
    ('main', ROOT / '.github/api-contract/chat_app_server_rs.openapi.yaml'),
    ('memory', ROOT / '.github/api-contract/memory_server.openapi.yaml'),
]
methods = {'get', 'post', 'put', 'patch', 'delete', 'head', 'options'}

print('| service | paths | operations | operationId count |')
print('| --- | ---: | ---: | ---: |')
for name, path in files:
    data = yaml.safe_load(path.read_text()) or {}
    paths = data.get('paths') or {}
    path_count = len(paths)
    op_count = 0
    op_id_count = 0
    for _, item in paths.items():
        if not isinstance(item, dict):
            continue
        for m, op in item.items():
            if m in methods and isinstance(op, dict):
                op_count += 1
                if isinstance(op.get('operationId'), str) and op.get('operationId').strip():
                    op_id_count += 1
    print(f'| {name} | {path_count} | {op_count} | {op_id_count} |')
PY
  echo
  echo "## Policy Snapshot"
  echo
  for key in "${POLICY_KEYS[@]}"; do
    value="$(awk -F= -v k="$key" '$1==k {print $2}' "$ROOT_DIR/.github/api-contract/openapi-gate-policy.env")"
    if [[ -n "$value" ]]; then
      echo "- \`$key=$value\`"
    fi
  done
} > "$OUTPUT_FILE"

if [[ -n "$PR_COMMENT_OUTPUT_FILE" ]]; then
  {
    echo "<!-- openapi-owner-checklist:start version=v1 -->"
    echo
    echo "### OpenAPI Contract Owner Checklist"
    echo
    echo "- Diff range: \`$BASE_REF..$HEAD_REF\`"
    echo "- Summary artifact: \`$OUTPUT_FILE\`"
    echo "- Owner JSON artifact: \`$JSON_OUTPUT_FILE\`"
    echo
    if [[ "${#map_lines[@]}" -eq 0 ]]; then
      echo "- [ ] N/A: No OpenAPI fragment changes in this diff."
    else
      for row in "${map_lines[@]}"; do
        IFS=$'\t' read -r file owners owner_status codeowners_status <<<"$row"
        echo "- [ ] \`$file\` owner review: $owners"
        echo "  - footprint: owner=$owner_status, codeowners=$codeowners_status"
      done
    fi
    echo
    echo "<!-- openapi-owner-checklist:end -->"
  } > "$PR_COMMENT_OUTPUT_FILE"
fi

echo "[OK] Generated OpenAPI change summary: $OUTPUT_FILE"
echo "[OK] Generated OpenAPI owner JSON summary: $JSON_OUTPUT_FILE"
if [[ -n "$PR_COMMENT_OUTPUT_FILE" ]]; then
  echo "[OK] Generated OpenAPI PR comment draft: $PR_COMMENT_OUTPUT_FILE"
fi
