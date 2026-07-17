#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLIENT_DIR="$ROOT_DIR/local_connector_client"
CORE_DIR="$CLIENT_DIR/core"
FRONTEND_DIR="$CLIENT_DIR/frontend"
LOG_DIR="$ROOT_DIR/logs"
ACTION="${1:-restart}"
CORE_SESSION="chatos_local_connector_client_core"
FRONTEND_SESSION="chatos_local_connector_client_frontend"
CORE_LOG_FILE="$LOG_DIR/local_connector_client_core.log"
FRONTEND_LOG_FILE="$LOG_DIR/local_connector_client_frontend.log"

load_optional_env() {
  local env_file="$1"
  if [[ "${CHATOS_SKIP_SERVICE_LOCAL_ENV:-0}" == "1" ]]; then
    return 0
  fi
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

load_shared_env() {
  local env_file="${CHATOS_SHARED_ENV_FILE:-}"
  if [[ -z "$env_file" && "$ROOT_DIR" == /opt/chatos/* ]]; then
    env_file="/etc/chatos/chatos-backend.env"
  fi
  if [[ -n "$env_file" && -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

resolve_target_dir() {
  local target_dir="$1"
  if [[ "$target_dir" != /* ]]; then
    target_dir="$ROOT_DIR/$target_dir"
  fi
  printf '%s\n' "$target_dir"
}

load_shared_env
load_optional_env "$ROOT_DIR/.env"
load_optional_env "$ROOT_DIR/local_connector_service/.env"
load_optional_env "$CLIENT_DIR/.env"
load_optional_env "$CORE_DIR/.env"

SERVICE_PORT="${LOCAL_CONNECTOR_SERVICE_PORT:-39230}"
CORE_PORT="${LOCAL_CONNECTOR_CORE_API_PORT:-39232}"
DESKTOP_AUTH_TOKEN="${LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN:-$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')}"
FRONTEND_PORT="${LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT:-39233}"
CLOUD_BASE_URL="${LOCAL_CONNECTOR_CLOUD_BASE_URL:-${LOCAL_CONNECTOR_SERVICE_BASE_URL:-${CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL:-http://127.0.0.1:${SERVICE_PORT}}}}"
USER_SERVICE_BASE_URL="${LOCAL_CONNECTOR_USER_SERVICE_BASE_URL:-${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-http://127.0.0.1:${USER_SERVICE_PORT:-39190}}}}"
CARGO_TARGET_DIR_EFFECTIVE="$(resolve_target_dir "${LOCAL_CONNECTOR_CLIENT_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-$CLIENT_DIR/target}}")"

mkdir -p "$LOG_DIR"

pid_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n 1 || true
  fi
}

stop_port() {
  local port="$1"
  local name="$2"
  local pid
  pid="$(pid_for_port "$port")"
  if [[ -n "$pid" ]]; then
    echo "[INFO] stopping $name on port $port (pid=$pid)"
    kill "$pid" 2>/dev/null || true
    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
}

stop_session() {
  local session="$1"
  local name="$2"
  if command -v tmux >/dev/null 2>&1 && tmux has-session -t "$session" 2>/dev/null; then
    echo "[INFO] stopping $name tmux session ($session)"
    tmux kill-session -t "$session" 2>/dev/null || true
  fi
}

start_core() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "[ERROR] missing command: cargo" >&2
    return 1
  fi

  echo "[INFO] building native sandbox MCP agent"
  (
    cd "$ROOT_DIR"
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR_EFFECTIVE" cargo build -p chatos_sandbox_mcp_server
  )

  echo "[INFO] starting local connector client core on 127.0.0.1:$CORE_PORT"
  if command -v tmux >/dev/null 2>&1; then
    tmux new-session -d -s "$CORE_SESSION" \
      "cd '$ROOT_DIR' && CARGO_TARGET_DIR='$CARGO_TARGET_DIR_EFFECTIVE' LOCAL_CONNECTOR_CORE_API_PORT='$CORE_PORT' LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN='$DESKTOP_AUTH_TOKEN' LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT='$FRONTEND_PORT' LOCAL_CONNECTOR_CLOUD_BASE_URL='$CLOUD_BASE_URL' LOCAL_CONNECTOR_USER_SERVICE_BASE_URL='$USER_SERVICE_BASE_URL' LOCAL_CONNECTOR_ACCESS_TOKEN='${LOCAL_CONNECTOR_ACCESS_TOKEN:-}' LOCAL_CONNECTOR_DEVICE_NAME='${LOCAL_CONNECTOR_DEVICE_NAME:-}' LOCAL_CONNECTOR_PUBLIC_KEY='${LOCAL_CONNECTOR_PUBLIC_KEY:-}' LOCAL_CONNECTOR_WORKSPACE_PATH='${LOCAL_CONNECTOR_WORKSPACE_PATH:-}' LOCAL_CONNECTOR_WORKSPACE_ALIAS='${LOCAL_CONNECTOR_WORKSPACE_ALIAS:-}' LOCAL_CONNECTOR_STATE_PATH='${LOCAL_CONNECTOR_STATE_PATH:-}' cargo run -p local_connector_client_core 2>&1 | tee '$CORE_LOG_FILE'"
  else
    nohup bash -lc "cd '$ROOT_DIR' && CARGO_TARGET_DIR='$CARGO_TARGET_DIR_EFFECTIVE' LOCAL_CONNECTOR_CORE_API_PORT='$CORE_PORT' LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN='$DESKTOP_AUTH_TOKEN' LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT='$FRONTEND_PORT' LOCAL_CONNECTOR_CLOUD_BASE_URL='$CLOUD_BASE_URL' LOCAL_CONNECTOR_USER_SERVICE_BASE_URL='$USER_SERVICE_BASE_URL' LOCAL_CONNECTOR_ACCESS_TOKEN='${LOCAL_CONNECTOR_ACCESS_TOKEN:-}' LOCAL_CONNECTOR_DEVICE_NAME='${LOCAL_CONNECTOR_DEVICE_NAME:-}' LOCAL_CONNECTOR_PUBLIC_KEY='${LOCAL_CONNECTOR_PUBLIC_KEY:-}' LOCAL_CONNECTOR_WORKSPACE_PATH='${LOCAL_CONNECTOR_WORKSPACE_PATH:-}' LOCAL_CONNECTOR_WORKSPACE_ALIAS='${LOCAL_CONNECTOR_WORKSPACE_ALIAS:-}' LOCAL_CONNECTOR_STATE_PATH='${LOCAL_CONNECTOR_STATE_PATH:-}' exec cargo run -p local_connector_client_core" \
      >"$CORE_LOG_FILE" 2>&1 < /dev/null &
  fi
}

start_frontend() {
  if ! command -v npm >/dev/null 2>&1; then
    echo "[ERROR] missing command: npm" >&2
    return 1
  fi

  echo "[INFO] starting local connector client frontend on 127.0.0.1:$FRONTEND_PORT"
  if command -v tmux >/dev/null 2>&1; then
    tmux new-session -d -s "$FRONTEND_SESSION" \
      "cd '$FRONTEND_DIR' && LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT='$FRONTEND_PORT' LOCAL_CONNECTOR_CORE_API_PROXY_TARGET='http://127.0.0.1:$CORE_PORT' LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN='$DESKTOP_AUTH_TOKEN' npm run dev -- --host 127.0.0.1 --port '$FRONTEND_PORT' --strictPort 2>&1 | tee '$FRONTEND_LOG_FILE'"
  else
    nohup bash -lc "cd '$FRONTEND_DIR' && LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT='$FRONTEND_PORT' LOCAL_CONNECTOR_CORE_API_PROXY_TARGET='http://127.0.0.1:$CORE_PORT' LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN='$DESKTOP_AUTH_TOKEN' exec npm run dev -- --host 127.0.0.1 --port '$FRONTEND_PORT' --strictPort" \
      >"$FRONTEND_LOG_FILE" 2>&1 < /dev/null &
  fi
}

print_session_status() {
  local session="$1"
  local name="$2"
  if command -v tmux >/dev/null 2>&1 && tmux has-session -t "$session" 2>/dev/null; then
    echo "  $name tmux session: running ($session)"
  else
    echo "  $name tmux session: N/A"
  fi
}

print_port_status() {
  local port="$1"
  local name="$2"
  local pid
  pid="$(pid_for_port "$port")"
  if [[ -n "$pid" ]]; then
    echo "  $name port: $port (listening pid=$pid)"
  else
    echo "  $name port: $port (not listening)"
  fi
}

status() {
  echo "[INFO] local connector client status"
  print_session_status "$CORE_SESSION" "core"
  print_session_status "$FRONTEND_SESSION" "frontend"
  print_port_status "$CORE_PORT" "core"
  print_port_status "$FRONTEND_PORT" "frontend"
  echo
  echo "  core url: http://127.0.0.1:$CORE_PORT"
  echo "  frontend url: http://localhost:$FRONTEND_PORT"
  echo "  core log: $CORE_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
}

case "$ACTION" in
  stop)
    stop_session "$FRONTEND_SESSION" "local connector client frontend"
    stop_session "$CORE_SESSION" "local connector client core"
    stop_port "$FRONTEND_PORT" "local connector client frontend"
    stop_port "$CORE_PORT" "local connector client core"
    ;;
  start)
    start_core
    start_frontend
    ;;
  restart)
    stop_session "$FRONTEND_SESSION" "local connector client frontend"
    stop_session "$CORE_SESSION" "local connector client core"
    stop_port "$FRONTEND_PORT" "local connector client frontend"
    stop_port "$CORE_PORT" "local connector client core"
    start_core
    start_frontend
    ;;
  status)
    status
    ;;
  *)
    echo "Usage: $0 [start|stop|restart|status]" >&2
    exit 2
    ;;
esac

if [[ "$ACTION" != "status" ]]; then
  echo "[INFO] core log: $CORE_LOG_FILE"
  echo "[INFO] frontend log: $FRONTEND_LOG_FILE"
fi
