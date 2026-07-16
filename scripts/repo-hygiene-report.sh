#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOP_N="${TOP_N:-20}"

print_header() {
  echo
  echo "== $1 =="
}

list_existing_paths() {
  for path in "$@"; do
    if [[ -e "$ROOT_DIR/$path" ]]; then
      printf '%s\0' "$ROOT_DIR/$path"
    fi
  done
}

print_header "Repository Root"
echo "$ROOT_DIR"

print_header "Top-Level Sizes"
while IFS= read -r line; do
  [[ -n "$line" ]] && echo "$line"
done < <(
  du -sh \
    "$ROOT_DIR/chatos" \
    "$ROOT_DIR/target-shared" \
    "$ROOT_DIR/docs" 2>/dev/null | sort -hr
)

print_header "Hotspot Subdirectories"
while IFS= read -r -d '' path; do
  du -sh "$path" 2>/dev/null || true
done < <(
  list_existing_paths \
    "chatos/frontend/node_modules" \
    "chatos/frontend/dist" \
    "chatos/backend/target" \
    "chatos/backend/data" \
    "chatos/backend/logs" \
    "chatos/backend/docs" \
    "target-shared"
) | sort -hr

print_header "Large Files Over 20MB"
bash "$ROOT_DIR/scripts/check-large-files.sh" --threshold 20 | tail -n +2 | head -n "$TOP_N"

print_header "Tracked Runtime Artifacts"
git -C "$ROOT_DIR" ls-files | rg '(^|/)(__pycache__/|.*\.pyc$|.*\.pyo$|.*\.sqlite3$|.*\.sqlite3-shm$|.*\.sqlite3-wal$|.*\.db$|.*\.db-shm$|.*\.db-wal$|.*\.DS_Store$)' || true

print_header "Local Runtime And Cache Artifacts"
find "$ROOT_DIR" \
  \( -path "$ROOT_DIR/.git" -o -path "$ROOT_DIR/chatos/frontend/node_modules" -o -path "$ROOT_DIR/chatos/backend/target" -o -path "$ROOT_DIR/target-shared" \) -prune \
  -o \( -name '.DS_Store' -o -name '__pycache__' -o -name '*.pyc' -o -name '*.pyo' -o -name '*.sqlite3' -o -name '*.sqlite3-shm' -o -name '*.sqlite3-wal' -o -name '*.db' -o -name '*.db-shm' -o -name '*.db-wal' \) -print \
  | sed "s#^$ROOT_DIR/##" | sort | head -n "$TOP_N"

print_header "Suggested Commands"
echo "bash scripts/check-large-files.sh --threshold 20"
echo "python scripts/check_source_size_policy.py"
echo "python scripts/check_new_code_clones.py --min-lines 25"
echo "python scripts/check-non-test-unwrap-expect.py"
echo "python scripts/check-rust-dependency-drift.py"
echo "bash scripts/check-hotspot-line-budgets.sh"
echo "bash scripts/cleanup-dev-artifacts.sh --dry-run"
