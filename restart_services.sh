#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
MAIN_BACKEND_DIR="$ROOT_DIR/chat_app_server_rs"
MAIN_FRONTEND_DIR="$ROOT_DIR/chat_app"

MAIN_BACKEND_PORT="${MAIN_BACKEND_PORT:-${BACKEND_PORT:-3997}}"
LEGACY_MAIN_BACKEND_PORT=3001
MAIN_FRONTEND_PORT="${FRONTEND_PORT:-8088}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="default"
fi

RUNTIME_DIR="${RUNTIME_DIR:-/tmp/chatos_rs_dev_${ROOT_HASH}}"
LEGACY_RUNTIME_DIR="/tmp/chatos_rs_dev"
STOP_BY_PORT="${STOP_BY_PORT:-1}"

MAIN_BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
MAIN_FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
MAIN_BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
MAIN_FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"
MAIN_BACKEND_BINARY="$ROOT_DIR/target-shared/debug/chat_app_server_rs"

LEGACY_MAIN_BACKEND_PID_FILE="$LEGACY_RUNTIME_DIR/backend.pid"
LEGACY_MAIN_FRONTEND_PID_FILE="$LEGACY_RUNTIME_DIR/frontend.pid"

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

stop_project_owned_port_processes() {
  local name="$1"
  local port="$2"

  if ! command -v lsof >/dev/null 2>&1; then
    return
  fi

  local pids
  pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -z "$pids" ]]; then
    return
  fi

  local pid cwd_path
  for pid in $pids; do
    cwd_path="$(lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1)"
    if [[ -z "$cwd_path" ]]; then
      continue
    fi
    if [[ "$cwd_path" == "$ROOT_DIR"* ]]; then
      echo "[INFO] 停止当前项目残留的 $name 进程 (pid=$pid, port=$port, cwd=$cwd_path)"
      kill "$pid" >/dev/null 2>&1 || true
      sleep 1
      if kill -0 "$pid" >/dev/null 2>&1; then
        kill -9 "$pid" >/dev/null 2>&1 || true
      fi
    fi
  done
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
  if is_port_listening "$port"; then
    echo "[ERROR] $name 端口已被占用: $port"
    if command -v lsof >/dev/null 2>&1; then
      echo "[INFO] 当前占用详情："
      lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
    fi
    echo "[HINT] 请改用其它端口（例如 MAIN_BACKEND_PORT/BACKEND_PORT），或先停止占用该端口的服务。"
    return 1
  fi
}

