#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
DEV_MONGO_HELPER="$ROOT_DIR/scripts/dev-mongo-common.sh"
LOCAL_SERVICE_LAUNCHER="$ROOT_DIR/scripts/local-service-launcher.sh"

# shellcheck disable=SC1090
source "$DEV_MONGO_HELPER"
# shellcheck disable=SC1090
source "$LOCAL_SERVICE_LAUNCHER"

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

TASK_RUNNER_ROOT_DIR="$ROOT_DIR/task_runner_service"
TASK_RUNNER_BACKEND_DIR="$TASK_RUNNER_ROOT_DIR/backend"
TASK_RUNNER_FRONTEND_DIR="$TASK_RUNNER_ROOT_DIR/frontend"

load_optional_env "$ROOT_DIR/.env"
load_optional_env "$ROOT_DIR/project_management_service/.env"
load_optional_env "$ROOT_DIR/project_management_service/backend/.env"
load_optional_env "$TASK_RUNNER_ROOT_DIR/.env"
load_optional_env "$TASK_RUNNER_BACKEND_DIR/.env"

DEV_MONGO_HOST="${DEV_MONGO_HOST:-127.0.0.1}"
DEV_MONGO_PORT="${DEV_MONGO_PORT:-27018}"
DEV_MONGO_CONTAINER_NAME="${DEV_MONGO_CONTAINER_NAME:-chatos-dev-mongo}"
TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}"
TASK_RUNNER_FRONTEND_PORT="${TASK_RUNNER_FRONTEND_PORT:-39091}"
TASK_RUNNER_HEALTHCHECK_HOST="${TASK_RUNNER_HEALTHCHECK_HOST:-127.0.0.1}"
TASK_RUNNER_VITE_API_BASE_URL="${TASK_RUNNER_VITE_API_BASE_URL:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
TASK_RUNNER_API_PROXY_TARGET="${TASK_RUNNER_API_PROXY_TARGET:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
TASK_RUNNER_STORE_MODE="${TASK_RUNNER_STORE_MODE:-mongo}"
TASK_RUNNER_START_DEV_MONGO="${TASK_RUNNER_START_DEV_MONGO:-${START_DEV_MONGO:-auto}}"
TASK_RUNNER_DEV_MONGO_HOST="${TASK_RUNNER_DEV_MONGO_HOST:-$DEV_MONGO_HOST}"
TASK_RUNNER_DEV_MONGO_PORT="${TASK_RUNNER_DEV_MONGO_PORT:-$DEV_MONGO_PORT}"
TASK_RUNNER_DEV_MONGO_CONTAINER_NAME="${TASK_RUNNER_DEV_MONGO_CONTAINER_NAME:-$DEV_MONGO_CONTAINER_NAME}"
TASK_RUNNER_DEV_MONGO_CLIENT_HOST="$(dev_mongo_client_host "$TASK_RUNNER_DEV_MONGO_HOST")"
TASK_RUNNER_MONGODB_DATABASE="${TASK_RUNNER_MONGODB_DATABASE:-task_runner_service}"
TASK_RUNNER_DATABASE_URL="${TASK_RUNNER_DATABASE_URL:-mongodb://admin:admin@${TASK_RUNNER_DEV_MONGO_CLIENT_HOST}:${TASK_RUNNER_DEV_MONGO_PORT}/${TASK_RUNNER_MONGODB_DATABASE}?authSource=admin}"
TASK_RUNNER_STOP_BY_PORT="${TASK_RUNNER_STOP_BY_PORT:-1}"
TASK_RUNNER_CHATOS_BACKEND_PORT="${TASK_RUNNER_CHATOS_BACKEND_PORT:-${MAIN_BACKEND_PORT:-${BACKEND_PORT:-3997}}}"
TASK_RUNNER_CALLBACK_SECRET_DEFAULT="${TASK_RUNNER_CALLBACK_SECRET_DEFAULT:-chatos-task-runner-dev-secret}"
TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-$TASK_RUNNER_CALLBACK_SECRET_DEFAULT}}"
TASK_RUNNER_CHATOS_CALLBACK_URL="${TASK_RUNNER_CHATOS_CALLBACK_URL:-http://127.0.0.1:${TASK_RUNNER_CHATOS_BACKEND_PORT}/api/agent/chat/task-runner/callback}"
PROJECT_SERVICE_HOST_EFFECTIVE="${PROJECT_SERVICE_HOST:-127.0.0.1}"
if [[ "$PROJECT_SERVICE_HOST_EFFECTIVE" == "0.0.0.0" || "$PROJECT_SERVICE_HOST_EFFECTIVE" == "::" || "$PROJECT_SERVICE_HOST_EFFECTIVE" == "[::]" ]]; then
  PROJECT_SERVICE_HOST_EFFECTIVE="127.0.0.1"
