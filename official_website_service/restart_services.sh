#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKEND_DIR="$SCRIPT_DIR/backend"
FRONTEND_DIR="$SCRIPT_DIR/frontend"
RUNTIME_ROOT="${RUNTIME_DIR:-/tmp}"
RUNTIME_DIR_EFFECTIVE="${OFFICIAL_WEBSITE_RUNTIME_DIR:-$RUNTIME_ROOT/chatos_rs_official_website}"
BACKEND_PID_FILE="$RUNTIME_DIR_EFFECTIVE/backend.pid"
FRONTEND_PID_FILE="$RUNTIME_DIR_EFFECTIVE/frontend.pid"
BACKEND_LOG_FILE="$RUNTIME_DIR_EFFECTIVE/backend.log"
FRONTEND_LOG_FILE="$RUNTIME_DIR_EFFECTIVE/frontend.log"
BACKEND_SESSION="${OFFICIAL_WEBSITE_BACKEND_SESSION:-chatos_official_website_backend}"
FRONTEND_SESSION="${OFFICIAL_WEBSITE_FRONTEND_SESSION:-chatos_official_website_frontend}"

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

load_optional_env "$ROOT_DIR/.env"
load_optional_env "$SCRIPT_DIR/.env"
load_optional_env "$BACKEND_DIR/.env"

OFFICIAL_WEBSITE_HOST="${OFFICIAL_WEBSITE_HOST:-127.0.0.1}"
OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-39250}"
OFFICIAL_WEBSITE_FRONTEND_PORT="${OFFICIAL_WEBSITE_FRONTEND_PORT:-39251}"
OFFICIAL_WEBSITE_STATIC_DIR="${OFFICIAL_WEBSITE_STATIC_DIR:-$FRONTEND_DIR/dist}"
OFFICIAL_WEBSITE_MODE="${OFFICIAL_WEBSITE_MODE:-dev}"

mkdir -p "$RUNTIME_DIR_EFFECTIVE"

normalize_mode() {
  case "$OFFICIAL_WEBSITE_MODE" in
    dev|development)
      echo "dev"
      ;;
    prod|production)
      echo "prod"
      ;;
    *)
      echo "[ERROR] OFFICIAL_WEBSITE_MODE must be dev or prod, got: $OFFICIAL_WEBSITE_MODE" >&2
      return 1
      ;;
  esac
}

OFFICIAL_WEBSITE_MODE_EFFECTIVE="$(normalize_mode)"

is_port_listening() {
  local port="$1"
  lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
}

ensure_static_build() {
  if [[ "$OFFICIAL_WEBSITE_MODE_EFFECTIVE" == "prod" && ! -f "$OFFICIAL_WEBSITE_STATIC_DIR/index.html" ]]; then
    echo "[ERROR] production mode requires a built frontend at $OFFICIAL_WEBSITE_STATIC_DIR"
    echo "        Run: make build-official-website"
    return 1
  fi
}

stop_pid_file() {
  local label="$1"
  local pid_file="$2"
  if [[ -f "$pid_file" ]]; then
    local pid
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      echo "[INFO] stopping $label pid=$pid"
      kill "$pid" >/dev/null 2>&1 || true
    fi
    rm -f "$pid_file"
  fi
}

stop_tmux_session() {
  local session="$1"
  local label="$2"
  if command -v tmux >/dev/null 2>&1 && tmux has-session -t "$session" 2>/dev/null; then
    echo "[INFO] stopping $label tmux session"
    tmux kill-session -t "$session" >/dev/null 2>&1 || true
  fi
}

stop_port() {
  local label="$1"
  local port="$2"
  local pids
  pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$pids" ]]; then
    echo "[INFO] stopping $label listeners on :$port"
    # shellcheck disable=SC2086
    kill $pids >/dev/null 2>&1 || true
  fi
}

wait_port_released() {
  local port="$1"
  for _ in {1..40}; do
    if ! is_port_listening "$port"; then
      return 0
    fi
    sleep 0.25
  done
  return 1
}