launch_service() {
  local name="$1"
  local port="$2"
  local pid_file="$3"
  local log_file="$4"
  local command="$5"

  ensure_port_available "$name" "$port" || return 1
  echo "[INFO] 启动 $name..."
  : >"$log_file"
  nohup bash -lc "$command" >"$log_file" 2>&1 &
  echo $! >"$pid_file"
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
    return 1
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

wait_port_released() {
  local name="$1"
  local port="$2"
  local timeout_sec="${3:-15}"

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while is_port_listening "$port"; do
    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name 端口未在预期时间内释放: $port"
      if command -v lsof >/dev/null 2>&1; then
        lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
      fi
      return 1
    fi
    sleep 1
  done
}

prepare() {
  need_cmd bash
  need_cmd npm
  need_cmd cargo

  if [[ ! -d "$MAIN_BACKEND_DIR" || ! -d "$MAIN_FRONTEND_DIR" ]]; then
    echo "[ERROR] 项目目录不完整: $MAIN_BACKEND_DIR / $MAIN_FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"
}

start_main_backend() {
  launch_service \
    "原项目 backend" \
    "$MAIN_BACKEND_PORT" \
    "$MAIN_BACKEND_PID_FILE" \
    "$MAIN_BACKEND_LOG_FILE" \
    "cd \"$MAIN_BACKEND_DIR\" && if [[ -f .env ]]; then set -a; source .env; set +a; fi; cargo build --bin chat_app_server_rs && BACKEND_PORT=\"$MAIN_BACKEND_PORT\" exec \"$MAIN_BACKEND_BINARY\""
}

start_main_frontend() {
  launch_service \
    "原项目 frontend" \
    "$MAIN_FRONTEND_PORT" \
    "$MAIN_FRONTEND_PID_FILE" \
    "$MAIN_FRONTEND_LOG_FILE" \
    "cd \"$MAIN_FRONTEND_DIR\" && exec npm run dev -- --host 0.0.0.0 --port \"$MAIN_FRONTEND_PORT\""
}

do_stop() {
  stop_from_pid_file "原项目 backend" "$MAIN_BACKEND_PID_FILE"
  stop_from_pid_file "原项目 frontend" "$MAIN_FRONTEND_PID_FILE"

  if [[ "$LEGACY_RUNTIME_DIR" != "$RUNTIME_DIR" ]]; then
    stop_from_pid_file "原项目 backend(legacy runtime)" "$LEGACY_MAIN_BACKEND_PID_FILE"
    stop_from_pid_file "原项目 frontend(legacy runtime)" "$LEGACY_MAIN_FRONTEND_PID_FILE"
  fi

  if [[ "$STOP_BY_PORT" == "1" ]]; then
    stop_from_port "原项目 backend" "$MAIN_BACKEND_PORT"
    if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
      stop_from_port "原项目 backend(legacy)" "$LEGACY_MAIN_BACKEND_PORT"
    fi
    stop_from_port "原项目 frontend" "$MAIN_FRONTEND_PORT"
  else
    echo "[INFO] 跳过按端口全局停止 (STOP_BY_PORT=${STOP_BY_PORT})，仅按 PID 文件停止，避免误伤其他项目。"
    stop_project_owned_port_processes "原项目 backend" "$MAIN_BACKEND_PORT"
    if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
      stop_project_owned_port_processes "原项目 backend(legacy)" "$LEGACY_MAIN_BACKEND_PORT"
    fi
    stop_project_owned_port_processes "原项目 frontend" "$MAIN_FRONTEND_PORT"
  fi

  wait_port_released "原项目 backend" "$MAIN_BACKEND_PORT" || return 1
  if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
    wait_port_released "原项目 backend(legacy)" "$LEGACY_MAIN_BACKEND_PORT" || return 1
  fi
  wait_port_released "原项目 frontend" "$MAIN_FRONTEND_PORT" || return 1
}

run_start_sequence() {
  local backend_timeout="${MAIN_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-120}}"
  local frontend_timeout="${MAIN_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}}"

  start_main_backend &&
    start_main_frontend

  sleep 2 &&
    check_alive "原项目 backend" "$MAIN_BACKEND_PID_FILE" "$MAIN_BACKEND_LOG_FILE" &&
    check_alive "原项目 frontend" "$MAIN_FRONTEND_PID_FILE" "$MAIN_FRONTEND_LOG_FILE" &&
    wait_http_ready "原项目 backend" "http://127.0.0.1:$MAIN_BACKEND_PORT/health" "$backend_timeout" &&
    wait_http_ready "原项目 frontend" "http://127.0.0.1:$MAIN_FRONTEND_PORT" "$frontend_timeout"
}

print_runtime_info() {
  echo "[OK] 全部服务已在后台运行"
  echo "  原项目 backend pid: $(cat "$MAIN_BACKEND_PID_FILE")"
  echo "  原项目 frontend pid: $(cat "$MAIN_FRONTEND_PID_FILE")"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo
  echo "  原项目 frontend url: http://localhost:$MAIN_FRONTEND_PORT"
  echo "  原项目 backend url: http://localhost:$MAIN_BACKEND_PORT"
}

status() {
  local main_backend_pid main_frontend_pid
  main_backend_pid="$(cat "$MAIN_BACKEND_PID_FILE" 2>/dev/null || true)"
  main_frontend_pid="$(cat "$MAIN_FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  原项目 backend pid: ${main_backend_pid:-N/A}"
  echo "  原项目 frontend pid: ${main_frontend_pid:-N/A}"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart|start)
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
