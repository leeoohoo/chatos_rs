#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWNERSHIP_MANIFEST_FILE="${OPENAPI_OWNERSHIP_MANIFEST_FILE:-$ROOT_DIR/.github/api-contract/ownership/manifest.yaml}"
OWNERSHIP_MAP_FILE="${OPENAPI_OWNERSHIP_MAP_FILE:-$ROOT_DIR/.github/api-contract/OWNERSHIP_MAP.md}"
CODEOWNERS_FILE="${OPENAPI_CODEOWNERS_FILE:-$ROOT_DIR/.github/CODEOWNERS.openapi}"
FRAGMENTS_DIR="${OPENAPI_FRAGMENTS_DIR:-$ROOT_DIR/.github/api-contract/fragments}"
JSON_OUT_FILE="${OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT:-}"
JSON_STDOUT="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json-out)
      if [[ $# -lt 2 ]]; then
        echo "[ERROR] Missing value for --json-out" >&2
        exit 2
      fi
      JSON_OUT_FILE="$2"
      shift 2
      ;;
    --json-stdout)
      JSON_STDOUT="true"
      shift
      ;;
    *)
      echo "[ERROR] Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -n "$JSON_OUT_FILE" ]]; then
  mkdir -p "$(dirname "$JSON_OUT_FILE")"
fi

ROOT_DIR="$ROOT_DIR" \
  OWNERSHIP_MANIFEST_FILE="$OWNERSHIP_MANIFEST_FILE" \
  OWNERSHIP_MAP_FILE="$OWNERSHIP_MAP_FILE" \
  CODEOWNERS_FILE="$CODEOWNERS_FILE" \
  FRAGMENTS_DIR="$FRAGMENTS_DIR" \
  JSON_OUT_FILE="$JSON_OUT_FILE" \
  JSON_STDOUT="$JSON_STDOUT" \
  python3 - <<'PY'
from pathlib import Path
import json
import os
import sys
import yaml

root_dir = Path(os.environ["ROOT_DIR"]).resolve()
ownership_manifest_file = Path(os.environ["OWNERSHIP_MANIFEST_FILE"]).resolve()
ownership_map_file = Path(os.environ["OWNERSHIP_MAP_FILE"]).resolve()
codeowners_file = Path(os.environ["CODEOWNERS_FILE"]).resolve()
fragments_dir = Path(os.environ["FRAGMENTS_DIR"]).resolve()
json_out_file = (os.environ.get("JSON_OUT_FILE") or "").strip()
json_stdout = os.environ.get("JSON_STDOUT", "false") == "true"

ALLOWED_SEVERITIES = ("missing_fragment", "missing_codeowner", "scope_mismatch")

issues = []

def add_issue(severity: str, code: str, message: str):
    if severity not in ALLOWED_SEVERITIES:
        severity = "scope_mismatch"
    issues.append(
        {
            "severity": severity,
            "code": code,
            "message": message,
        }
    )


def parse_ownership_map(path: Path):
    scope_by_prefix = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line.startswith("|"):
            continue
        cols = [c.strip() for c in line.strip("|").split("|")]
        if len(cols) < 3:
            continue
        path_col = cols[0]
        scope_col = cols[1]
        if path_col.lower() == "path prefix":
            continue
        if set(path_col.replace(" ", "")) == {"-"}:
            continue
        if "/api/" not in path_col:
            continue
        prefixes = [p.strip().strip("`") for p in path_col.split(",")]
        scope = scope_col.strip().strip("`")
        for prefix in prefixes:
            if prefix:
                scope_by_prefix[prefix] = scope
    return scope_by_prefix


def parse_codeowners(path: Path):
    entries = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        entries[parts[0]] = parts[1:]
    return entries


missing_preconditions = False
if not ownership_manifest_file.is_file():
    missing_preconditions = True
    add_issue(
        "missing_fragment",
        "missing_ownership_manifest",
        f"Missing ownership manifest `{ownership_manifest_file}`",
    )
if not ownership_map_file.is_file():
    missing_preconditions = True
    add_issue(
        "scope_mismatch",
        "missing_ownership_map",
        f"Missing ownership map `{ownership_map_file}`",
    )
