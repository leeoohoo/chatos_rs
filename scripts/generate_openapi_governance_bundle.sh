#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BASE_REF="${1:-${OPENAPI_DIFF_BASE:-}}"
HEAD_REF="${2:-${OPENAPI_DIFF_HEAD:-HEAD}}"
BUNDLE_DIR="${3:-${OPENAPI_GOVERNANCE_BUNDLE_DIR:-$ROOT_DIR/.github/api-contract/ownership/OPENAPI_GOVERNANCE_BUNDLE}}"
REUSE_EXISTING="${OPENAPI_GOVERNANCE_BUNDLE_REUSE_EXISTING:-true}"

SUMMARY_INPUT="${OPENAPI_CHANGE_SUMMARY_INPUT:-}"
OWNER_REPORT_INPUT="${OPENAPI_OWNER_REPORT_INPUT:-}"
PR_COMMENT_INPUT="${OPENAPI_PR_COMMENT_DRAFT_INPUT:-}"
DISCREPANCY_INPUT="${OPENAPI_OWNERSHIP_DISCREPANCY_INPUT:-}"
DRIFT_MD_INPUT="${OPENAPI_OWNERSHIP_DRIFT_TREND_INPUT:-}"
DRIFT_JSON_INPUT="${OPENAPI_OWNERSHIP_DRIFT_TREND_JSON_INPUT:-}"

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

mkdir -p "$BUNDLE_DIR"

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

WORK_SUMMARY_MD="$TMP_DIR/openapi-contract-change-summary.md"
WORK_OWNER_REPORT_JSON="$TMP_DIR/openapi-owner-report.json"
WORK_PR_COMMENT_MD="$TMP_DIR/openapi-pr-comment.md"
WORK_DISCREPANCY_JSON="$TMP_DIR/openapi-ownership-discrepancy.json"
WORK_DRIFT_MD="$TMP_DIR/openapi-ownership-drift-trend.md"
WORK_DRIFT_JSON="$TMP_DIR/openapi-ownership-drift-trend.json"

is_truthy() {
  local raw="$1"
  case "$raw" in
    1|true|TRUE|yes|YES|on|ON)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

has_file() {
  local p="$1"
  [[ -n "$p" && -f "$p" ]]
}

copy_reused_artifact() {
  local input_file="$1"
  local output_file="$2"
  local label="$3"

  if ! is_truthy "$REUSE_EXISTING"; then
    return 1
  fi

  if has_file "$input_file"; then
    cp "$input_file" "$output_file"
    echo "[INFO] Reused $label: $input_file"
    return 0
  fi

  if [[ -n "$input_file" ]]; then
    echo "[WARN] Reuse requested but missing $label: $input_file (will regenerate)." >&2
  fi
  return 1
}

