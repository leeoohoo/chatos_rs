#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TARGETS=(
  "chatos/backend/src/api/fs"
  "chatos/backend/src/services/v2/ai_client/history_tools.rs"
  "chatos/backend/src/services/v2/ai_client/mod.rs"
  "chatos/backend/src/services/mcp_loader.rs"
  "chatos/backend/src/services/user_settings.rs"
  "chatos/backend/src/utils/model_config.rs"
)

echo "Checking request-path unwrap/expect usage..."

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

search_unwrap_expect() {
  if command -v rg >/dev/null 2>&1; then
    rg -n '\b(?:unwrap|expect)\s*\(' --color=never
  else
    grep -En '(^|[^[:alnum:]_])(unwrap|expect)[[:space:]]*\('
  fi
}

check_file() {
  local file_path="$1"
  awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { exit }
    { print }
  ' "$file_path"
}

for target in "${TARGETS[@]}"; do
  abs_target="$ROOT_DIR/$target"
  if [[ -d "$abs_target" ]]; then
    while IFS= read -r file; do
      check_file "$file" | search_unwrap_expect >>"$tmp_file" || true
    done < <(find "$abs_target" -type f -name '*.rs' ! -name 'tests.rs' | sort)
  elif [[ -f "$abs_target" ]]; then
    check_file "$abs_target" | search_unwrap_expect >>"$tmp_file" || true
  fi
done

if [[ -s "$tmp_file" ]]; then
  cat "$tmp_file"
  echo
  echo "Found unwrap/expect in guarded request-path files."
  exit 1
fi

echo "No unwrap/expect found in guarded request-path files."