if not codeowners_file.is_file():
    missing_preconditions = True
    add_issue(
        "missing_codeowner",
        "missing_codeowners_file",
        f"Missing OpenAPI CODEOWNERS file `{codeowners_file}`",
    )
if not fragments_dir.is_dir():
    missing_preconditions = True
    add_issue(
        "missing_fragment",
        "missing_fragments_dir",
        f"Missing OpenAPI fragments directory `{fragments_dir}`",
    )

manifest_file_to_codeowners = {}
manifest_prefix_to_scope = {}
scope_by_prefix = {}
codeowners_entries = {}
fragment_files = []

if not missing_preconditions:
    manifest_data = yaml.safe_load(ownership_manifest_file.read_text(encoding="utf-8")) or {}
    manifest_fragments = manifest_data.get("fragments") or []

    for item in manifest_fragments:
        if not isinstance(item, dict):
            add_issue(
                "missing_fragment",
                "manifest_fragment_not_object",
                "Ownership manifest contains non-object fragment item",
            )
            continue

        file_path = str(item.get("file") or "").strip()
        if not file_path:
            add_issue(
                "missing_fragment",
                "manifest_fragment_missing_file",
                "Ownership manifest contains fragment entry without `file`",
            )
            continue
        if file_path in manifest_file_to_codeowners:
            add_issue(
                "missing_fragment",
                "manifest_fragment_duplicate",
                f"Ownership manifest has duplicate fragment file `{file_path}`",
            )
            continue

        expected_owners = []
        for owner in item.get("codeowners") or []:
            owner_text = str(owner).strip()
            if owner_text:
                expected_owners.append(owner_text)
        if len(expected_owners) == 0:
            add_issue(
                "missing_codeowner",
                "manifest_fragment_empty_codeowners",
                f"Ownership manifest fragment `{file_path}` has empty `codeowners`",
            )

        manifest_file_to_codeowners[file_path] = expected_owners

        for rule in item.get("ownership_map_rules") or []:
            if not isinstance(rule, dict):
                add_issue(
                    "scope_mismatch",
                    "manifest_scope_rule_not_object",
                    f"Ownership manifest fragment `{file_path}` has non-object ownership_map_rules item",
                )
                continue
            path_prefix = str(rule.get("path_prefix") or "").strip()
            owner_scope = str(rule.get("owner_scope") or "").strip()
            if not path_prefix or not owner_scope:
                add_issue(
                    "scope_mismatch",
                    "manifest_scope_rule_invalid",
                    f"Ownership manifest fragment `{file_path}` has invalid ownership_map_rules item "
                    f"(path_prefix=`{path_prefix}`, owner_scope=`{owner_scope}`)",
                )
                continue
            existing_scope = manifest_prefix_to_scope.get(path_prefix)
            if existing_scope is not None and existing_scope != owner_scope:
                add_issue(
                    "scope_mismatch",
                    "manifest_scope_rule_conflict",
                    f"Ownership manifest has conflicting owner_scope for `{path_prefix}`: "
                    f"`{existing_scope}` vs `{owner_scope}`",
                )
                continue
            manifest_prefix_to_scope[path_prefix] = owner_scope

    scope_by_prefix = parse_ownership_map(ownership_map_file)
    codeowners_entries = parse_codeowners(codeowners_file)

    for fragment in sorted(fragments_dir.rglob("*.yaml")):
        rel = str(fragment.relative_to(root_dir)).replace("\\", "/")
        fragment_files.append(rel)

    fragment_set = set(fragment_files)
    manifest_fragment_set = set(manifest_file_to_codeowners.keys())

    for fragment in fragment_files:
        if fragment not in manifest_fragment_set:
            add_issue(
                "missing_fragment",
                "fragment_missing_in_manifest",
                f"Ownership manifest missing fragment entry `{fragment}`",
            )

    for fragment in sorted(manifest_fragment_set):
        if fragment not in fragment_set:
            add_issue(
                "missing_fragment",
                "manifest_references_missing_fragment",
                f"Ownership manifest references missing fragment `{fragment}`",
            )

    for fragment in sorted(manifest_fragment_set):
        expected_owners = manifest_file_to_codeowners.get(fragment) or []
        owners = codeowners_entries.get(fragment)
        if owners is None:
            add_issue(
                "missing_codeowner",
                "codeowners_missing_fragment_entry",
                f"CODEOWNERS missing fragment entry `{fragment}`",
            )
            continue
        missing_owners = [owner for owner in expected_owners if owner not in owners]
        if missing_owners:
            add_issue(
                "missing_codeowner",
                "codeowners_owner_mismatch",
                f"CODEOWNERS owner mismatch for `{fragment}`: expected to include "
                f"`{', '.join(expected_owners)}`, got `{', '.join(owners)}`",
            )

    for pattern in sorted(codeowners_entries.keys()):
        if pattern.startswith(".github/api-contract/fragments/") and pattern.endswith(".yaml"):
            if pattern not in manifest_fragment_set:
                add_issue(
                    "missing_codeowner",
                    "codeowners_fragment_not_declared_in_manifest",
                    f"CODEOWNERS fragment entry not declared in ownership manifest `{pattern}`",
                )

    for prefix, expected_scope in sorted(manifest_prefix_to_scope.items()):
        actual_scope = scope_by_prefix.get(prefix)
        if actual_scope is None:
            add_issue(
                "scope_mismatch",
                "ownership_map_missing_prefix",
                f"OWNERSHIP_MAP missing path prefix `{prefix}` (expected scope `{expected_scope}`)",
            )
        elif actual_scope != expected_scope:
            add_issue(
                "scope_mismatch",
                "ownership_map_scope_mismatch",
                f"OWNERSHIP_MAP scope mismatch for `{prefix}`: expected `{expected_scope}`, got `{actual_scope}`",
            )

    for prefix, scope in sorted(scope_by_prefix.items()):
        if prefix not in manifest_prefix_to_scope:
            add_issue(
                "scope_mismatch",
                "ownership_map_prefix_not_declared_in_manifest",
                f"OWNERSHIP_MAP path prefix not declared in ownership manifest `{prefix}` "
                f"(scope `{scope}`)",
            )