fi
PROJECT_SERVICE_PORT_EFFECTIVE="${PROJECT_SERVICE_PORT:-39210}"
TASK_RUNNER_PROJECT_SERVICE_BASE_URL_EFFECTIVE="${TASK_RUNNER_PROJECT_SERVICE_BASE_URL:-${PROJECT_SERVICE_BASE_URL:-${CHATOS_PROJECT_SERVICE_BASE_URL:-http://${PROJECT_SERVICE_HOST_EFFECTIVE}:${PROJECT_SERVICE_PORT_EFFECTIVE}}}}"
PROJECT_SERVICE_SYNC_SECRET_EFFECTIVE="${TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET:-${PROJECT_SERVICE_SYNC_SECRET:-${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}}}"
TASK_RUNNER_USER_SERVICE_BASE_URL_EFFECTIVE="${TASK_RUNNER_USER_SERVICE_BASE_URL:-${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-http://127.0.0.1:39190}}}"
TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_EFFECTIVE="${TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS:-${CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS:-${USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS:-5000}}}"
TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN_EFFECTIVE="${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-${MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-dev-operator-token}}"
TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE="${TASK_RUNNER_MEMORY_ENGINE_HOST:-${MEMORY_ENGINE_HOST:-127.0.0.1}}"
if [[ "$TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE" == "0.0.0.0" || "$TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE" == "::" || "$TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE" == "[::]" ]]; then
  TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE="127.0.0.1"
fi
TASK_RUNNER_MEMORY_ENGINE_PORT_EFFECTIVE="${TASK_RUNNER_MEMORY_ENGINE_PORT:-${MEMORY_ENGINE_PORT:-7081}}"
TASK_RUNNER_MEMORY_ENGINE_BASE_URL_EFFECTIVE="${TASK_RUNNER_MEMORY_ENGINE_BASE_URL:-${MEMORY_ENGINE_BASE_URL:-http://${TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE}:${TASK_RUNNER_MEMORY_ENGINE_PORT_EFFECTIVE}/api/memory-engine/v1}}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:task-runner" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:task-runner" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="taskrunner"
fi

TASK_RUNNER_RUNTIME_DIR="${TASK_RUNNER_RUNTIME_DIR:-/tmp/chatos_rs_task_runner_${ROOT_HASH}}"
LOCAL_SERVICE_LAUNCHD_PREFIX="${LOCAL_SERVICE_LAUNCHD_PREFIX:-chatos-rs-task-runner-${ROOT_HASH}}"