write_summary_placeholder() {
  local output_file="$1"
  cat > "$output_file" <<EOF
# OpenAPI Contract Change Summary

- Generated at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
- Diff range: \`$BASE_REF..$HEAD_REF\`
- Status: unavailable (fallback placeholder)

Summary generation failed in governance-bundle fallback mode.
EOF
}

write_pr_comment_placeholder() {
  local output_file="$1"
  cat > "$output_file" <<EOF
<!-- openapi-owner-checklist:start version=v1 -->

### OpenAPI Contract Owner Checklist

- Diff range: \`$BASE_REF..$HEAD_REF\`
- Status: unavailable (fallback placeholder)

Unable to generate owner checklist details in this run.

<!-- openapi-owner-checklist:end -->
EOF
}

write_owner_report_placeholder() {
  local output_file="$1"
  BASE_REF="$BASE_REF" HEAD_REF="$HEAD_REF" ROOT_DIR="$ROOT_DIR" OUTPUT_FILE="$output_file" \
    python3 - <<'PY'
from datetime import datetime, timezone
from pathlib import Path
import json
import os

payload = {
    "schema_version": "openapi.owner_report.v1",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "diff_range": {
        "base_ref": os.environ["BASE_REF"],
        "head_ref": os.environ["HEAD_REF"],
    },
    "codeowners_file": str(Path(os.environ["ROOT_DIR"]) / ".github/CODEOWNERS.openapi"),
    "ownership_manifest_file": str(Path(os.environ["ROOT_DIR"]) / ".github/api-contract/ownership/manifest.yaml"),
    "changed_fragment_count": 0,
    "fragments": [],
    "footprint_summary": {
        "owner_mapped": 0,
        "owner_fallback": 0,
        "codeowners": {},
    },
    "policy": {
        "strict_mode": False,
        "status": "not_checked",
        "error_count": 1,
        "errors": ["owner_report_generation_failed"],
    },
}

Path(os.environ["OUTPUT_FILE"]).write_text(
    json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY
}

write_drift_markdown_placeholder() {
  local output_file="$1"
  cat > "$output_file" <<EOF
# OpenAPI Ownership Drift Trend Snapshot

- Generated at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
- Diff range: \`$BASE_REF..$HEAD_REF\`
- Status: unavailable (fallback placeholder)

Ownership drift trend generation failed in governance-bundle fallback mode.
EOF
}

write_drift_json_placeholder() {
  local output_file="$1"
  BASE_REF="$BASE_REF" HEAD_REF="$HEAD_REF" OUTPUT_FILE="$output_file" \
    python3 - <<'PY'
from datetime import datetime, timezone
from pathlib import Path
import json
import os

base_ref = os.environ["BASE_REF"]
head_ref = os.environ["HEAD_REF"]

payload = {
    "schema_version": "openapi.ownership_drift_trend.v1",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "diff_range": {
        "base_ref": base_ref,
        "head_ref": head_ref,
    },
    "base": {
        "ref": base_ref,
        "analysis_status": "unavailable",
        "note": "generation_failed",
        "drift_count": None,
        "drift_by_severity": {
            "missing_fragment": None,
            "missing_codeowner": None,
            "scope_mismatch": None,
        },
        "schema_version": None,
    },
    "head": {
        "ref": head_ref,
        "analysis_status": "unavailable",
        "note": "generation_failed",
        "drift_count": None,
        "drift_by_severity": {
            "missing_fragment": None,
            "missing_codeowner": None,
            "scope_mismatch": None,
        },
        "schema_version": None,
    },
    "delta": {
        "drift_count": None,
    },
    "head_discrepancy_sample": [],
}

Path(os.environ["OUTPUT_FILE"]).write_text(
    json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY
}

need_summary_regen="false"
summary_reused="false"
owner_report_reused="false"
pr_comment_reused="false"

if copy_reused_artifact "$SUMMARY_INPUT" "$BUNDLE_DIR/contract-change-summary.md" "contract change summary"; then
  summary_reused="true"
else
  need_summary_regen="true"
fi
if copy_reused_artifact "$OWNER_REPORT_INPUT" "$BUNDLE_DIR/owner-report.json" "owner report JSON"; then
  owner_report_reused="true"
else
  need_summary_regen="true"
fi
if copy_reused_artifact "$PR_COMMENT_INPUT" "$BUNDLE_DIR/pr-comment-draft.md" "PR comment draft"; then
  pr_comment_reused="true"
else
  need_summary_regen="true"
fi

if [[ "$need_summary_regen" == "true" ]]; then
  if bash "$ROOT_DIR/scripts/generate_openapi_contract_change_summary.sh" \
    "$BASE_REF" \
    "$HEAD_REF" \
    "$WORK_SUMMARY_MD" \
    "$WORK_OWNER_REPORT_JSON" \
    "$WORK_PR_COMMENT_MD"; then
    [[ "$summary_reused" == "true" ]] || cp "$WORK_SUMMARY_MD" "$BUNDLE_DIR/contract-change-summary.md"
    [[ "$owner_report_reused" == "true" ]] || cp "$WORK_OWNER_REPORT_JSON" "$BUNDLE_DIR/owner-report.json"
    [[ "$pr_comment_reused" == "true" ]] || cp "$WORK_PR_COMMENT_MD" "$BUNDLE_DIR/pr-comment-draft.md"
  else
    echo "[WARN] Failed to regenerate summary artifacts; writing fallback placeholders." >&2
    [[ "$summary_reused" == "true" ]] || write_summary_placeholder "$BUNDLE_DIR/contract-change-summary.md"
    [[ "$owner_report_reused" == "true" ]] || write_owner_report_placeholder "$BUNDLE_DIR/owner-report.json"
    [[ "$pr_comment_reused" == "true" ]] || write_pr_comment_placeholder "$BUNDLE_DIR/pr-comment-draft.md"
  fi
fi

DISCREPANCY_CHECK_STATUS="reused"
if ! copy_reused_artifact "$DISCREPANCY_INPUT" "$BUNDLE_DIR/ownership-discrepancy.json" "ownership discrepancy JSON"; then
  DISCREPANCY_CHECK_STATUS="passed"
  if ! OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT="$WORK_DISCREPANCY_JSON" \
    bash "$ROOT_DIR/scripts/check_openapi_ownership_map_consistency.sh"; then
    DISCREPANCY_CHECK_STATUS="failed"
  fi

  if [[ -f "$WORK_DISCREPANCY_JSON" ]]; then
    cp "$WORK_DISCREPANCY_JSON" "$BUNDLE_DIR/ownership-discrepancy.json"
  fi
fi

if [[ ! -f "$BUNDLE_DIR/ownership-discrepancy.json" ]]; then
  python3 - "$BUNDLE_DIR/ownership-discrepancy.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
payload = {
    "schema_version": "openapi.ownership_discrepancy.v1",
    "status": "unavailable",
    "files": {},
    "drift_count": None,
    "drift_by_severity": {
        "missing_fragment": None,
        "missing_codeowner": None,
        "scope_mismatch": None,
    },
    "discrepancies": [],
}
path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
fi

need_drift_regen="false"
drift_md_reused="false"
drift_json_reused="false"
if copy_reused_artifact "$DRIFT_MD_INPUT" "$BUNDLE_DIR/ownership-drift-trend.md" "ownership drift trend markdown"; then
  drift_md_reused="true"
else
  need_drift_regen="true"
fi
if copy_reused_artifact "$DRIFT_JSON_INPUT" "$BUNDLE_DIR/ownership-drift-trend.json" "ownership drift trend JSON"; then
  drift_json_reused="true"
else
  need_drift_regen="true"
fi

if [[ "$need_drift_regen" == "true" ]]; then
  if bash "$ROOT_DIR/scripts/generate_openapi_ownership_drift_trend.sh" \
    "$BASE_REF" \
    "$HEAD_REF" \
    "$WORK_DRIFT_MD" \
    "$WORK_DRIFT_JSON"; then
    [[ "$drift_md_reused" == "true" ]] || cp "$WORK_DRIFT_MD" "$BUNDLE_DIR/ownership-drift-trend.md"
    [[ "$drift_json_reused" == "true" ]] || cp "$WORK_DRIFT_JSON" "$BUNDLE_DIR/ownership-drift-trend.json"
  else
    echo "[WARN] Failed to regenerate drift trend artifacts; writing fallback placeholders." >&2
    [[ "$drift_md_reused" == "true" ]] || write_drift_markdown_placeholder "$BUNDLE_DIR/ownership-drift-trend.md"
    [[ "$drift_json_reused" == "true" ]] || write_drift_json_placeholder "$BUNDLE_DIR/ownership-drift-trend.json"
  fi
fi

BASE_REF="$BASE_REF" \
  HEAD_REF="$HEAD_REF" \
  DISCREPANCY_CHECK_STATUS="$DISCREPANCY_CHECK_STATUS" \
  BUNDLE_DIR="$BUNDLE_DIR" \
  python3 - <<'PY'
from datetime import datetime, timezone
from pathlib import Path
import json
import os

bundle_dir = Path(os.environ["BUNDLE_DIR"])
base_ref = os.environ["BASE_REF"]
head_ref = os.environ["HEAD_REF"]
discrepancy_check_status = os.environ.get("DISCREPANCY_CHECK_STATUS", "unknown")


def load_json(path: Path):
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def artifact_entry(file_name: str):
    path = bundle_dir / file_name
    return {
        "file": file_name,
        "exists": path.exists(),
        "bytes": path.stat().st_size if path.exists() else None,
    }


owner_report = load_json(bundle_dir / "owner-report.json")
discrepancy = load_json(bundle_dir / "ownership-discrepancy.json")
drift_trend = load_json(bundle_dir / "ownership-drift-trend.json")

owner_policy_status = None
if isinstance(owner_report, dict):
    owner_policy_status = ((owner_report.get("policy") or {}).get("status"))

discrepancy_status = discrepancy.get("status") if isinstance(discrepancy, dict) else None
drift_head_status = None
drift_delta = None
if isinstance(drift_trend, dict):
    drift_head_status = ((drift_trend.get("head") or {}).get("analysis_status"))
    drift_delta = ((drift_trend.get("delta") or {}).get("drift_count"))

artifacts = {
    "contract_change_summary": {
        **artifact_entry("contract-change-summary.md"),
        "media_type": "text/markdown",
    },
    "owner_report": {
        **artifact_entry("owner-report.json"),
        "media_type": "application/json",
        "schema_version": (owner_report or {}).get("schema_version"),
    },
    "ownership_discrepancy": {
        **artifact_entry("ownership-discrepancy.json"),
        "media_type": "application/json",
        "schema_version": (discrepancy or {}).get("schema_version"),
    },
    "ownership_drift_trend_markdown": {
        **artifact_entry("ownership-drift-trend.md"),
        "media_type": "text/markdown",
    },
    "ownership_drift_trend": {
        **artifact_entry("ownership-drift-trend.json"),
        "media_type": "application/json",
        "schema_version": (drift_trend or {}).get("schema_version"),
    },
    "pr_comment_draft": {
        **artifact_entry("pr-comment-draft.md"),
        "media_type": "text/markdown",
    },
}

index_payload = {
    "schema_version": "openapi.governance_bundle_index.v1",
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "diff_range": {
        "base_ref": base_ref,
        "head_ref": head_ref,
    },
    "aggregate": {
        "owner_policy_status": owner_policy_status,
        "discrepancy_status": discrepancy_status,
        "discrepancy_check_status": discrepancy_check_status,
        "drift_head_status": drift_head_status,
        "drift_delta": drift_delta,
    },
    "artifacts": artifacts,
}

(bundle_dir / "artifact-index.json").write_text(
    json.dumps(index_payload, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)

summary_lines = []
summary_lines.append("# OpenAPI Governance Bundle")
summary_lines.append("")
summary_lines.append(f"- Generated at: {index_payload['generated_at']}")
summary_lines.append(f"- Diff range: `{base_ref}..{head_ref}`")
summary_lines.append("")
summary_lines.append("## Aggregate Status")
summary_lines.append("")
summary_lines.append(f"- owner policy status: `{owner_policy_status}`")
summary_lines.append(f"- discrepancy status: `{discrepancy_status}` (check: `{discrepancy_check_status}`)")
summary_lines.append(f"- drift head status: `{drift_head_status}`")
summary_lines.append(f"- drift delta: `{drift_delta}`")
summary_lines.append("")
summary_lines.append("## Artifact Index")
summary_lines.append("")
summary_lines.append("| key | file | exists | schema_version |")
summary_lines.append("| --- | --- | --- | --- |")
for key, info in artifacts.items():
    exists = "yes" if info.get("exists") else "no"
    schema_version = info.get("schema_version") or "-"
    summary_lines.append(f"| {key} | `{info.get('file')}` | {exists} | {schema_version} |")
summary_lines.append("")
summary_lines.append("- Index JSON: `artifact-index.json`")

(bundle_dir / "GOVERNANCE_SUMMARY.md").write_text(
    "\n".join(summary_lines) + "\n",
    encoding="utf-8",
)
PY

echo "[OK] Generated OpenAPI governance bundle: $BUNDLE_DIR"
echo "[OK] Bundle entry summary: $BUNDLE_DIR/GOVERNANCE_SUMMARY.md"
echo "[OK] Bundle artifact index: $BUNDLE_DIR/artifact-index.json"
