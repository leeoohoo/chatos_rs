#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BASE_REF="${1:-${OPENAPI_DIFF_BASE:-}}"
HEAD_REF="${2:-${OPENAPI_DIFF_HEAD:-HEAD}}"
OUTPUT_FILE="${3:-${OPENAPI_OWNERSHIP_DRIFT_TREND_OUTPUT:-$ROOT_DIR/.github/api-contract/ownership/OPENAPI_OWNERSHIP_DRIFT_TREND.md}}"
JSON_OUTPUT_FILE="${4:-${OPENAPI_OWNERSHIP_DRIFT_TREND_JSON_OUTPUT:-}}"

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
if [[ -n "$JSON_OUTPUT_FILE" ]]; then
  mkdir -p "$(dirname "$JSON_OUTPUT_FILE")"
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

analyze_ref() {
  local ref="$1"
  local label="$2"
  local meta_file="$3"
  local out_json="$4"
  local snapshot_dir="$TMP_DIR/$label-snapshot"
  local resolved_ref=""
  local current_head=""

  {
    echo "status=unknown"
    echo "note="
    echo "ref=$ref"
  } > "$meta_file"

  if ! resolved_ref="$(git rev-parse --verify "$ref^{commit}" 2>/dev/null)"; then
    {
      echo "status=unavailable"
      echo "note=ref_not_found"
      echo "ref=$ref"
    } > "$meta_file"
    return
  fi

  current_head="$(git rev-parse --verify HEAD)"
  if [[ "$resolved_ref" == "$current_head" && -f "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh" ]]; then
    if OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT="$out_json" \
      bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh" >/dev/null 2>&1; then
      {
        echo "status=passed"
        echo "note=workspace_head"
        echo "ref=$ref"
      } > "$meta_file"
      return
    fi
    if [[ -f "$out_json" ]]; then
      {
        echo "status=failed"
        echo "note=workspace_head"
        echo "ref=$ref"
      } > "$meta_file"
      return
    fi
  fi

  mkdir -p "$snapshot_dir"
  if ! git archive "$ref" | tar -x -C "$snapshot_dir" >/dev/null 2>&1; then
    {
      echo "status=unavailable"
      echo "note=archive_failed"
      echo "ref=$ref"
    } > "$meta_file"
    return
  fi

  if OPENAPI_OWNERSHIP_MANIFEST_FILE="$snapshot_dir/.github/api-contract/ownership/manifest.yaml" \
    OPENAPI_OWNERSHIP_MAP_FILE="$snapshot_dir/.github/api-contract/OWNERSHIP_MAP.md" \
    OPENAPI_CODEOWNERS_FILE="$snapshot_dir/.github/CODEOWNERS.openapi" \
    OPENAPI_FRAGMENTS_DIR="$snapshot_dir/.github/api-contract/fragments" \
    OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT="$out_json" \
    bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh" >/dev/null 2>&1; then
    {
      echo "status=passed"
      echo "note="
      echo "ref=$ref"
    } > "$meta_file"
    return
  fi

  if [[ -f "$out_json" ]]; then
    {
      echo "status=failed"
      echo "note="
      echo "ref=$ref"
    } > "$meta_file"
  else
    {
      echo "status=unavailable"
      echo "note=missing_discrepancy_artifact"
      echo "ref=$ref"
    } > "$meta_file"
  fi
}

BASE_META_FILE="$TMP_DIR/base.meta"
HEAD_META_FILE="$TMP_DIR/head.meta"
BASE_JSON_FILE="$TMP_DIR/base-discrepancy.json"
HEAD_JSON_FILE="$TMP_DIR/head-discrepancy.json"

analyze_ref "$BASE_REF" "base" "$BASE_META_FILE" "$BASE_JSON_FILE"
analyze_ref "$HEAD_REF" "head" "$HEAD_META_FILE" "$HEAD_JSON_FILE"

BASE_REF="$BASE_REF" \
  HEAD_REF="$HEAD_REF" \
  OUTPUT_FILE="$OUTPUT_FILE" \
  JSON_OUTPUT_FILE="$JSON_OUTPUT_FILE" \
  python3 - "$BASE_META_FILE" "$HEAD_META_FILE" "$BASE_JSON_FILE" "$HEAD_JSON_FILE" <<'PY'
from datetime import datetime, timezone
from pathlib import Path
import json
import os
import sys

base_meta_file = Path(sys.argv[1])
head_meta_file = Path(sys.argv[2])
base_json_file = Path(sys.argv[3])
head_json_file = Path(sys.argv[4])

base_ref = os.environ["BASE_REF"]
head_ref = os.environ["HEAD_REF"]
output_file = Path(os.environ["OUTPUT_FILE"])
json_output_file_raw = os.environ.get("JSON_OUTPUT_FILE", "").strip()


