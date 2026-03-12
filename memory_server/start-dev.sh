#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi # CRLF-safe bootstrap for `bash start-dev.sh` #

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
BACKEND_DIR="$ROOT_DIR/backend"
FRONTEND_DIR="$ROOT_DIR/frontend"
BACKEND_ENV_FILE="$BACKEND_DIR/.env"

RUNTIME_DIR="${RUNTIME_DIR:-/tmp/memory_server_dev}"
BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"

BACKEND_PORT="${MEMORY_SERVER_BACKEND_PORT:-}"
FRONTEND_PORT="${MEMORY_SERVER_FRONTEND_PORT:-5176}"
FRONTEND_HOST="${MEMORY_SERVER_FRONTEND_HOST:-0.0.0.0}"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

read_port_from_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    return
  fi
  local port
  port="$(grep -E '^[[:space:]]*MEMORY_SERVER_PORT=' "$env_file" | tail -n 1 | cut -d '=' -f 2- | tr -d '"' | tr -d "'" | tr -d '[:space:]' || true)"
  if [[ -n "$port" ]]; then
    BACKEND_PORT="$port"
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
    echo "[INFO] 停止$name (pid=$pid)"
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

  if [[ -z "$port" ]]; then
    return
  fi

  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      echo "[INFO] 停止占用端口 ${port} 的${name}进程: ${pids}"
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
      echo "[INFO] 停止占用端口 ${port} 的${name}进程"
      fuser -k -n tcp "$port" >/dev/null 2>&1 || true
    fi
  fi
}

prepare() {
  need_cmd bash
  need_cmd cargo
  need_cmd npm

  if [[ ! -d "$BACKEND_DIR" || ! -d "$FRONTEND_DIR" ]]; then
    echo "[ERROR] 请在 memory_server 目录执行该脚本: $ROOT_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"

  if [[ ! -f "$BACKEND_ENV_FILE" ]]; then
    echo "[INFO] backend/.env 不存在，自动从 .env.example 复制"
    cp "$BACKEND_DIR/.env.example" "$BACKEND_ENV_FILE"
  fi

  if [[ ! -d "$FRONTEND_DIR/node_modules" ]]; then
    echo "[INFO] 前端依赖不存在，执行 npm install"
    (cd "$FRONTEND_DIR" && npm install)
  fi

  if [[ -z "$BACKEND_PORT" ]]; then
    read_port_from_env_file "$BACKEND_ENV_FILE"
  fi
  BACKEND_PORT="${BACKEND_PORT:-7080}"
}

start_backend() {
  echo "[INFO] 启动 backend..."
  nohup bash -lc "cd \"$BACKEND_DIR\" && if [[ -f .env ]]; then set -a; source .env; set +a; fi; cargo run --bin memory_server" >"$BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$BACKEND_PID_FILE"
}

start_frontend() {
  echo "[INFO] 启动 frontend..."
  nohup bash -lc "cd \"$FRONTEND_DIR\" && npm run dev -- --host \"$FRONTEND_HOST\" --port \"$FRONTEND_PORT\"" >"$FRONTEND_LOG_FILE" 2>&1 &
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
    tail -n 80 "$log_file" 2>/dev/null || true
    exit 1
  fi
}

do_stop() {
  stop_from_pid_file "backend" "$BACKEND_PID_FILE"
  stop_from_pid_file "frontend" "$FRONTEND_PID_FILE"
  stop_from_port "backend" "$BACKEND_PORT"
  stop_from_port "frontend" "$FRONTEND_PORT"
}

print_runtime_info() {
  echo "[OK] memory_server 前后端已在后台运行"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE")"
  echo "  frontend pid: $(cat "$FRONTEND_PID_FILE")"
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo "  backend url: http://localhost:$BACKEND_PORT"
  echo "  frontend url: http://localhost:$FRONTEND_PORT"
  echo
  echo "  tail backend log: tail -f $BACKEND_LOG_FILE"
  echo "  tail frontend log: tail -f $FRONTEND_LOG_FILE"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$FRONTEND_PID_FILE" 2>/dev/null || true)"
  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  backend pid: ${backend_pid:-N/A}"
  echo "  frontend pid: ${frontend_pid:-N/A}"
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart|start)
    do_stop
    start_backend
    start_frontend
    sleep 2
    check_alive "backend" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE"
    check_alive "frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE"
    print_runtime_info
    ;;
  stop)
    do_stop
    echo "[OK] memory_server 已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
