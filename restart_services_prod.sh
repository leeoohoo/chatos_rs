#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
BASE_SCRIPT="$ROOT_DIR/restart_services.sh"

if [[ ! -f "$BASE_SCRIPT" ]]; then
  echo "[ERROR] 未找到基础启动脚本: $BASE_SCRIPT"
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:prod" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:prod" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="prod"
fi

# 固定使用当前仓库可用的 Rust stable，避免后台登录 shell 吃到旧 nightly Cargo。
if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if command -v rustup >/dev/null 2>&1; then
  ACTIVE_TOOLCHAIN="$(cd "$ROOT_DIR" && rustup show active-toolchain 2>/dev/null | awk 'NR==1 {print $1}')"
  if [[ -n "$ACTIVE_TOOLCHAIN" ]]; then
    export RUSTUP_TOOLCHAIN="$ACTIVE_TOOLCHAIN"
  fi
fi

# 与开发环境隔离的生产端口。
export MAIN_BACKEND_PORT="${MAIN_BACKEND_PORT:-13997}"
export BACKEND_PORT="${BACKEND_PORT:-$MAIN_BACKEND_PORT}"
export LEGACY_MAIN_BACKEND_PORT="${LEGACY_MAIN_BACKEND_PORT:-13001}"
export FRONTEND_PORT="${FRONTEND_PORT:-18088}"

# 前端 dev 服务默认会直连 3997，这里显式改到生产端口，避免串到开发后端。
export VITE_API_BASE_URL="${VITE_API_BASE_URL:-http://127.0.0.1:${MAIN_BACKEND_PORT}/api}"

# 与开发脚本使用不同的 runtime 目录和停服策略，避免互相覆盖 PID / 误杀端口。
export RUNTIME_DIR="${RUNTIME_DIR:-/tmp/chatos_rs_prod_${ROOT_HASH}}"
export LEGACY_RUNTIME_DIR="${LEGACY_RUNTIME_DIR:-/tmp/chatos_rs_prod}"
export STOP_BY_PORT="${STOP_BY_PORT:-0}"

exec "$BASE_SCRIPT" "$@"
