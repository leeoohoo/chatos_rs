#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MARKDOWN_MODE="false"
TSV_MODE="false"
JSON_MODE="false"
STRICT_MODE="false"
BASE_REF=""
HEAD_REF=""
CODEOWNERS_FILE="${OPENAPI_CODEOWNERS_FILE:-$ROOT_DIR/.github/CODEOWNERS.openapi}"
OWNERSHIP_MANIFEST_FILE="${OPENAPI_OWNERSHIP_MANIFEST_FILE:-$ROOT_DIR/.github/api-contract/ownership/manifest.yaml}"

for arg in "$@"; do
  case "$arg" in
    --markdown)
      MARKDOWN_MODE="true"
      ;;
    --tsv)
      TSV_MODE="true"
      ;;
    --json)
      JSON_MODE="true"
      ;;
    --strict)
      STRICT_MODE="true"
      ;;
    *)
      if [[ -z "$BASE_REF" ]]; then
        BASE_REF="$arg"
      elif [[ -z "$HEAD_REF" ]]; then
        HEAD_REF="$arg"
      fi
      ;;
  esac
done

output_mode_count=0
if [[ "$MARKDOWN_MODE" == "true" ]]; then
  output_mode_count=$((output_mode_count + 1))
fi
if [[ "$TSV_MODE" == "true" ]]; then
  output_mode_count=$((output_mode_count + 1))
fi
if [[ "$JSON_MODE" == "true" ]]; then
  output_mode_count=$((output_mode_count + 1))
fi

if [[ "$output_mode_count" -gt 1 ]]; then
  echo "[ERROR] Choose only one output mode: --markdown, --tsv, or --json." >&2
  exit 2
fi

BASE_REF="${BASE_REF:-${OPENAPI_DIFF_BASE:-}}"
HEAD_REF="${HEAD_REF:-${OPENAPI_DIFF_HEAD:-HEAD}}"

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

if [[ ! -f "$OWNERSHIP_MANIFEST_FILE" ]]; then
  echo "[ERROR] Missing ownership manifest: $OWNERSHIP_MANIFEST_FILE" >&2
  exit 1
fi

build_report_rows_from_manifest() {
  local changed_files_tmp
  changed_files_tmp="$(mktemp)"
  printf '%s\n' "$CHANGED_FILES_TEXT" | awk 'NF > 0 {print}' > "$changed_files_tmp"

  OWNERSHIP_MANIFEST_FILE="$OWNERSHIP_MANIFEST_FILE" \
    CODEOWNERS_FILE="$CODEOWNERS_FILE" \
    python3 - "$changed_files_tmp" <<'PY'
from pathlib import Path
import os
import sys
import yaml

changed_files_file = Path(sys.argv[1])
manifest_file = Path(os.environ["OWNERSHIP_MANIFEST_FILE"])
codeowners_file = Path(os.environ["CODEOWNERS_FILE"])

manifest_data = yaml.safe_load(manifest_file.read_text(encoding="utf-8")) or {}
defaults = manifest_data.get("defaults") or {}
default_owner_hint = (defaults.get("owner_hint") or "Repository/platform maintainers").strip()
default_owner_footprint = (defaults.get("owner_footprint") or "fallback").strip()

fragment_map = {}
for item in manifest_data.get("fragments") or []:
    if not isinstance(item, dict):
        continue
    file_path = str(item.get("file") or "").strip()
    if not file_path:
        continue
    owner_hint = str(item.get("owner_hint") or default_owner_hint).strip() or default_owner_hint
    owner_footprint = str(item.get("owner_footprint") or "mapped").strip() or "mapped"
    fragment_map[file_path] = {
        "owner_hint": owner_hint,
        "owner_footprint": owner_footprint,
    }

codeowners_entries = {}
codeowners_missing_file = not codeowners_file.exists()
if not codeowners_missing_file:
    for raw in codeowners_file.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        codeowners_entries[parts[0]] = parts[1:]

changed_files = sorted(
    set(
        line.strip()
        for line in changed_files_file.read_text(encoding="utf-8").splitlines()
        if line.strip()
    )
)

for file_path in changed_files:
    row = fragment_map.get(
        file_path,
        {
            "owner_hint": default_owner_hint,
            "owner_footprint": default_owner_footprint,
        },
    )
    if codeowners_missing_file:
        codeowners_footprint = "missing_file"
    else:
        owners = codeowners_entries.get(file_path) or []
        codeowners_footprint = "mapped" if len(owners) > 0 else "missing_entry"
    print(
        "\t".join(
            [
                file_path,
                row["owner_hint"],
                row["owner_footprint"],
                codeowners_footprint,
            ]
        )
    )
PY

  rm -f "$changed_files_tmp"
}

