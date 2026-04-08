#!/usr/bin/env bash

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
    "$ROOT_DIR/agent_workspace" \
    "$ROOT_DIR/agent_orchestrator" \
    "$ROOT_DIR/memory_server" \
    "$ROOT_DIR/openai-codex-gateway" \
    "$ROOT_DIR/target-shared" \
    "$ROOT_DIR/docs" 2>/dev/null | sort -hr
)

print_header "Hotspot Subdirectories"
while IFS= read -r -d '' path; do
  du -sh "$path" 2>/dev/null || true
done < <(
  list_existing_paths \
    "agent_workspace/node_modules" \
    "agent_workspace/dist" \
    "agent_orchestrator/target" \
    "agent_orchestrator/data" \
    "agent_orchestrator/logs" \
    "agent_orchestrator/docs" \
    "memory_server/backend/target" \
    "memory_server/backend/data" \
    "memory_server/frontend/node_modules" \
    "memory_server/frontend/dist" \
    "target-shared"
) | sort -hr

print_header "Large Files Over 20MB"
find "$ROOT_DIR" \
  \( -path "$ROOT_DIR/.git" -o -path "$ROOT_DIR/agent_workspace/node_modules" -o -path "$ROOT_DIR/agent_orchestrator/target" -o -path "$ROOT_DIR/memory_server/backend/target" -o -path "$ROOT_DIR/memory_server/frontend/node_modules" -o -path "$ROOT_DIR/target-shared" \) -prune \
  -o \( -type f -size +20M -print \) | sed "s#^$ROOT_DIR/##" | sort | head -n "$TOP_N"

print_header "Tracked Runtime Artifacts"
git -C "$ROOT_DIR" ls-files | rg '(^|/)(__pycache__/|.*\.pyc$|.*\.pyo$|.*\.sqlite3$|.*\.sqlite3-shm$|.*\.sqlite3-wal$|.*\.db$|.*\.db-shm$|.*\.db-wal$|.*\.DS_Store$)' || true

print_header "Local Runtime And Cache Artifacts"
find "$ROOT_DIR" \
  \( -path "$ROOT_DIR/.git" -o -path "$ROOT_DIR/agent_workspace/node_modules" -o -path "$ROOT_DIR/agent_orchestrator/target" -o -path "$ROOT_DIR/memory_server/backend/target" -o -path "$ROOT_DIR/memory_server/frontend/node_modules" -o -path "$ROOT_DIR/target-shared" \) -prune \
  -o \( -name '.DS_Store' -o -name '__pycache__' -o -name '*.pyc' -o -name '*.pyo' -o -name '*.sqlite3' -o -name '*.sqlite3-shm' -o -name '*.sqlite3-wal' -o -name '*.db' -o -name '*.db-shm' -o -name '*.db-wal' \) -print \
  | sed "s#^$ROOT_DIR/##" | sort | head -n "$TOP_N"

print_header "Suggested Commands"
echo "bash scripts/check-large-files.sh --threshold 20"
echo "bash scripts/cleanup-dev-artifacts.sh --dry-run"