TASK_RUNNER_BACKEND_PID_FILE="$TASK_RUNNER_RUNTIME_DIR/backend.pid"
TASK_RUNNER_FRONTEND_PID_FILE="$TASK_RUNNER_RUNTIME_DIR/frontend.pid"
TASK_RUNNER_BACKEND_LOG_FILE="$TASK_RUNNER_RUNTIME_DIR/backend.log"
TASK_RUNNER_FRONTEND_LOG_FILE="$TASK_RUNNER_RUNTIME_DIR/frontend.log"
TASK_RUNNER_TARGET_DIR="${TASK_RUNNER_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-$TASK_RUNNER_ROOT_DIR/target}}"
if [[ "$TASK_RUNNER_TARGET_DIR" != /* ]]; then
  TASK_RUNNER_TARGET_DIR="$ROOT_DIR/$TASK_RUNNER_TARGET_DIR"
fi
TASK_RUNNER_BACKEND_BINARY="$TASK_RUNNER_TARGET_DIR/debug/task_runner_service_backend"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

kill_pid_or_group() {
  local pid="$1"
  if [[ -z "$pid" ]]; then
    return 0
  fi
  if kill -0 -- "-$pid" >/dev/null 2>&1; then
    kill -- "-$pid" >/dev/null 2>&1 || true
  else
    kill "$pid" >/dev/null 2>&1 || true
  fi
}

force_kill_pid_or_group() {
  local pid="$1"
  if [[ -z "$pid" ]]; then
    return 0
  fi
  if kill -0 -- "-$pid" >/dev/null 2>&1; then
    kill -9 -- "-$pid" >/dev/null 2>&1 || true
  else
    kill -9 "$pid" >/dev/null 2>&1 || true
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
    kill_pid_or_group "$pid"
    sleep 1
    if kill -0 "$pid" >/dev/null 2>&1; then
      force_kill_pid_or_group "$pid"
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
    echo "[HINT] 请改用其它端口（例如 TASK_RUNNER_BACKEND_PORT / TASK_RUNNER_FRONTEND_PORT），或先停止占用该端口的服务。"
    return 1
  fi
}

launch_service() {
  local name="$1"
  local port="$2"
  local pid_file="$3"
  local log_file="$4"
  local command="$5"
  local launchd_label
  launchd_label="$(local_service_launchd_label "$LOCAL_SERVICE_LAUNCHD_PREFIX" "$name")"

  if local_service_use_launchd; then
    local_service_stop_launchd_job "$launchd_label"
  fi
  ensure_port_available "$name" "$port" || return 1
  echo "[INFO] 启动 $name..."
  : >"$log_file"
  if local_service_use_launchd; then
    local_service_launch_with_launchd "$launchd_label" "$name" "$log_file" "$pid_file" "$command"
  elif command -v setsid >/dev/null 2>&1; then
    nohup setsid bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
    echo $! >"$pid_file"
  else
    nohup bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
    echo $! >"$pid_file"
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
    tail -n 60 "$log_file" 2>/dev/null || true
    return 1
  fi
}

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-30}"
  local pid_file="${4:-}"
  local log_file="${5:-}"

  if ! command -v curl >/dev/null 2>&1; then
    echo "[WARN] 未找到 curl，跳过 $name 健康检查: $url"
    return 0
  fi

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while true; do
    if [[ -n "$pid_file" ]]; then
      local pid
      pid="$(cat "$pid_file" 2>/dev/null || true)"
      if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
        echo "[ERROR] $name 在健康检查完成前已退出"
        if [[ -n "$log_file" ]]; then
          echo "[INFO] 最近日志: $log_file"
          tail -n 80 "$log_file" 2>/dev/null || true
        fi
        return 1
      fi
    fi

    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
      echo "[INFO] $name 健康检查通过: $url"
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name 健康检查超时 (${timeout_sec}s): $url"
      if [[ -n "$log_file" ]]; then
        echo "[INFO] 最近日志: $log_file"
        tail -n 80 "$log_file" 2>/dev/null || true
      fi
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

ensure_sqlite_parent_dir() {
  local database_url="$1"
  if [[ "$database_url" != sqlite://* ]]; then
    return
  fi

  local db_path
  db_path="${database_url#sqlite://}"
  db_path="${db_path%%\?*}"
  if [[ -z "$db_path" || "$db_path" == ":memory:" ]]; then
    return
  fi

  if [[ "$db_path" == /* ]]; then
    mkdir -p "$(dirname "$db_path")"
  else
    mkdir -p "$ROOT_DIR/$(dirname "$db_path")"
  fi
}

ensure_task_runner_dev_mongo() {
  local store_mode
  store_mode="$(printf '%s' "$TASK_RUNNER_STORE_MODE" | tr '[:upper:]' '[:lower:]')"
  if [[ "$store_mode" != "mongo" && "$store_mode" != "mongodb" ]]; then
    return 0
  fi

  ensure_dev_mongo_service \
    "$TASK_RUNNER_START_DEV_MONGO" \
    "$TASK_RUNNER_DEV_MONGO_HOST" \
    "$TASK_RUNNER_DEV_MONGO_PORT" \
    "$TASK_RUNNER_DEV_MONGO_CONTAINER_NAME"
}

prepare() {
  local cmd="${1:-restart}"

  need_cmd bash

  if [[ ! -d "$TASK_RUNNER_BACKEND_DIR" || ! -d "$TASK_RUNNER_FRONTEND_DIR" ]]; then
    echo "[ERROR] 项目目录不完整: $TASK_RUNNER_BACKEND_DIR / $TASK_RUNNER_FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$TASK_RUNNER_RUNTIME_DIR"

  case "$cmd" in
    restart|start)
      need_cmd npm
      need_cmd cargo
      ensure_sqlite_parent_dir "$TASK_RUNNER_DATABASE_URL"
      ;;
  esac
}

build_task_runner_backend() {
  echo "[INFO] 构建 Task Runner backend..."
  cargo build \
    -p task_runner_service_backend \
    --bin task_runner_service_backend \
    --target-dir "$TASK_RUNNER_TARGET_DIR"
}

start_task_runner_backend() {
  launch_service \
    "Task Runner backend" \
    "$TASK_RUNNER_BACKEND_PORT" \
    "$TASK_RUNNER_BACKEND_PID_FILE" \
    "$TASK_RUNNER_BACKEND_LOG_FILE" \
    "cd \"$ROOT_DIR\" && TASK_RUNNER_STORE_MODE=\"\${TASK_RUNNER_STORE_MODE:-$TASK_RUNNER_STORE_MODE}\" TASK_RUNNER_HOST=\"\${TASK_RUNNER_HOST:-127.0.0.1}\" TASK_RUNNER_PORT=\"$TASK_RUNNER_BACKEND_PORT\" TASK_RUNNER_WORKSPACE_DIR=\"\${TASK_RUNNER_WORKSPACE_DIR:-$ROOT_DIR}\" TASK_RUNNER_DATABASE_URL=\"\${TASK_RUNNER_DATABASE_URL:-$TASK_RUNNER_DATABASE_URL}\" TASK_RUNNER_PROJECT_SERVICE_BASE_URL=\"\${TASK_RUNNER_PROJECT_SERVICE_BASE_URL:-$TASK_RUNNER_PROJECT_SERVICE_BASE_URL_EFFECTIVE}\" PROJECT_SERVICE_BASE_URL=\"\${PROJECT_SERVICE_BASE_URL:-$TASK_RUNNER_PROJECT_SERVICE_BASE_URL_EFFECTIVE}\" TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET=\"\${TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET:-$PROJECT_SERVICE_SYNC_SECRET_EFFECTIVE}\" PROJECT_SERVICE_SYNC_SECRET=\"\${PROJECT_SERVICE_SYNC_SECRET:-$PROJECT_SERVICE_SYNC_SECRET_EFFECTIVE}\" TASK_RUNNER_MEMORY_ENGINE_BASE_URL=\"\${TASK_RUNNER_MEMORY_ENGINE_BASE_URL:-$TASK_RUNNER_MEMORY_ENGINE_BASE_URL_EFFECTIVE}\" MEMORY_ENGINE_BASE_URL=\"\${MEMORY_ENGINE_BASE_URL:-$TASK_RUNNER_MEMORY_ENGINE_BASE_URL_EFFECTIVE}\" MEMORY_ENGINE_HOST=\"\${MEMORY_ENGINE_HOST:-$TASK_RUNNER_MEMORY_ENGINE_HOST_EFFECTIVE}\" MEMORY_ENGINE_PORT=\"\${MEMORY_ENGINE_PORT:-$TASK_RUNNER_MEMORY_ENGINE_PORT_EFFECTIVE}\" TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN=\"\${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-$TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN_EFFECTIVE}\" MEMORY_ENGINE_OPERATOR_TOKEN=\"\${MEMORY_ENGINE_OPERATOR_TOKEN:-$TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN_EFFECTIVE}\" TASK_RUNNER_CHATOS_CALLBACK_URL=\"\${TASK_RUNNER_CHATOS_CALLBACK_URL:-$TASK_RUNNER_CHATOS_CALLBACK_URL}\" TASK_RUNNER_CHATOS_CALLBACK_SECRET=\"\${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-$TASK_RUNNER_CHATOS_CALLBACK_SECRET}\" CHATOS_TASK_RUNNER_CALLBACK_SECRET=\"\${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-$TASK_RUNNER_CHATOS_CALLBACK_SECRET}\" TASK_RUNNER_USER_SERVICE_BASE_URL=\"\${TASK_RUNNER_USER_SERVICE_BASE_URL:-$TASK_RUNNER_USER_SERVICE_BASE_URL_EFFECTIVE}\" TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS=\"\${TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS:-$TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_EFFECTIVE}\" exec \"$TASK_RUNNER_BACKEND_BINARY\""
}

start_task_runner_frontend() {
  launch_service \
    "Task Runner frontend" \
    "$TASK_RUNNER_FRONTEND_PORT" \
    "$TASK_RUNNER_FRONTEND_PID_FILE" \
    "$TASK_RUNNER_FRONTEND_LOG_FILE" \
    "cd \"$TASK_RUNNER_FRONTEND_DIR\" && TASK_RUNNER_API_PROXY_TARGET=\"$TASK_RUNNER_API_PROXY_TARGET\" VITE_API_BASE_URL=\"$TASK_RUNNER_VITE_API_BASE_URL\" exec npm run dev -- --host 0.0.0.0 --port \"$TASK_RUNNER_FRONTEND_PORT\""
}

do_stop() {
  local_service_stop_launchd_job "$(local_service_launchd_label "$LOCAL_SERVICE_LAUNCHD_PREFIX" "Task Runner backend")"
  local_service_stop_launchd_job "$(local_service_launchd_label "$LOCAL_SERVICE_LAUNCHD_PREFIX" "Task Runner frontend")"

  stop_from_pid_file "Task Runner backend" "$TASK_RUNNER_BACKEND_PID_FILE"
  stop_from_pid_file "Task Runner frontend" "$TASK_RUNNER_FRONTEND_PID_FILE"

  if [[ "$TASK_RUNNER_STOP_BY_PORT" == "1" ]]; then
    stop_from_port "Task Runner backend" "$TASK_RUNNER_BACKEND_PORT"
    stop_from_port "Task Runner frontend" "$TASK_RUNNER_FRONTEND_PORT"
  else
    echo "[INFO] 跳过按端口全局停止 (TASK_RUNNER_STOP_BY_PORT=${TASK_RUNNER_STOP_BY_PORT})，仅按 PID 文件停止，避免误伤其他项目。"
    stop_project_owned_port_processes "Task Runner backend" "$TASK_RUNNER_BACKEND_PORT"
    stop_project_owned_port_processes "Task Runner frontend" "$TASK_RUNNER_FRONTEND_PORT"
  fi

  wait_port_released "Task Runner backend" "$TASK_RUNNER_BACKEND_PORT" || return 1
  wait_port_released "Task Runner frontend" "$TASK_RUNNER_FRONTEND_PORT" || return 1
}

run_start_sequence() {
  local backend_timeout="${TASK_RUNNER_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-120}}"
  local frontend_timeout="${TASK_RUNNER_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}}"

  ensure_task_runner_dev_mongo &&
    build_task_runner_backend &&
    start_task_runner_backend &&
    start_task_runner_frontend

  sleep 2 &&
    check_alive "Task Runner backend" "$TASK_RUNNER_BACKEND_PID_FILE" "$TASK_RUNNER_BACKEND_LOG_FILE" &&
    check_alive "Task Runner frontend" "$TASK_RUNNER_FRONTEND_PID_FILE" "$TASK_RUNNER_FRONTEND_LOG_FILE" &&
    wait_http_ready "Task Runner backend" "http://${TASK_RUNNER_HEALTHCHECK_HOST}:$TASK_RUNNER_BACKEND_PORT/api/health" "$backend_timeout" "$TASK_RUNNER_BACKEND_PID_FILE" "$TASK_RUNNER_BACKEND_LOG_FILE" &&
    wait_http_ready "Task Runner frontend" "http://127.0.0.1:$TASK_RUNNER_FRONTEND_PORT" "$frontend_timeout" "$TASK_RUNNER_FRONTEND_PID_FILE" "$TASK_RUNNER_FRONTEND_LOG_FILE"
}

print_runtime_info() {
  echo "[OK] Task Runner 服务已在后台运行"
  echo "  Task Runner backend pid: $(cat "$TASK_RUNNER_BACKEND_PID_FILE")"
  echo "  Task Runner frontend pid: $(cat "$TASK_RUNNER_FRONTEND_PID_FILE")"
  echo
  echo "  runtime dir: $TASK_RUNNER_RUNTIME_DIR"
  echo "  Task Runner backend log: $TASK_RUNNER_BACKEND_LOG_FILE"
  echo "  Task Runner frontend log: $TASK_RUNNER_FRONTEND_LOG_FILE"
  echo
  echo "  Task Runner frontend url: http://localhost:$TASK_RUNNER_FRONTEND_PORT"
  echo "  Task Runner backend url: http://localhost:$TASK_RUNNER_BACKEND_PORT"
  echo "  Task Runner frontend api base url: $TASK_RUNNER_VITE_API_BASE_URL"
  echo "  Task Runner api proxy target: $TASK_RUNNER_API_PROXY_TARGET"
  echo "  Task Runner health url: http://${TASK_RUNNER_HEALTHCHECK_HOST}:$TASK_RUNNER_BACKEND_PORT/api/health"
  echo "  Task Runner callback url: $TASK_RUNNER_CHATOS_CALLBACK_URL"
  echo "  Task Runner store mode: $TASK_RUNNER_STORE_MODE"
  echo "  Task Runner database url: $TASK_RUNNER_DATABASE_URL"
  echo "  Memory Engine base url: $TASK_RUNNER_MEMORY_ENGINE_BASE_URL_EFFECTIVE"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$TASK_RUNNER_BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$TASK_RUNNER_FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $TASK_RUNNER_RUNTIME_DIR"
  echo "  Task Runner backend pid: ${backend_pid:-N/A}"
  echo "  Task Runner frontend pid: ${frontend_pid:-N/A}"
  echo
  echo "  Task Runner backend log: $TASK_RUNNER_BACKEND_LOG_FILE"
  echo "  Task Runner frontend log: $TASK_RUNNER_FRONTEND_LOG_FILE"
  echo
  echo "  Task Runner frontend url: http://localhost:$TASK_RUNNER_FRONTEND_PORT"
  echo "  Task Runner backend url: http://localhost:$TASK_RUNNER_BACKEND_PORT"
  echo "  Task Runner frontend api base url: $TASK_RUNNER_VITE_API_BASE_URL"
  echo "  Task Runner api proxy target: $TASK_RUNNER_API_PROXY_TARGET"
  echo "  Task Runner callback url: $TASK_RUNNER_CHATOS_CALLBACK_URL"
  echo "  Task Runner store mode: $TASK_RUNNER_STORE_MODE"
  echo "  Task Runner database url: $TASK_RUNNER_DATABASE_URL"
  echo "  Memory Engine base url: $TASK_RUNNER_MEMORY_ENGINE_BASE_URL_EFFECTIVE"
}

CMD="${1:-restart}"
prepare "$CMD"

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
    echo "[OK] Task Runner 服务已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
