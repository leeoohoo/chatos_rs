#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
BASE_SCRIPT="$ROOT_DIR/restart_task_runner_service.sh"

if [[ ! -f "$BASE_SCRIPT" ]]; then
  echo "[ERROR] 未找到基础启动脚本: $BASE_SCRIPT"
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:task-runner:prod" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:task-runner:prod" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="taskrunnerprod"
fi

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if command -v rustup >/dev/null 2>&1; then
  ACTIVE_TOOLCHAIN="$(cd "$ROOT_DIR" && rustup show active-toolchain 2>/dev/null | awk 'NR==1 {print $1}')"
  if [[ -n "$ACTIVE_TOOLCHAIN" ]]; then
    export RUSTUP_TOOLCHAIN="$ACTIVE_TOOLCHAIN"
  fi
fi

export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-49090}"
export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-$TASK_RUNNER_BACKEND_PORT}"
export TASK_RUNNER_FRONTEND_PORT="${TASK_RUNNER_FRONTEND_PORT:-49091}"
export TASK_RUNNER_VITE_API_BASE_URL="${TASK_RUNNER_VITE_API_BASE_URL:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
export TASK_RUNNER_STORE_MODE="${TASK_RUNNER_STORE_MODE:-mongo}"
export TASK_RUNNER_DATABASE_URL="${TASK_RUNNER_DATABASE_URL:-mongodb://admin:admin@127.0.0.1:27018/task_runner_service_prod?authSource=admin}"
export TASK_RUNNER_RUNTIME_DIR="${TASK_RUNNER_RUNTIME_DIR:-/tmp/chatos_rs_task_runner_prod_${ROOT_HASH}}"
export TASK_RUNNER_STOP_BY_PORT="${TASK_RUNNER_STOP_BY_PORT:-0}"

exec "$BASE_SCRIPT" "$@"