start_process() {
  local label="$1"
  local port="$2"
  local pid_file="$3"
  local log_file="$4"
  local command="$5"
  local session="$6"

  if is_port_listening "$port"; then
    echo "[WARN] $label port :$port is already in use"
    return 1
  fi

  echo "[INFO] starting $label on :$port"
  if command -v tmux >/dev/null 2>&1; then
    local escaped_command
    local escaped_log_file
    printf -v escaped_command '%q' "$command"
    printf -v escaped_log_file '%q' "$log_file"
    tmux new-session -d -s "$session" \
      "bash -lc $escaped_command 2>&1 | tee $escaped_log_file"
    tmux display-message -p -t "$session" '#{pane_pid}' >"$pid_file"
  elif command -v setsid >/dev/null 2>&1; then
    nohup setsid bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
    echo "$!" >"$pid_file"
  else
    nohup bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
    echo "$!" >"$pid_file"
  fi
}

start_services() {
  ensure_static_build

  start_process \
    "official website backend" \
    "$OFFICIAL_WEBSITE_PORT" \
    "$BACKEND_PID_FILE" \
    "$BACKEND_LOG_FILE" \
    "cd '$BACKEND_DIR' && OFFICIAL_WEBSITE_HOST='$OFFICIAL_WEBSITE_HOST' OFFICIAL_WEBSITE_PORT='$OFFICIAL_WEBSITE_PORT' OFFICIAL_WEBSITE_STATIC_DIR='$OFFICIAL_WEBSITE_STATIC_DIR' exec cargo run" \
    "$BACKEND_SESSION"

  if [[ "$OFFICIAL_WEBSITE_MODE_EFFECTIVE" == "dev" ]]; then
    start_process \
      "official website frontend" \
      "$OFFICIAL_WEBSITE_FRONTEND_PORT" \
      "$FRONTEND_PID_FILE" \
      "$FRONTEND_LOG_FILE" \
      "cd '$FRONTEND_DIR' && exec npm run dev -- --host 0.0.0.0 --port '$OFFICIAL_WEBSITE_FRONTEND_PORT'" \
      "$FRONTEND_SESSION"
  else
    echo "[INFO] production mode: frontend dev server disabled"
  fi
}

stop_services() {
  stop_tmux_session "$FRONTEND_SESSION" "official website frontend"
  stop_tmux_session "$BACKEND_SESSION" "official website backend"
  stop_pid_file "official website frontend" "$FRONTEND_PID_FILE"
  stop_pid_file "official website backend" "$BACKEND_PID_FILE"
  stop_port "official website frontend" "$OFFICIAL_WEBSITE_FRONTEND_PORT"
  stop_port "official website backend" "$OFFICIAL_WEBSITE_PORT"
  wait_port_released "$OFFICIAL_WEBSITE_FRONTEND_PORT" || true
  wait_port_released "$OFFICIAL_WEBSITE_PORT" || true
}

status_services() {
  echo "official website:"
  echo "  mode: $OFFICIAL_WEBSITE_MODE_EFFECTIVE"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE" 2>/dev/null || echo '-')"
  echo "  site url: http://localhost:$OFFICIAL_WEBSITE_PORT"
  if [[ "$OFFICIAL_WEBSITE_MODE_EFFECTIVE" == "dev" ]]; then
    echo "  frontend dev pid: $(cat "$FRONTEND_PID_FILE" 2>/dev/null || echo '-')"
    echo "  frontend dev url: http://localhost:$OFFICIAL_WEBSITE_FRONTEND_PORT"
  else
    echo "  frontend dev pid: $(cat "$FRONTEND_PID_FILE" 2>/dev/null || echo '-') (disabled on prod start)"
  fi
  echo "  static dir: $OFFICIAL_WEBSITE_STATIC_DIR"
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
}

case "${1:-restart}" in
  restart|start)
    stop_services
    start_services
    status_services
    ;;
  stop)
    stop_services
    echo "[OK] official website stopped"
    ;;
  status)
    status_services
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