CHANGED_FILES_TEXT="$(
  git diff --name-only "$BASE_REF" "$HEAD_REF" -- .github/api-contract/fragments \
    | awk '/\.ya?ml$/ {print}' \
    | sort -u
)"

HAS_CHANGED_FILES="false"
if [[ -n "$CHANGED_FILES_TEXT" ]]; then
  HAS_CHANGED_FILES="true"
fi

REPORT_ROWS_TEXT=""
if [[ "$HAS_CHANGED_FILES" == "true" ]]; then
  REPORT_ROWS_TEXT="$(build_report_rows_from_manifest)"
fi

STRICT_ERROR_TEXT=""
if [[ "$STRICT_MODE" == "true" ]]; then
  while IFS= read -r row; do
    [[ -z "$row" ]] && continue
    IFS=$'\t' read -r file owners owner_status codeowners_status <<<"$row"
    if [[ "$owner_status" != "mapped" ]]; then
      STRICT_ERROR_TEXT+="$file -> owner-footprint is $owner_status"$'\n'
    fi
    if [[ "$codeowners_status" != "mapped" ]]; then
      STRICT_ERROR_TEXT+="$file -> codeowners-footprint is $codeowners_status"$'\n'
    fi
  done <<< "$REPORT_ROWS_TEXT"
fi

