#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICE_DIR="$ROOT_DIR/sandbox_manager_service"
BACKEND_DIR="$SERVICE_DIR/backend"
FRONTEND_DIR="$SERVICE_DIR/frontend"
LOG_DIR="$ROOT_DIR/logs"
BACKEND_PORT="${SANDBOX_MANAGER_PORT:-8095}"
FRONTEND_PORT="${SANDBOX_MANAGER_FRONTEND_PORT:-8096}"
BACKEND_SESSION="chatos_sandbox_manager_backend"
FRONTEND_SESSION="chatos_sandbox_manager_frontend"
ACTION="${1:-restart}"

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
  echo "[INFO] starting sandbox manager backend on :$BACKEND_PORT"
  if command -v tmux >/dev/null 2>&1; then
    tmux new-session -d -s "$BACKEND_SESSION" \
      "cd '$ROOT_DIR' && SANDBOX_MANAGER_PORT='$BACKEND_PORT' cargo run -p sandbox_manager_service_backend 2>&1 | tee '$LOG_DIR/sandbox_manager_backend.log'"
  else
    nohup bash -lc "cd '$ROOT_DIR' && SANDBOX_MANAGER_PORT='$BACKEND_PORT' exec cargo run -p sandbox_manager_service_backend" \
      >"$LOG_DIR/sandbox_manager_backend.log" 2>&1 < /dev/null &
  fi
}

start_frontend() {
  echo "[INFO] starting sandbox manager frontend on :$FRONTEND_PORT"
  if command -v tmux >/dev/null 2>&1; then
    tmux new-session -d -s "$FRONTEND_SESSION" \
      "cd '$FRONTEND_DIR' && SANDBOX_MANAGER_FRONTEND_PORT='$FRONTEND_PORT' SANDBOX_MANAGER_API_PROXY_TARGET='http://127.0.0.1:$BACKEND_PORT' npm run dev -- --host 0.0.0.0 --port '$FRONTEND_PORT' 2>&1 | tee '$LOG_DIR/sandbox_manager_frontend.log'"
  else
    nohup bash -lc "cd '$FRONTEND_DIR' && SANDBOX_MANAGER_FRONTEND_PORT='$FRONTEND_PORT' SANDBOX_MANAGER_API_PROXY_TARGET='http://127.0.0.1:$BACKEND_PORT' exec npm run dev -- --host 0.0.0.0 --port '$FRONTEND_PORT'" \
      >"$LOG_DIR/sandbox_manager_frontend.log" 2>&1 < /dev/null &
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
  echo "[INFO] sandbox manager status"
  print_session_status "$BACKEND_SESSION" "backend"
  print_session_status "$FRONTEND_SESSION" "frontend"
  print_port_status "$BACKEND_PORT" "backend"
  print_port_status "$FRONTEND_PORT" "frontend"
  echo
  echo "  backend url: http://localhost:$BACKEND_PORT"
  echo "  frontend url: http://localhost:$FRONTEND_PORT"
  echo "  backend log: $LOG_DIR/sandbox_manager_backend.log"
  echo "  frontend log: $LOG_DIR/sandbox_manager_frontend.log"
}

case "$ACTION" in
  stop)
    stop_session "$FRONTEND_SESSION" "sandbox manager frontend"
    stop_session "$BACKEND_SESSION" "sandbox manager backend"
    stop_port "$FRONTEND_PORT" "sandbox manager frontend"
    stop_port "$BACKEND_PORT" "sandbox manager backend"
    ;;
  start)
    start_backend
    start_frontend
    ;;
  restart)
    stop_session "$FRONTEND_SESSION" "sandbox manager frontend"
    stop_session "$BACKEND_SESSION" "sandbox manager backend"
    stop_port "$FRONTEND_PORT" "sandbox manager frontend"
    stop_port "$BACKEND_PORT" "sandbox manager backend"
    start_backend
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
  echo "[INFO] backend log: $LOG_DIR/sandbox_manager_backend.log"
  echo "[INFO] frontend log: $LOG_DIR/sandbox_manager_frontend.log"
fi