def parse_meta(path: Path):
    data = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if "=" not in raw:
            continue
        k, v = raw.split("=", 1)
        data[k.strip()] = v.strip()
    return data


def load_discrepancy(path: Path):
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def snapshot_row(meta, discrepancy):
    status = meta.get("status", "unavailable")
    note = meta.get("note", "")
    if discrepancy is None:
        return {
            "analysis_status": status if status else "unavailable",
            "note": note,
            "drift_count": None,
            "drift_by_severity": {
                "missing_fragment": None,
                "missing_codeowner": None,
                "scope_mismatch": None,
            },
            "schema_version": None,
        }

    drift_by = discrepancy.get("drift_by_severity") or {}
    return {
        "analysis_status": status if status else discrepancy.get("status", "unknown"),
        "note": note,
        "drift_count": discrepancy.get("drift_count"),
        "drift_by_severity": {
            "missing_fragment": drift_by.get("missing_fragment", 0),
            "missing_codeowner": drift_by.get("missing_codeowner", 0),
            "scope_mismatch": drift_by.get("scope_mismatch", 0),
        },
        "schema_version": discrepancy.get("schema_version"),
    }


base_meta = parse_meta(base_meta_file)
head_meta = parse_meta(head_meta_file)
base_discrepancy = load_discrepancy(base_json_file)
head_discrepancy = load_discrepancy(head_json_file)

base_row = snapshot_row(base_meta, base_discrepancy)
head_row = snapshot_row(head_meta, head_discrepancy)

delta = None
if isinstance(base_row["drift_count"], int) and isinstance(head_row["drift_count"], int):
    delta = head_row["drift_count"] - base_row["drift_count"]

payload = {
    "schema_version": "openapi.ownership_drift_trend.v1",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "diff_range": {
        "base_ref": base_ref,
        "head_ref": head_ref,
    },
    "base": {
        "ref": base_ref,
        **base_row,
    },
    "head": {
        "ref": head_ref,
        **head_row,
    },
    "delta": {
        "drift_count": delta,
    },
    "head_discrepancy_sample": (head_discrepancy or {}).get("discrepancies", [])[:15],
}

lines = []
lines.append("# OpenAPI Ownership Drift Trend Snapshot")
lines.append("")
lines.append(f"- Generated at: {payload['generated_at']}")
lines.append(f"- Diff range: `{base_ref}..{head_ref}`")
lines.append("")
lines.append("| snapshot | analysis status | drift count | missing_fragment | missing_codeowner | scope_mismatch | note |")
lines.append("| --- | --- | ---: | ---: | ---: | ---: | --- |")

def fmt_row(name: str, row: dict):
    drift = row["drift_count"]
    sev = row["drift_by_severity"]
    drift_txt = str(drift) if isinstance(drift, int) else "N/A"
    mf = str(sev["missing_fragment"]) if isinstance(sev["missing_fragment"], int) else "N/A"
    mc = str(sev["missing_codeowner"]) if isinstance(sev["missing_codeowner"], int) else "N/A"
    sm = str(sev["scope_mismatch"]) if isinstance(sev["scope_mismatch"], int) else "N/A"
    note = row.get("note") or ""
    return f"| {name} | {row['analysis_status']} | {drift_txt} | {mf} | {mc} | {sm} | {note} |"

lines.append(fmt_row(f"base (`{base_ref}`)", base_row))
lines.append(fmt_row(f"head (`{head_ref}`)", head_row))
lines.append("")

lines.append("## Trend")
lines.append("")
if isinstance(delta, int):
    trend = "unchanged"
    if delta > 0:
        trend = "increased"
    elif delta < 0:
        trend = "decreased"
    lines.append(f"- Ownership drift count {trend} by `{delta}` (head - base).")
else:
    lines.append("- Ownership drift delta unavailable (base or head snapshot missing).")
lines.append("")

lines.append("## Head Discrepancy Sample")
lines.append("")
sample = payload["head_discrepancy_sample"]
if len(sample) == 0:
    lines.append("- No ownership discrepancies in head snapshot.")
else:
    for item in sample:
        severity = item.get("severity", "unknown")
        message = item.get("message", "").strip()
        lines.append(f"- [{severity}] {message}")

output_file.write_text("\n".join(lines) + "\n", encoding="utf-8")

if json_output_file_raw:
    json_output_file = Path(json_output_file_raw)
    json_output_file.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY

echo "[OK] Generated OpenAPI ownership drift trend snapshot: $OUTPUT_FILE"
if [[ -n "$JSON_OUTPUT_FILE" ]]; then
  echo "[OK] Generated OpenAPI ownership drift trend JSON: $JSON_OUTPUT_FILE"
fi
