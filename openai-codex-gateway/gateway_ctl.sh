#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_FILE="$SCRIPT_DIR/server.py"

RUNTIME_DIR="${CODEX_GATEWAY_RUNTIME_DIR:-/tmp/chatos_rs_dev}"
PID_FILE="${CODEX_GATEWAY_PID_FILE:-$RUNTIME_DIR/codex_gateway.pid}"
LOG_FILE="${CODEX_GATEWAY_LOG_FILE:-$RUNTIME_DIR/codex_gateway.log}"

HOST="${CODEX_GATEWAY_HOST:-127.0.0.1}"
PORT="${CODEX_GATEWAY_PORT:-8089}"

PYTHON_BIN="${PYTHON_BIN:-}"
if [[ -z "$PYTHON_BIN" ]]; then
  if command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN="python3"
  elif command -v python >/dev/null 2>&1; then
    PYTHON_BIN="python"
  else
    echo "[ERROR] python3/python not found"
    exit 1
  fi
fi

ACTION="${1:-start}"

mkdir -p "$RUNTIME_DIR"

is_pid_running() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1
}

read_pid() {
  if [[ -f "$PID_FILE" ]]; then
    cat "$PID_FILE" 2>/dev/null || true
  fi
}

print_status() {
  local pid
  pid="$(read_pid)"
  if is_pid_running "$pid"; then
    echo "[INFO] codex gateway is running (pid=$pid)"
  else
    echo "[INFO] codex gateway is not running"
  fi
  echo "[INFO] host=$HOST port=$PORT"
  echo "[INFO] pid_file=$PID_FILE"
  echo "[INFO] log_file=$LOG_FILE"
}

start_gateway() {
  local pid
  pid="$(read_pid)"
  if is_pid_running "$pid"; then
    echo "[INFO] codex gateway already running (pid=$pid)"
    echo "[INFO] log_file=$LOG_FILE"
    exit 0
  fi

  rm -f "$PID_FILE"
  touch "$LOG_FILE"

  local -a cmd=(
    "$PYTHON_BIN" -u "$SERVER_FILE"
    --host "$HOST"
    --port "$PORT"
  )

  if [[ -n "${CODEX_GATEWAY_CODEX_BIN:-}" ]]; then
    cmd+=(--codex-bin "$CODEX_GATEWAY_CODEX_BIN")
  fi
  if [[ -n "${CODEX_GATEWAY_STATE_DB:-}" ]]; then
    cmd+=(--state-db "$CODEX_GATEWAY_STATE_DB")
  fi
  if [[ -n "${CODEX_GATEWAY_CWD:-}" ]]; then
    cmd+=(--cwd "$CODEX_GATEWAY_CWD")
  fi
  if [[ -n "${CODEX_GATEWAY_SANDBOX:-}" ]]; then
    cmd+=(--sandbox "$CODEX_GATEWAY_SANDBOX")
  fi

  echo "[INFO] starting codex gateway..."
  nohup "${cmd[@]}" >>"$LOG_FILE" 2>&1 &
  local new_pid="$!"
  echo "$new_pid" >"$PID_FILE"

  sleep 1
  if is_pid_running "$new_pid"; then
    echo "[INFO] codex gateway started (pid=$new_pid)"
    echo "[INFO] log_file=$LOG_FILE"
  else
    echo "[ERROR] codex gateway failed to start, check log:"
    echo "  $LOG_FILE"
    rm -f "$PID_FILE"
    exit 1
  fi
}

stop_gateway() {
  local pid
  pid="$(read_pid)"
  if ! is_pid_running "$pid"; then
    echo "[INFO] codex gateway is not running"
    rm -f "$PID_FILE"
    exit 0
  fi

  echo "[INFO] stopping codex gateway (pid=$pid)..."
  kill "$pid" >/dev/null 2>&1 || true
  sleep 1
  if is_pid_running "$pid"; then
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi
  rm -f "$PID_FILE"
  echo "[INFO] codex gateway stopped"
}

tail_logs() {
  touch "$LOG_FILE"
  echo "[INFO] tailing $LOG_FILE"
  tail -n 200 -f "$LOG_FILE"
}

case "$ACTION" in
  start)
    start_gateway
    ;;
  stop)
    stop_gateway
    ;;
  restart)
    stop_gateway || true
    start_gateway
    ;;
  status)
    print_status
    ;;
  tail)
    tail_logs
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status|tail}"
    exit 1
    ;;
esac