drift_by_severity = {k: 0 for k in ALLOWED_SEVERITIES}
for issue in issues:
    drift_by_severity[issue["severity"]] += 1

payload = {
    "schema_version": "openapi.ownership_discrepancy.v1",
    "status": "failed" if len(issues) > 0 else "passed",
    "files": {
        "ownership_manifest": str(ownership_manifest_file),
        "ownership_map": str(ownership_map_file),
        "codeowners": str(codeowners_file),
        "fragments_dir": str(fragments_dir),
    },
    "drift_count": len(issues),
    "drift_by_severity": drift_by_severity,
    "discrepancies": issues,
}

if json_out_file:
    out_path = Path(json_out_file)
    out_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

if json_stdout:
    print(json.dumps(payload, indent=2, ensure_ascii=False))

if issues:
    stream = sys.stderr if json_stdout else sys.stdout
    print("[ERROR] OpenAPI ownership consistency check failed.", file=stream)
    for issue in issues:
        print(f"  - [{issue['severity']}] {issue['message']}", file=stream)
    print("[INFO] Alignment targets:", file=stream)
    print(f"       ownership manifest: {ownership_manifest_file}", file=stream)
    print(f"       ownership map: {ownership_map_file}", file=stream)
    print(f"       codeowners:    {codeowners_file}", file=stream)
    if json_out_file:
        print(f"[INFO] Discrepancy JSON artifact: {json_out_file}", file=stream)
    sys.exit(1)

if not json_stdout:
    print("[OK] OpenAPI ownership consistency check passed.")
    print(f"  ownership manifest fragments: {len(manifest_file_to_codeowners)}")
    print(f"  path-prefix rules checked:    {len(manifest_prefix_to_scope)}")
    print(f"  fragment entries checked:     {len(fragment_files)}")
    if json_out_file:
        print(f"  discrepancy artifact:         {json_out_file}")
PY
