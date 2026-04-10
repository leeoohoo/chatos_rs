#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi # CRLF-safe bootstrap for `bash restart_services.sh` #

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
MAIN_BACKEND_DIR="$ROOT_DIR/chat_app_server_rs"
MAIN_FRONTEND_DIR="$ROOT_DIR/chat_app"
MEMORY_ROOT_DIR="$ROOT_DIR/memory_server"
MEMORY_BACKEND_DIR="$MEMORY_ROOT_DIR/backend"
MEMORY_FRONTEND_DIR="$MEMORY_ROOT_DIR/frontend"
MEMORY_BACKEND_ENV_FILE="$MEMORY_BACKEND_DIR/.env"

MAIN_BACKEND_PORT=3997
LEGACY_MAIN_BACKEND_PORT=3001
MAIN_FRONTEND_PORT="${FRONTEND_PORT:-8088}"
MEMORY_BACKEND_PORT="${MEMORY_SERVER_BACKEND_PORT:-}"
MEMORY_FRONTEND_PORT="${MEMORY_SERVER_FRONTEND_PORT:-5176}"
MEMORY_FRONTEND_HOST="${MEMORY_SERVER_FRONTEND_HOST:-0.0.0.0}"

RUNTIME_DIR="${RUNTIME_DIR:-/tmp/chatos_rs_dev}"
MAIN_BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
MAIN_FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
MEMORY_BACKEND_PID_FILE="$RUNTIME_DIR/memory_backend.pid"
MEMORY_FRONTEND_PID_FILE="$RUNTIME_DIR/memory_frontend.pid"
MAIN_BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
MAIN_FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"
MEMORY_BACKEND_LOG_FILE="$RUNTIME_DIR/memory_backend.log"
MEMORY_FRONTEND_LOG_FILE="$RUNTIME_DIR/memory_frontend.log"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

read_memory_port_from_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    return
  fi
  local port
  port="$(grep -E '^[[:space:]]*MEMORY_SERVER_PORT=' "$env_file" | tail -n 1 | cut -d '=' -f 2- | tr -d '"' | tr -d "'" | tr -d '[:space:]' || true)"
  if [[ -n "$port" ]]; then
    MEMORY_BACKEND_PORT="$port"
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

prepare() {
  need_cmd bash
  need_cmd npm
  need_cmd cargo

  if [[ ! -d "$MAIN_BACKEND_DIR" || ! -d "$MAIN_FRONTEND_DIR" ]]; then
    echo "[ERROR] 原项目目录不完整: $MAIN_BACKEND_DIR / $MAIN_FRONTEND_DIR"
    exit 1
  fi

  if [[ ! -d "$MEMORY_BACKEND_DIR" || ! -d "$MEMORY_FRONTEND_DIR" ]]; then
    echo "[ERROR] memory_server 目录不完整: $MEMORY_BACKEND_DIR / $MEMORY_FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"

  if [[ ! -f "$MEMORY_BACKEND_ENV_FILE" && -f "$MEMORY_BACKEND_DIR/.env.example" ]]; then
    echo "[INFO] memory_server backend/.env 不存在，自动从 .env.example 复制"
    cp "$MEMORY_BACKEND_DIR/.env.example" "$MEMORY_BACKEND_ENV_FILE"
  fi

  if [[ -z "$MEMORY_BACKEND_PORT" ]]; then
    read_memory_port_from_env_file "$MEMORY_BACKEND_ENV_FILE"
  fi
  MEMORY_BACKEND_PORT="${MEMORY_BACKEND_PORT:-7080}"
}

start_main_backend() {
  echo "[INFO] 启动原项目 backend..."
  nohup bash -lc "cd \"$MAIN_BACKEND_DIR\" && BACKEND_PORT=\"$MAIN_BACKEND_PORT\" cargo run --bin chat_app_server_rs" >"$MAIN_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$MAIN_BACKEND_PID_FILE"
}

start_main_frontend() {
  echo "[INFO] 启动原项目 frontend..."
  nohup bash -lc "cd \"$MAIN_FRONTEND_DIR\" && npm run dev -- --host 0.0.0.0 --port \"$MAIN_FRONTEND_PORT\"" >"$MAIN_FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$MAIN_FRONTEND_PID_FILE"
}

start_memory_backend() {
  echo "[INFO] 启动 memory backend..."
  nohup bash -lc "cd \"$MEMORY_BACKEND_DIR\" && if [[ -f .env ]]; then set -a; source .env; set +a; fi; cargo run --bin memory_server" >"$MEMORY_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$MEMORY_BACKEND_PID_FILE"
}

