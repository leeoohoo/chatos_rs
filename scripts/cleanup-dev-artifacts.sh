#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DRY_RUN=0

for arg in "$@"; do
  case "$arg" in
    --dry-run)
      DRY_RUN=1
      ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/cleanup-dev-artifacts.sh [--dry-run]

Cleanup common local development artifacts:
- chat_app/dist
- chat_app_server_rs/target
- target-shared
- memory_server/backend/target
- chat_app_server_rs logs and sqlite wal/shm files
- memory_server/backend sqlite wal/shm files
- memory_server/frontend/dist
- openai-codex-gateway sqlite wal/shm files
- python __pycache__ / .pyc
- common .DS_Store files
EOF
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

removed_count=0

remove_path() {
  local target="$1"
  if [[ ! -e "$target" ]]; then
    return
  fi
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] rm -rf $target"
    removed_count=$((removed_count + 1))
    return
  fi
  rm -rf "$target"
  echo "[removed] $target"
  removed_count=$((removed_count + 1))
}

remove_glob() {
  local pattern="$1"
  local found=0
  while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    found=1
    remove_path "$path"
  done < <(compgen -G "$pattern" || true)
  if [[ "$found" -eq 0 ]]; then
    return
  fi
}

remove_path "$ROOT_DIR/chat_app/dist"
remove_path "$ROOT_DIR/memory_server/frontend/dist"
remove_path "$ROOT_DIR/chat_app_server_rs/target"
remove_path "$ROOT_DIR/memory_server/backend/target"
remove_path "$ROOT_DIR/target-shared"
remove_glob "$ROOT_DIR/chat_app_server_rs/logs/server.log*"
remove_glob "$ROOT_DIR/chat_app_server_rs/logs/error.log*"
remove_path "$ROOT_DIR/chat_app_server_rs/data/chat_app.db-wal"
remove_path "$ROOT_DIR/chat_app_server_rs/data/chat_app.db-shm"
remove_path "$ROOT_DIR/memory_server/backend/data/memory_server.db-wal"
remove_path "$ROOT_DIR/memory_server/backend/data/memory_server.db-shm"
remove_path "$ROOT_DIR/openai-codex-gateway/gateway_state.sqlite3-wal"
remove_path "$ROOT_DIR/openai-codex-gateway/gateway_state.sqlite3-shm"

while IFS= read -r path; do
  remove_path "$path"
done < <(
  find "$ROOT_DIR" \
    \( -path "$ROOT_DIR/.git" -o -path "$ROOT_DIR/chat_app/node_modules" -o -path "$ROOT_DIR/chat_app_server_rs/target" -o -path "$ROOT_DIR/memory_server/backend/target" -o -path "$ROOT_DIR/memory_server/frontend/node_modules" -o -path "$ROOT_DIR/target-shared" \) -prune \
    -o \( -name .DS_Store -o -name __pycache__ -o -name '*.pyc' -o -name '*.pyo' \) -print
)

if [[ "$removed_count" -eq 0 ]]; then
  echo "No dev artifacts to clean."
else
  echo "Cleanup completed. Removed entries: $removed_count"
fi
