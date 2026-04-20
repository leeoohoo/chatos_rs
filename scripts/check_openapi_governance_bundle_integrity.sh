#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="${1:-${OPENAPI_GOVERNANCE_BUNDLE_DIR:-$ROOT_DIR/.github/api-contract/ownership/OPENAPI_GOVERNANCE_BUNDLE}}"
SCHEMA_FILE="${OPENAPI_GOVERNANCE_BUNDLE_INDEX_SCHEMA_FILE:-$ROOT_DIR/.github/api-contract/ownership/governance-bundle-index.schema.json}"

if [[ ! -d "$BUNDLE_DIR" ]]; then
  echo "[ERROR] Missing governance bundle directory: $BUNDLE_DIR" >&2
  exit 1
fi

if [[ ! -f "$SCHEMA_FILE" ]]; then
  echo "[ERROR] Missing governance bundle schema file: $SCHEMA_FILE" >&2
  exit 1
fi

python3 - "$BUNDLE_DIR" "$SCHEMA_FILE" <<'PY'
from pathlib import Path
import json
import sys

bundle_dir = Path(sys.argv[1]).resolve()
schema_file = Path(sys.argv[2]).resolve()

errors = []

required_files = [
    "contract-change-summary.md",
    "owner-report.json",
    "ownership-discrepancy.json",
    "ownership-drift-trend.md",
    "ownership-drift-trend.json",
    "pr-comment-draft.md",
    "artifact-index.json",
    "GOVERNANCE_SUMMARY.md",
]

for file_name in required_files:
    p = bundle_dir / file_name
    if not p.exists():
        errors.append(f"missing required bundle file `{file_name}`")

index_path = bundle_dir / "artifact-index.json"
if not index_path.exists():
    print("[ERROR] OpenAPI governance bundle integrity check failed.")
    for err in errors:
        print(f"  - {err}")
    print(f"[INFO] Bundle dir: {bundle_dir}")
    sys.exit(1)

try:
    schema_obj = json.loads(schema_file.read_text(encoding="utf-8"))
except Exception as exc:
    print("[ERROR] Failed to parse governance bundle schema.")
    print(f"  - {exc}")
    print(f"[INFO] Schema file: {schema_file}")
    sys.exit(1)

try:
    index_obj = json.loads(index_path.read_text(encoding="utf-8"))
except Exception as exc:
    print("[ERROR] Failed to parse artifact-index.json.")
    print(f"  - {exc}")
    print(f"[INFO] Index file: {index_path}")
    sys.exit(1)

expected_index_schema = (
    schema_obj.get("properties", {})
    .get("schema_version", {})
    .get("const", "openapi.governance_bundle_index.v1")
)
if index_obj.get("schema_version") != expected_index_schema:
    errors.append(
        f"artifact-index schema_version mismatch: expected `{expected_index_schema}`, "
        f"got `{index_obj.get('schema_version')}`"
    )

for key in ["generated_at", "diff_range", "aggregate", "artifacts"]:
    if key not in index_obj:
        errors.append(f"artifact-index missing key `{key}`")

diff_range = index_obj.get("diff_range")
if not isinstance(diff_range, dict):
    errors.append("artifact-index `diff_range` must be an object")
else:
    if not isinstance(diff_range.get("base_ref"), str):
        errors.append("artifact-index `diff_range.base_ref` must be a string")
    if not isinstance(diff_range.get("head_ref"), str):
        errors.append("artifact-index `diff_range.head_ref` must be a string")

artifacts = index_obj.get("artifacts")
if not isinstance(artifacts, dict):
    errors.append("artifact-index `artifacts` must be an object")
    artifacts = {}

expected_artifacts = {
    "contract_change_summary": "contract-change-summary.md",
    "owner_report": "owner-report.json",
    "ownership_discrepancy": "ownership-discrepancy.json",
    "ownership_drift_trend_markdown": "ownership-drift-trend.md",
    "ownership_drift_trend": "ownership-drift-trend.json",
    "pr_comment_draft": "pr-comment-draft.md",
}

for key, expected_file in expected_artifacts.items():
    artifact = artifacts.get(key)
    if not isinstance(artifact, dict):
        errors.append(f"artifact-index missing artifact entry `{key}`")
        continue
    actual_file = artifact.get("file")
    if actual_file != expected_file:
        errors.append(f"artifact `{key}` file mismatch: expected `{expected_file}`, got `{actual_file}`")
    path = bundle_dir / expected_file
    actual_exists = path.exists()
    if artifact.get("exists") is not actual_exists:
        errors.append(f"artifact `{key}` exists flag mismatch for `{expected_file}`")
    bytes_value = artifact.get("bytes")
    if actual_exists:
        if not isinstance(bytes_value, int) or bytes_value <= 0:
            errors.append(f"artifact `{key}` invalid byte size for `{expected_file}`")

expected_json_schema_versions = {
    "owner-report.json": "openapi.owner_report.v1",
    "ownership-discrepancy.json": "openapi.ownership_discrepancy.v1",
    "ownership-drift-trend.json": "openapi.ownership_drift_trend.v1",
}

for file_name, expected_schema in expected_json_schema_versions.items():
    path = bundle_dir / file_name
    if not path.exists():
        continue
    try:
        obj = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        errors.append(f"failed to parse `{file_name}`: {exc}")
        continue
    if obj.get("schema_version") != expected_schema:
        errors.append(
            f"`{file_name}` schema_version mismatch: expected `{expected_schema}`, got `{obj.get('schema_version')}`"
        )

summary_path = bundle_dir / "GOVERNANCE_SUMMARY.md"
if summary_path.exists():
    summary_text = summary_path.read_text(encoding="utf-8")
    if "OpenAPI Governance Bundle" not in summary_text:
        errors.append("`GOVERNANCE_SUMMARY.md` missing expected title")

if errors:
    print("[ERROR] OpenAPI governance bundle integrity check failed.")
    for err in errors:
        print(f"  - {err}")
    print(f"[INFO] Bundle dir: {bundle_dir}")
    print(f"[INFO] Index schema: {schema_file}")
    sys.exit(1)

print("[OK] OpenAPI governance bundle integrity check passed.")
print(f"  bundle dir: {bundle_dir}")
print(f"  index schema: {schema_file}")
PY
