#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

move_runtime_file() {
  local source="$1"
  local target="$2"

  if [[ ! -f "$source" ]]; then
    return
  fi
  if [[ -e "$target" ]]; then
    echo "[skip] target exists: $target"
    return
  fi
  if lsof "$source" >/dev/null 2>&1; then
    echo "[skip] file is in use: $source"
    return
  fi

  mkdir -p "$(dirname "$target")"
  mv "$source" "$target"
  echo "[moved] $source -> $target"

  for suffix in -wal -shm; do
    if [[ -f "${source}${suffix}" && ! -e "${target}${suffix}" ]]; then
      mv "${source}${suffix}" "${target}${suffix}"
      echo "[moved] ${source}${suffix} -> ${target}${suffix}"
    fi
  done
}

move_runtime_file \
  "$ROOT_DIR/chat_app_server_rs/data/chat_app.db" \
  "$ROOT_DIR/.local/chat_app_server/data/chat_app.db"

move_runtime_file \
  "$ROOT_DIR/openai-codex-gateway/gateway_state.sqlite3" \
  "$ROOT_DIR/.local/openai-codex-gateway/gateway_state.sqlite3"

move_runtime_file \
  "$ROOT_DIR/memory_server/backend/data/memory_server.db" \
  "$ROOT_DIR/.local/memory_server/data/memory_server.db"
