#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi # CRLF-safe bootstrap for `bash restart_services.sh` #

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
BACKEND_DIR="$ROOT_DIR/chat_app_server_rs"
FRONTEND_DIR="$ROOT_DIR/chat_app"

BACKEND_PORT="${BACKEND_PORT:-3001}"
FRONTEND_PORT="${FRONTEND_PORT:-8088}"

RUNTIME_DIR="${RUNTIME_DIR:-/tmp/chatos_rs_dev}"
BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

stop_from_pid_file() {
  local name="$1"
  local pid_file="$2"
  if [[ ! -f "$pid_file" ]]; then
    return
  fi
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    echo "[INFO] 停止 $name (pid=$pid)"
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
  fi
  rm -f "$pid_file"
}

stop_from_port() {
  local name="$1"
  local port="$2"

  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      echo "[INFO] 停止占用端口 $port 的 $name 进程: $pids"
      kill $pids >/dev/null 2>&1 || true
      sleep 1
      local left
      left="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
      if [[ -n "$left" ]]; then
        kill -9 $left >/dev/null 2>&1 || true
      fi
    fi
  elif command -v fuser >/dev/null 2>&1; then
    if fuser -n tcp "$port" >/dev/null 2>&1; then
      echo "[INFO] 停止占用端口 $port 的 $name 进程"
      fuser -k -n tcp "$port" >/dev/null 2>&1 || true
    fi
  fi
}

start_backend() {
  echo "[INFO] 启动后端..."
  nohup bash -lc "cd \"$BACKEND_DIR\" && cargo run --bin chat_app_server_rs" >"$BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$BACKEND_PID_FILE"
}

start_frontend() {
  echo "[INFO] 启动前端..."
  nohup bash -lc "cd \"$FRONTEND_DIR\" && npm run dev -- --host 0.0.0.0 --port $FRONTEND_PORT" >"$FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$FRONTEND_PID_FILE"
}

check_alive() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "[ERROR] $name 启动失败，请检查日志: $log_file"
    tail -n 60 "$log_file" 2>/dev/null || true
    exit 1
  fi
}

need_cmd bash
need_cmd npm
need_cmd cargo

if [[ ! -d "$BACKEND_DIR" || ! -d "$FRONTEND_DIR" ]]; then
  echo "[ERROR] 请在项目根目录执行该脚本: $ROOT_DIR"
  exit 1
fi

mkdir -p "$RUNTIME_DIR"

stop_from_pid_file "后端" "$BACKEND_PID_FILE"
stop_from_pid_file "前端" "$FRONTEND_PID_FILE"
stop_from_port "后端" "$BACKEND_PORT"
stop_from_port "前端" "$FRONTEND_PORT"

start_backend
start_frontend

sleep 2
check_alive "后端" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE"
check_alive "前端" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE"

echo "[OK] 前后端已重启并在后台运行"
echo "  后端 PID: $(cat "$BACKEND_PID_FILE")  日志: $BACKEND_LOG_FILE"
echo "  前端 PID: $(cat "$FRONTEND_PID_FILE")  日志: $FRONTEND_LOG_FILE"
echo "  前端地址: http://localhost:$FRONTEND_PORT"
echo "  后端地址: http://localhost:$BACKEND_PORT"
