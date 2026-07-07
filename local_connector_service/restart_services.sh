#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICE_DIR="$ROOT_DIR/local_connector_service"
BACKEND_DIR="$SERVICE_DIR/backend"
LOG_DIR="$ROOT_DIR/logs"
ACTION="${1:-restart}"
BACKEND_SESSION="chatos_local_connector_service_backend"
BACKEND_LOG_FILE="$LOG_DIR/local_connector_service_backend.log"

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

client_accessible_host() {
  local host="$1"
  case "$host" in
    ""|"0.0.0.0"|"::"|"[::]")
      printf '%s\n' "127.0.0.1"
      ;;
    *)
      printf '%s\n' "$host"
      ;;
  esac
}

load_shared_env
load_optional_env "$ROOT_DIR/.env"
load_optional_env "$SERVICE_DIR/.env"
load_optional_env "$BACKEND_DIR/.env"

BACKEND_HOST="${LOCAL_CONNECTOR_SERVICE_HOST:-127.0.0.1}"
BACKEND_PORT="${LOCAL_CONNECTOR_SERVICE_PORT:-39230}"
PUBLIC_HOST="$(client_accessible_host "$BACKEND_HOST")"
DATABASE_URL="${LOCAL_CONNECTOR_DATABASE_URL:-sqlite://local_connector_service/data/local_connector.db}"
USER_SERVICE_BASE_URL="${LOCAL_CONNECTOR_USER_SERVICE_BASE_URL:-${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-http://127.0.0.1:${USER_SERVICE_PORT:-39190}}}}"
PUBLIC_BASE_URL="${LOCAL_CONNECTOR_PUBLIC_BASE_URL:-http://${PUBLIC_HOST}:${BACKEND_PORT}}"
INTERNAL_API_SECRET="${LOCAL_CONNECTOR_INTERNAL_API_SECRET:-${CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-${TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-chatos-local-connector-dev-secret}}}"
CARGO_TARGET_DIR_EFFECTIVE="$(resolve_target_dir "${LOCAL_CONNECTOR_SERVICE_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-$SERVICE_DIR/target}}")"

mkdir -p "$LOG_DIR" "$SERVICE_DIR/data"

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

start_backend() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "[ERROR] missing command: cargo" >&2
    return 1
  fi

  echo "[INFO] starting local connector service backend on ${BACKEND_HOST}:${BACKEND_PORT}"
  if command -v tmux >/dev/null 2>&1; then
    tmux new-session -d -s "$BACKEND_SESSION" \
      "cd '$ROOT_DIR' && CARGO_TARGET_DIR='$CARGO_TARGET_DIR_EFFECTIVE' LOCAL_CONNECTOR_SERVICE_HOST='$BACKEND_HOST' LOCAL_CONNECTOR_SERVICE_PORT='$BACKEND_PORT' LOCAL_CONNECTOR_DATABASE_URL='$DATABASE_URL' LOCAL_CONNECTOR_USER_SERVICE_BASE_URL='$USER_SERVICE_BASE_URL' LOCAL_CONNECTOR_PUBLIC_BASE_URL='$PUBLIC_BASE_URL' LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' cargo run -p local_connector_service_backend 2>&1 | tee '$BACKEND_LOG_FILE'"
  else
    nohup bash -lc "cd '$ROOT_DIR' && CARGO_TARGET_DIR='$CARGO_TARGET_DIR_EFFECTIVE' LOCAL_CONNECTOR_SERVICE_HOST='$BACKEND_HOST' LOCAL_CONNECTOR_SERVICE_PORT='$BACKEND_PORT' LOCAL_CONNECTOR_DATABASE_URL='$DATABASE_URL' LOCAL_CONNECTOR_USER_SERVICE_BASE_URL='$USER_SERVICE_BASE_URL' LOCAL_CONNECTOR_PUBLIC_BASE_URL='$PUBLIC_BASE_URL' LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET='$INTERNAL_API_SECRET' exec cargo run -p local_connector_service_backend" \
      >"$BACKEND_LOG_FILE" 2>&1 < /dev/null &
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
  echo "[INFO] local connector service status"
  print_session_status "$BACKEND_SESSION" "backend"
  print_port_status "$BACKEND_PORT" "backend"
  echo
  echo "  backend url: $PUBLIC_BASE_URL"
  echo "  backend log: $BACKEND_LOG_FILE"
}

case "$ACTION" in
  stop)
    stop_session "$BACKEND_SESSION" "local connector service backend"
    stop_port "$BACKEND_PORT" "local connector service backend"
    ;;
  start)
    start_backend
    ;;
  restart)
    stop_session "$BACKEND_SESSION" "local connector service backend"
    stop_port "$BACKEND_PORT" "local connector service backend"
    start_backend
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
  echo "[INFO] backend log: $BACKEND_LOG_FILE"
fi
