#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCHEMA_FILE="${OPENAPI_OWNER_REPORT_SCHEMA_FILE:-$ROOT_DIR/.github/api-contract/ownership/owner-report.schema.json}"

JSON_FILE=""
BASE_REF=""
HEAD_REF=""
GENERATED_JSON_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --file)
      if [[ $# -lt 2 ]]; then
        echo "[ERROR] Missing value for --file" >&2
        exit 2
      fi
      JSON_FILE="$2"
      shift 2
      ;;
    *)
      if [[ -z "$BASE_REF" ]]; then
        BASE_REF="$1"
      elif [[ -z "$HEAD_REF" ]]; then
        HEAD_REF="$1"
      else
        echo "[ERROR] Unexpected extra argument: $1" >&2
        exit 2
      fi
      shift
      ;;
  esac
done

if [[ ! -f "$SCHEMA_FILE" ]]; then
  echo "[ERROR] Missing owner report schema file: $SCHEMA_FILE" >&2
  exit 1
fi

if [[ -z "$JSON_FILE" ]]; then
  BASE_REF="${BASE_REF:-${OPENAPI_DIFF_BASE:-}}"
  HEAD_REF="${HEAD_REF:-${OPENAPI_DIFF_HEAD:-HEAD}}"
  GENERATED_JSON_FILE="$(mktemp)"
  bash "$ROOT_DIR/scripts/report_openapi_fragment_owners.sh" "$BASE_REF" "$HEAD_REF" --json > "$GENERATED_JSON_FILE"
  JSON_FILE="$GENERATED_JSON_FILE"
fi

if [[ ! -f "$JSON_FILE" ]]; then
  echo "[ERROR] Owner report JSON file not found: $JSON_FILE" >&2
  exit 1
fi

SCHEMA_FILE="$SCHEMA_FILE" python3 - "$JSON_FILE" <<'PY'
from pathlib import Path
import json
import os
import sys

json_file = Path(sys.argv[1])
schema_file = Path(os.environ["SCHEMA_FILE"])

schema = json.loads(schema_file.read_text(encoding="utf-8"))
data = json.loads(json_file.read_text(encoding="utf-8"))

errors = []

def expect(condition: bool, message: str):
    if not condition:
        errors.append(message)

expect(isinstance(data, dict), "root must be an object")
if not isinstance(data, dict):
    for err in errors:
        print(f"[ERROR] {err}")
    sys.exit(1)

required_root_keys = [
    "schema_version",
    "diff_range",
    "codeowners_file",
    "ownership_manifest_file",
    "changed_fragment_count",
    "fragments",
    "footprint_summary",
    "policy",
]
for key in required_root_keys:
    expect(key in data, f"missing root key `{key}`")

expected_schema_version = (
    schema.get("properties", {})
    .get("schema_version", {})
    .get("const", "openapi.owner_report.v1")
)
expect(
    data.get("schema_version") == expected_schema_version,
    f"schema_version must be `{expected_schema_version}`",
)

diff_range = data.get("diff_range")
expect(isinstance(diff_range, dict), "`diff_range` must be an object")
if isinstance(diff_range, dict):
    expect(isinstance(diff_range.get("base_ref"), str), "`diff_range.base_ref` must be a string")
    expect(isinstance(diff_range.get("head_ref"), str), "`diff_range.head_ref` must be a string")

expect(isinstance(data.get("codeowners_file"), str), "`codeowners_file` must be a string")
expect(
    isinstance(data.get("ownership_manifest_file"), str),
    "`ownership_manifest_file` must be a string",
)

changed_fragment_count = data.get("changed_fragment_count")
expect(isinstance(changed_fragment_count, int), "`changed_fragment_count` must be an integer")
if isinstance(changed_fragment_count, int):
    expect(changed_fragment_count >= 0, "`changed_fragment_count` must be >= 0")

fragments = data.get("fragments")
expect(isinstance(fragments, list), "`fragments` must be an array")
allowed_owner_footprints = {"mapped", "fallback"}
allowed_codeowners_footprints = {"mapped", "missing_entry", "missing_file"}
if isinstance(fragments, list):
    for idx, row in enumerate(fragments):
        expect(isinstance(row, dict), f"`fragments[{idx}]` must be an object")
        if not isinstance(row, dict):
            continue
        for key in ["file", "owner_hint", "owner_footprint", "codeowners_footprint"]:
            expect(key in row, f"`fragments[{idx}]` missing key `{key}`")
        expect(isinstance(row.get("file"), str), f"`fragments[{idx}].file` must be a string")
        expect(isinstance(row.get("owner_hint"), str), f"`fragments[{idx}].owner_hint` must be a string")
        expect(
            row.get("owner_footprint") in allowed_owner_footprints,
            f"`fragments[{idx}].owner_footprint` must be one of {sorted(allowed_owner_footprints)}",
        )
        expect(
            row.get("codeowners_footprint") in allowed_codeowners_footprints,
            f"`fragments[{idx}].codeowners_footprint` must be one of {sorted(allowed_codeowners_footprints)}",
        )

footprint_summary = data.get("footprint_summary")
expect(isinstance(footprint_summary, dict), "`footprint_summary` must be an object")
owner_mapped = None
owner_fallback = None
if isinstance(footprint_summary, dict):
    owner_mapped = footprint_summary.get("owner_mapped")
    owner_fallback = footprint_summary.get("owner_fallback")
    codeowners_summary = footprint_summary.get("codeowners")
    expect(isinstance(owner_mapped, int), "`footprint_summary.owner_mapped` must be an integer")
    expect(isinstance(owner_fallback, int), "`footprint_summary.owner_fallback` must be an integer")
    if isinstance(owner_mapped, int):
        expect(owner_mapped >= 0, "`footprint_summary.owner_mapped` must be >= 0")
    if isinstance(owner_fallback, int):
        expect(owner_fallback >= 0, "`footprint_summary.owner_fallback` must be >= 0")
    expect(isinstance(codeowners_summary, dict), "`footprint_summary.codeowners` must be an object")
    if isinstance(codeowners_summary, dict):
        for key, value in codeowners_summary.items():
            expect(isinstance(key, str), "all codeowners summary keys must be strings")
            expect(
                isinstance(value, int) and value >= 0,
                f"`footprint_summary.codeowners[{key}]` must be an integer >= 0",
            )

policy = data.get("policy")
expect(isinstance(policy, dict), "`policy` must be an object")
if isinstance(policy, dict):
    strict_mode = policy.get("strict_mode")
    status = policy.get("status")
    error_count = policy.get("error_count")
    policy_errors = policy.get("errors")

    expect(isinstance(strict_mode, bool), "`policy.strict_mode` must be a boolean")
    expect(status in {"not_checked", "passed", "failed"}, "`policy.status` must be not_checked|passed|failed")
    expect(isinstance(error_count, int), "`policy.error_count` must be an integer")
    if isinstance(error_count, int):
        expect(error_count >= 0, "`policy.error_count` must be >= 0")
    expect(isinstance(policy_errors, list), "`policy.errors` must be an array")
    if isinstance(policy_errors, list):
        for idx, item in enumerate(policy_errors):
            expect(isinstance(item, str), f"`policy.errors[{idx}]` must be a string")
        if isinstance(error_count, int):
            expect(error_count == len(policy_errors), "`policy.error_count` must equal len(policy.errors)")

    if strict_mode is True:
        if isinstance(error_count, int):
            if error_count == 0:
                expect(status == "passed", "strict mode with zero errors must have status `passed`")
            else:
                expect(status == "failed", "strict mode with errors must have status `failed`")
    if strict_mode is False:
        expect(status == "not_checked", "non-strict mode must have status `not_checked`")

if isinstance(changed_fragment_count, int) and isinstance(fragments, list):
    expect(
        changed_fragment_count == len(fragments),
        "`changed_fragment_count` must equal len(fragments)",
    )
if isinstance(fragments, list) and isinstance(owner_mapped, int) and isinstance(owner_fallback, int):
    expect(
        owner_mapped + owner_fallback == len(fragments),
        "`owner_mapped + owner_fallback` must equal len(fragments)",
    )

if errors:
    print("[ERROR] OpenAPI owner report schema validation failed.")
    for err in errors:
        print(f"  - {err}")
    print(f"[INFO] JSON file: {json_file}")
    print(f"[INFO] Schema file: {schema_file}")
    sys.exit(1)

print("[OK] OpenAPI owner report schema validation passed.")
print(f"  schema version: {data['schema_version']}")
print(f"  json file:       {json_file}")
PY

if [[ -n "$GENERATED_JSON_FILE" ]]; then
  rm -f "$GENERATED_JSON_FILE"
fi