emit_json_report() {
  local rows_file
  local strict_errors_file
  rows_file="$(mktemp)"
  strict_errors_file="$(mktemp)"
  printf '%s' "$REPORT_ROWS_TEXT" > "$rows_file"
  printf '%s' "$STRICT_ERROR_TEXT" > "$strict_errors_file"

  BASE_REF="$BASE_REF" \
    HEAD_REF="$HEAD_REF" \
    STRICT_MODE="$STRICT_MODE" \
    CODEOWNERS_FILE="$CODEOWNERS_FILE" \
    OWNERSHIP_MANIFEST_FILE="$OWNERSHIP_MANIFEST_FILE" \
    python3 - "$rows_file" "$strict_errors_file" <<'PY'
import json
import os
import sys

rows_file = sys.argv[1]
errors_file = sys.argv[2]

rows = []
with open(rows_file, "r", encoding="utf-8") as fh:
    for raw in fh:
        raw = raw.rstrip("\n")
        if not raw:
            continue
        parts = raw.split("\t")
        if len(parts) < 4:
            continue
        file_path, owners, owner_status, codeowners_status = parts[:4]
        rows.append(
            {
                "file": file_path,
                "owner_hint": owners,
                "owner_footprint": owner_status,
                "codeowners_footprint": codeowners_status,
            }
        )

errors = []
with open(errors_file, "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.strip()
        if line:
            errors.append(line)

strict_mode = os.environ.get("STRICT_MODE", "false") == "true"
if strict_mode:
    policy_status = "failed" if errors else "passed"
else:
    policy_status = "not_checked"

owner_mapped = sum(1 for row in rows if row["owner_footprint"] == "mapped")
owner_fallback = sum(1 for row in rows if row["owner_footprint"] == "fallback")

codeowners_summary = {}
for row in rows:
    key = row["codeowners_footprint"]
    codeowners_summary[key] = codeowners_summary.get(key, 0) + 1

payload = {
    "schema_version": "openapi.owner_report.v1",
    "diff_range": {
        "base_ref": os.environ.get("BASE_REF", ""),
        "head_ref": os.environ.get("HEAD_REF", ""),
    },
    "codeowners_file": os.environ.get("CODEOWNERS_FILE", ""),
    "ownership_manifest_file": os.environ.get("OWNERSHIP_MANIFEST_FILE", ""),
    "changed_fragment_count": len(rows),
    "fragments": rows,
    "footprint_summary": {
        "owner_mapped": owner_mapped,
        "owner_fallback": owner_fallback,
        "codeowners": codeowners_summary,
    },
    "policy": {
        "strict_mode": strict_mode,
        "status": policy_status,
        "error_count": len(errors),
        "errors": errors,
    },
}

print(json.dumps(payload, indent=2, ensure_ascii=False))
PY

  rm -f "$rows_file" "$strict_errors_file"
}

if [[ "$MARKDOWN_MODE" == "true" ]]; then
  echo "Diff Range: \`$BASE_REF..$HEAD_REF\`"
  if [[ "$HAS_CHANGED_FILES" != "true" ]]; then
    echo "- No OpenAPI fragment changes detected."
  else
    while IFS= read -r row; do
      [[ -z "$row" ]] && continue
      IFS=$'\t' read -r file owners owner_status codeowners_status <<<"$row"
      echo "- \`$file\`"
      echo "  - owners: $owners"
      echo "  - owner-footprint: $owner_status"
      echo "  - codeowners-footprint: $codeowners_status"
    done <<< "$REPORT_ROWS_TEXT"
  fi
  if [[ "$STRICT_MODE" == "true" ]]; then
    echo "- strict-mode: enabled"
  fi
fi

if [[ "$TSV_MODE" == "true" ]]; then
  if [[ -n "$REPORT_ROWS_TEXT" ]]; then
    printf '%s\n' "$REPORT_ROWS_TEXT"
  fi
fi

if [[ "$JSON_MODE" == "true" ]]; then
  emit_json_report
fi

if [[ "$MARKDOWN_MODE" != "true" && "$TSV_MODE" != "true" && "$JSON_MODE" != "true" ]]; then
  echo "[INFO] OpenAPI fragment owner report ($BASE_REF..$HEAD_REF)"
  if [[ "$HAS_CHANGED_FILES" != "true" ]]; then
    echo "[INFO] No OpenAPI fragment changes detected."
  else
    while IFS= read -r row; do
      [[ -z "$row" ]] && continue
      IFS=$'\t' read -r file owners owner_status codeowners_status <<<"$row"
      echo "- $file"
      echo "  owners: $owners"
      echo "  owner-footprint: $owner_status"
      echo "  codeowners-footprint: $codeowners_status"
    done <<< "$REPORT_ROWS_TEXT"
  fi
fi

if [[ "$STRICT_MODE" == "true" ]]; then
  if [[ -n "$STRICT_ERROR_TEXT" ]]; then
    echo "[ERROR] OpenAPI fragment owner policy check failed." >&2
    while IFS= read -r line; do
      [[ -z "$line" ]] && continue
      echo "  - $line" >&2
    done <<< "$STRICT_ERROR_TEXT"
    echo "[INFO] Update owner mapping in:" >&2
    echo "       scripts/report_openapi_fragment_owners.sh" >&2
    echo "[INFO] Update CODEOWNERS mapping in:" >&2
    echo "       $CODEOWNERS_FILE" >&2
    exit 1
  fi
  if [[ "$MARKDOWN_MODE" != "true" && "$TSV_MODE" != "true" && "$JSON_MODE" != "true" ]]; then
    echo "[OK] OpenAPI fragment owner policy check passed."
  fi
fi