start_memory_frontend() {
  echo "[INFO] 启动 memory frontend..."
  nohup bash -lc "cd \"$MEMORY_FRONTEND_DIR\" && npm run dev -- --host \"$MEMORY_FRONTEND_HOST\" --port \"$MEMORY_FRONTEND_PORT\"" >"$MEMORY_FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$MEMORY_FRONTEND_PID_FILE"
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

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-30}"

  if ! command -v curl >/dev/null 2>&1; then
    echo "[WARN] 未找到 curl，跳过 $name 健康检查: $url"
    return 0
  fi

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while true; do
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
      echo "[INFO] $name 健康检查通过: $url"
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name 健康检查超时 (${timeout_sec}s): $url"
      return 1
    fi
    sleep 1
  done
}

do_stop() {
  stop_from_pid_file "原项目 backend" "$MAIN_BACKEND_PID_FILE"
  stop_from_pid_file "原项目 frontend" "$MAIN_FRONTEND_PID_FILE"
  stop_from_pid_file "memory backend" "$MEMORY_BACKEND_PID_FILE"
  stop_from_pid_file "memory frontend" "$MEMORY_FRONTEND_PID_FILE"

  stop_from_port "原项目 backend" "$MAIN_BACKEND_PORT"
  if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
    stop_from_port "原项目 backend(legacy)" "$LEGACY_MAIN_BACKEND_PORT"
  fi
  stop_from_port "原项目 frontend" "$MAIN_FRONTEND_PORT"
  stop_from_port "memory backend" "$MEMORY_BACKEND_PORT"
  stop_from_port "memory frontend" "$MEMORY_FRONTEND_PORT"
}

print_runtime_info() {
  echo "[OK] 全部服务已在后台运行"
  echo "  原项目 backend pid: $(cat "$MAIN_BACKEND_PID_FILE")"
  echo "  原项目 frontend pid: $(cat "$MAIN_FRONTEND_PID_FILE")"
  echo "  memory backend pid: $(cat "$MEMORY_BACKEND_PID_FILE")"
  echo "  memory frontend pid: $(cat "$MEMORY_FRONTEND_PID_FILE")"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  memory backend log: $MEMORY_BACKEND_LOG_FILE"
  echo "  memory frontend log: $MEMORY_FRONTEND_LOG_FILE"
  echo
  echo "  原项目 frontend url: http://localhost:$MAIN_FRONTEND_PORT"
  echo "  原项目 backend url: http://localhost:$MAIN_BACKEND_PORT"
  echo "  memory frontend url: http://localhost:$MEMORY_FRONTEND_PORT"
  echo "  memory backend url: http://localhost:$MEMORY_BACKEND_PORT"
}

status() {
  local main_backend_pid main_frontend_pid memory_backend_pid memory_frontend_pid
  main_backend_pid="$(cat "$MAIN_BACKEND_PID_FILE" 2>/dev/null || true)"
  main_frontend_pid="$(cat "$MAIN_FRONTEND_PID_FILE" 2>/dev/null || true)"
  memory_backend_pid="$(cat "$MEMORY_BACKEND_PID_FILE" 2>/dev/null || true)"
  memory_frontend_pid="$(cat "$MEMORY_FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  原项目 backend pid: ${main_backend_pid:-N/A}"
  echo "  原项目 frontend pid: ${main_frontend_pid:-N/A}"
  echo "  memory backend pid: ${memory_backend_pid:-N/A}"
  echo "  memory frontend pid: ${memory_frontend_pid:-N/A}"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  memory backend log: $MEMORY_BACKEND_LOG_FILE"
  echo "  memory frontend log: $MEMORY_FRONTEND_LOG_FILE"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart|start)
    STARTUP_HEALTHCHECK_TIMEOUT_SEC="${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}"
    do_stop
    start_main_backend
    start_main_frontend
    start_memory_backend
    start_memory_frontend
    sleep 2
    check_alive "原项目 backend" "$MAIN_BACKEND_PID_FILE" "$MAIN_BACKEND_LOG_FILE"
    check_alive "原项目 frontend" "$MAIN_FRONTEND_PID_FILE" "$MAIN_FRONTEND_LOG_FILE"
    check_alive "memory backend" "$MEMORY_BACKEND_PID_FILE" "$MEMORY_BACKEND_LOG_FILE"
    check_alive "memory frontend" "$MEMORY_FRONTEND_PID_FILE" "$MEMORY_FRONTEND_LOG_FILE"

    wait_http_ready "原项目 backend" "http://127.0.0.1:$MAIN_BACKEND_PORT/health" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC"
    wait_http_ready "原项目 frontend" "http://127.0.0.1:$MAIN_FRONTEND_PORT" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC"
    wait_http_ready "memory backend" "http://127.0.0.1:$MEMORY_BACKEND_PORT/health" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC"
    wait_http_ready "memory frontend" "http://127.0.0.1:$MEMORY_FRONTEND_PORT" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC"

    print_runtime_info
    ;;
  stop)
    do_stop
    echo "[OK] 全部服务已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
