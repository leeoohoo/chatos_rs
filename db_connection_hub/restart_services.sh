#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi # CRLF-safe bootstrap for `bash restart_services.sh` #

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
BACKEND_DIR="$ROOT_DIR/backend"
FRONTEND_DIR="$ROOT_DIR/frontend"

DB_HUB_BACKEND_HOST="${DB_HUB_BACKEND_HOST:-0.0.0.0}"
DB_HUB_BACKEND_PORT="${DB_HUB_BACKEND_PORT:-${DB_HUB_PORT:-8099}}"
DB_HUB_FRONTEND_HOST="${DB_HUB_FRONTEND_HOST:-0.0.0.0}"
DB_HUB_FRONTEND_PORT="${DB_HUB_FRONTEND_PORT:-5174}"
DB_HUB_BACKEND_RUST_LOG="${DB_HUB_BACKEND_RUST_LOG:-info}"
DB_HUB_DEV_BACKEND_ORIGIN="${DB_HUB_DEV_BACKEND_ORIGIN:-http://127.0.0.1:${DB_HUB_BACKEND_PORT}}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="default"
fi

RUNTIME_DIR="${RUNTIME_DIR:-/tmp/db_connection_hub_dev_${ROOT_HASH}}"
STOP_BY_PORT="${STOP_BY_PORT:-1}"

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

is_port_listening() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -n "$pids" ]]
    return
  fi
  if command -v fuser >/dev/null 2>&1; then
    fuser -n tcp "$port" >/dev/null 2>&1
    return
  fi
  return 1
}

ensure_port_available() {
  local name="$1"
  local port="$2"
  local retries="${PORT_RELEASE_RETRIES:-15}"
  local sleep_sec="${PORT_RELEASE_SLEEP_SEC:-1}"
  local i
  for ((i = 0; i < retries; i++)); do
    if ! is_port_listening "$port"; then
      return 0
    fi
    sleep "$sleep_sec"
  done

  echo "[ERROR] $name 端口已被占用: $port"
  if command -v lsof >/dev/null 2>&1; then
    echo "[INFO] 当前占用详情："
    lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
  fi
  return 1
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
    if command -v pgrep >/dev/null 2>&1; then
      local children
      children="$(pgrep -P "$pid" 2>/dev/null || true)"
      if [[ -n "$children" ]]; then
        kill $children >/dev/null 2>&1 || true
      fi
    fi
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
    if command -v pgrep >/dev/null 2>&1; then
      local left_children
      left_children="$(pgrep -P "$pid" 2>/dev/null || true)"
      if [[ -n "$left_children" ]]; then
        kill -9 $left_children >/dev/null 2>&1 || true
      fi
    fi
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

check_alive() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "[ERROR] $name 启动失败，请检查日志: $log_file"
    tail -n 80 "$log_file" 2>/dev/null || true
    return 1
  fi
}

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-45}"

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

prepare() {
  need_cmd bash
  need_cmd cargo
  need_cmd npm

  if [[ ! -d "$BACKEND_DIR" || ! -d "$FRONTEND_DIR" ]]; then
    echo "[ERROR] db_connection_hub 目录不完整: $BACKEND_DIR / $FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"
}

start_backend() {
  ensure_port_available "db_connection_hub backend" "$DB_HUB_BACKEND_PORT" || return 1
  echo "[INFO] 启动 db_connection_hub backend..."
  nohup bash -lc "cd \"$BACKEND_DIR\" && DB_HUB_HOST=\"$DB_HUB_BACKEND_HOST\" DB_HUB_PORT=\"$DB_HUB_BACKEND_PORT\" RUST_LOG=\"$DB_HUB_BACKEND_RUST_LOG\" cargo run --bin db_connection_hub_backend" >"$BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$BACKEND_PID_FILE"
}

start_frontend() {
  ensure_port_available "db_connection_hub frontend" "$DB_HUB_FRONTEND_PORT" || return 1
  echo "[INFO] 启动 db_connection_hub frontend..."
  nohup bash -lc "cd \"$FRONTEND_DIR\" && VITE_DEV_BACKEND_ORIGIN=\"$DB_HUB_DEV_BACKEND_ORIGIN\" npm run dev -- --host \"$DB_HUB_FRONTEND_HOST\" --port \"$DB_HUB_FRONTEND_PORT\"" >"$FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$FRONTEND_PID_FILE"
}

do_stop() {
  stop_from_pid_file "db_connection_hub backend" "$BACKEND_PID_FILE"
  stop_from_pid_file "db_connection_hub frontend" "$FRONTEND_PID_FILE"

  if [[ "$STOP_BY_PORT" == "1" ]]; then
    stop_from_port "db_connection_hub backend" "$DB_HUB_BACKEND_PORT"
    stop_from_port "db_connection_hub frontend" "$DB_HUB_FRONTEND_PORT"
  else
    echo "[INFO] 跳过按端口全局停止 (STOP_BY_PORT=${STOP_BY_PORT})，仅按 PID 文件停止。"
  fi
}

run_start_sequence() {
  start_backend &&
    start_frontend &&
    sleep 2 &&
    check_alive "db_connection_hub backend" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE" &&
    check_alive "db_connection_hub frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE" &&
    wait_http_ready "db_connection_hub backend" "http://127.0.0.1:$DB_HUB_BACKEND_PORT/api/v1/health" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC" &&
    wait_http_ready "db_connection_hub frontend" "http://127.0.0.1:$DB_HUB_FRONTEND_PORT" "$STARTUP_HEALTHCHECK_TIMEOUT_SEC"
}

print_runtime_info() {
  echo "[OK] db_connection_hub 服务已在后台运行"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE")"
  echo "  frontend pid: $(cat "$FRONTEND_PID_FILE")"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  backend url: http://localhost:$DB_HUB_BACKEND_PORT"
  echo "  frontend url: http://localhost:$DB_HUB_FRONTEND_PORT"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  backend pid: ${backend_pid:-N/A}"
  echo "  frontend pid: ${frontend_pid:-N/A}"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  backend url: http://localhost:$DB_HUB_BACKEND_PORT"
  echo "  frontend url: http://localhost:$DB_HUB_FRONTEND_PORT"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart|start)
    STARTUP_HEALTHCHECK_TIMEOUT_SEC="${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}"
    do_stop
    if run_start_sequence; then
      print_runtime_info
    else
      echo "[WARN] 启动失败，正在回滚已启动的服务..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] db_connection_hub 服务已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
