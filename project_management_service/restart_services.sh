#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

SCRIPT_PATH="${BASH_SOURCE[0]}"
PROJECT_SERVICE_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
ROOT_DIR="$(cd "$PROJECT_SERVICE_DIR/.." && pwd)"
BACKEND_DIR="$PROJECT_SERVICE_DIR/backend"
FRONTEND_DIR="$PROJECT_SERVICE_DIR/frontend"
DEV_MONGO_HELPER="$ROOT_DIR/scripts/dev-mongo-common.sh"

# shellcheck disable=SC1090
source "$DEV_MONGO_HELPER"

load_optional_env() {
  local env_file="$1"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

load_optional_env "$ROOT_DIR/.env"
load_optional_env "$PROJECT_SERVICE_DIR/.env"
load_optional_env "$BACKEND_DIR/.env"

PROJECT_SERVICE_HOST="${PROJECT_SERVICE_HOST:-127.0.0.1}"
PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
PROJECT_SERVICE_FRONTEND_PORT="${PROJECT_SERVICE_FRONTEND_PORT:-39211}"
DEV_MONGO_HOST="${DEV_MONGO_HOST:-127.0.0.1}"
DEV_MONGO_PORT="${DEV_MONGO_PORT:-27018}"
DEV_MONGO_CONTAINER_NAME="${DEV_MONGO_CONTAINER_NAME:-chatos-dev-mongo}"
PROJECT_SERVICE_START_DEV_MONGO="${PROJECT_SERVICE_START_DEV_MONGO:-${START_DEV_MONGO:-auto}}"
PROJECT_SERVICE_DEV_MONGO_HOST="${PROJECT_SERVICE_DEV_MONGO_HOST:-$DEV_MONGO_HOST}"
PROJECT_SERVICE_DEV_MONGO_PORT="${PROJECT_SERVICE_DEV_MONGO_PORT:-$DEV_MONGO_PORT}"
PROJECT_SERVICE_DEV_MONGO_CONTAINER_NAME="${PROJECT_SERVICE_DEV_MONGO_CONTAINER_NAME:-$DEV_MONGO_CONTAINER_NAME}"
PROJECT_SERVICE_DEV_MONGO_CLIENT_HOST="$(dev_mongo_client_host "$PROJECT_SERVICE_DEV_MONGO_HOST")"
PROJECT_SERVICE_MONGODB_DATABASE="${PROJECT_SERVICE_MONGODB_DATABASE:-project_management_service}"
PROJECT_SERVICE_DATABASE_URL="${PROJECT_SERVICE_DATABASE_URL:-mongodb://admin:admin@${PROJECT_SERVICE_DEV_MONGO_CLIENT_HOST}:${PROJECT_SERVICE_DEV_MONGO_PORT}/${PROJECT_SERVICE_MONGODB_DATABASE}?authSource=admin}"
PROJECT_SERVICE_USER_SERVICE_BASE_URL="${PROJECT_SERVICE_USER_SERVICE_BASE_URL:-${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-http://127.0.0.1:39190}}}"
PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS="${PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS:-${CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS:-${USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS:-5000}}}"
PROJECT_SERVICE_TASK_RUNNER_BASE_URL="${PROJECT_SERVICE_TASK_RUNNER_BASE_URL:-${TASK_RUNNER_BASE_URL:-${CHATOS_TASK_RUNNER_BASE_URL:-http://127.0.0.1:39090}}}"
PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS="${PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS:-10000}"
PROJECT_SERVICE_SYNC_SECRET="${PROJECT_SERVICE_SYNC_SECRET:-${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}}"
PROJECT_SERVICE_STOP_BY_PORT="${PROJECT_SERVICE_STOP_BY_PORT:-1}"
PROJECT_SERVICE_VITE_API_BASE_URL="${PROJECT_SERVICE_VITE_API_BASE_URL:-http://127.0.0.1:${PROJECT_SERVICE_PORT}}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:project-management" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:project-management" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="projectmanagement"
fi

PROJECT_SERVICE_RUNTIME_DIR="${PROJECT_SERVICE_RUNTIME_DIR:-/tmp/chatos_rs_project_management_${ROOT_HASH}}"
BACKEND_PID_FILE="$PROJECT_SERVICE_RUNTIME_DIR/backend.pid"
FRONTEND_PID_FILE="$PROJECT_SERVICE_RUNTIME_DIR/frontend.pid"
BACKEND_LOG_FILE="$PROJECT_SERVICE_RUNTIME_DIR/backend.log"
FRONTEND_LOG_FILE="$PROJECT_SERVICE_RUNTIME_DIR/frontend.log"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $cmd"
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
    echo "[INFO] stopping $name (pid=$pid)"
    kill_pid_or_group "$pid"
    sleep 1
    if kill -0 "$pid" >/dev/null 2>&1; then
      force_kill_pid_or_group "$pid"
    fi
  fi
  rm -f "$pid_file"
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

stop_from_port() {
  local name="$1"
  local port="$2"

  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      echo "[INFO] stopping $name processes on port $port: $pids"
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
      echo "[INFO] stopping $name process on port $port"
      fuser -k -n tcp "$port" >/dev/null 2>&1 || true
    fi
  fi
}

ensure_port_available() {
  local name="$1"
  local port="$2"
  if is_port_listening "$port"; then
    echo "[ERROR] $name port is already in use: $port"
    if command -v lsof >/dev/null 2>&1; then
      lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
    fi
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
  echo "[INFO] starting $name..."
  : >"$log_file"
  if command -v setsid >/dev/null 2>&1; then
    nohup setsid bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
  else
    nohup bash -lc "$command" >"$log_file" 2>&1 < /dev/null &
  fi
  echo $! >"$pid_file"
}

check_alive() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "[ERROR] $name failed to start, inspect $log_file"
    tail -n 80 "$log_file" 2>/dev/null || true
    return 1
  fi
}

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-45}"

  if ! command -v curl >/dev/null 2>&1; then
    echo "[WARN] curl not found, skip healthcheck: $name $url"
    return 0
  fi

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while true; do
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
      echo "[INFO] $name is ready: $url"
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name healthcheck timed out after ${timeout_sec}s: $url"
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
      echo "[ERROR] $name port was not released in time: $port"
      return 1
    fi
    sleep 1
  done
}

prepare() {
  local cmd="${1:-restart}"

  need_cmd bash
  if [[ ! -d "$BACKEND_DIR" || ! -d "$FRONTEND_DIR" ]]; then
    echo "[ERROR] project_management_service directory is incomplete"
    exit 1
  fi
  mkdir -p "$PROJECT_SERVICE_RUNTIME_DIR"

  case "$cmd" in
    restart|start)
      need_cmd cargo
      need_cmd npm
      ;;
  esac
}

ensure_project_service_database() {
  case "$PROJECT_SERVICE_DATABASE_URL" in
    mongodb://*|mongodb+srv://*)
      ensure_dev_mongo_service \
        "$PROJECT_SERVICE_START_DEV_MONGO" \
        "$PROJECT_SERVICE_DEV_MONGO_HOST" \
        "$PROJECT_SERVICE_DEV_MONGO_PORT" \
        "$PROJECT_SERVICE_DEV_MONGO_CONTAINER_NAME"
      ;;
    sqlite:*)
      echo "[WARN] project_management_service is using SQLite fallback: $PROJECT_SERVICE_DATABASE_URL"
      ;;
    *)
      echo "[ERROR] unsupported PROJECT_SERVICE_DATABASE_URL: $PROJECT_SERVICE_DATABASE_URL"
      return 1
      ;;
  esac
}

ensure_frontend_deps() {
  if [[ -x "$FRONTEND_DIR/node_modules/.bin/vite" ]]; then
    return
  fi

  echo "[INFO] installing project_management_service frontend dependencies..."
  (
    cd "$FRONTEND_DIR"
    if [[ -f package-lock.json ]]; then
      npm ci
    else
      npm install
    fi
  )
}

start_backend() {
  ensure_project_service_database

  launch_service \
    "project_management_service backend" \
    "$PROJECT_SERVICE_PORT" \
    "$BACKEND_PID_FILE" \
    "$BACKEND_LOG_FILE" \
    "cd \"$ROOT_DIR\" && PROJECT_SERVICE_HOST=\"$PROJECT_SERVICE_HOST\" PROJECT_SERVICE_PORT=\"$PROJECT_SERVICE_PORT\" PROJECT_SERVICE_DATABASE_URL=\"$PROJECT_SERVICE_DATABASE_URL\" PROJECT_SERVICE_MONGODB_DATABASE=\"$PROJECT_SERVICE_MONGODB_DATABASE\" PROJECT_SERVICE_USER_SERVICE_BASE_URL=\"$PROJECT_SERVICE_USER_SERVICE_BASE_URL\" PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS=\"$PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS\" PROJECT_SERVICE_TASK_RUNNER_BASE_URL=\"$PROJECT_SERVICE_TASK_RUNNER_BASE_URL\" PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS=\"$PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS\" PROJECT_SERVICE_SYNC_SECRET=\"$PROJECT_SERVICE_SYNC_SECRET\" exec cargo run -p project_management_service_backend"
}

start_frontend() {
  ensure_frontend_deps

  launch_service \
    "project_management_service frontend" \
    "$PROJECT_SERVICE_FRONTEND_PORT" \
    "$FRONTEND_PID_FILE" \
    "$FRONTEND_LOG_FILE" \
    "cd \"$FRONTEND_DIR\" && VITE_API_BASE_URL=\"$PROJECT_SERVICE_VITE_API_BASE_URL\" exec npm run dev -- --host 0.0.0.0 --port \"$PROJECT_SERVICE_FRONTEND_PORT\""
}

do_stop() {
  stop_from_pid_file "project_management_service backend" "$BACKEND_PID_FILE"
  stop_from_pid_file "project_management_service frontend" "$FRONTEND_PID_FILE"

  if [[ "$PROJECT_SERVICE_STOP_BY_PORT" == "1" ]]; then
    stop_from_port "project_management_service backend" "$PROJECT_SERVICE_PORT"
    stop_from_port "project_management_service frontend" "$PROJECT_SERVICE_FRONTEND_PORT"
  fi

  wait_port_released "project_management_service backend" "$PROJECT_SERVICE_PORT" || return 1
  wait_port_released "project_management_service frontend" "$PROJECT_SERVICE_FRONTEND_PORT" || return 1
}

run_start_sequence() {
  local backend_timeout="${PROJECT_SERVICE_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-120}}"
  local frontend_timeout="${PROJECT_SERVICE_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}}"

  start_backend &&
    start_frontend

  sleep 2 &&
    check_alive "project_management_service backend" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE" &&
    check_alive "project_management_service frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE" &&
    wait_http_ready "project_management_service backend" "http://127.0.0.1:${PROJECT_SERVICE_PORT}/api/health" "$backend_timeout" &&
    wait_http_ready "project_management_service frontend" "http://127.0.0.1:${PROJECT_SERVICE_FRONTEND_PORT}" "$frontend_timeout"
}

print_runtime_info() {
  echo "[OK] project_management_service is running"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE")"
  echo "  frontend pid: $(cat "$FRONTEND_PID_FILE")"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  backend url: http://localhost:$PROJECT_SERVICE_PORT"
  echo "  frontend url: http://localhost:$PROJECT_SERVICE_FRONTEND_PORT"
  echo "  database url: $PROJECT_SERVICE_DATABASE_URL"
  echo "  user_service url: $PROJECT_SERVICE_USER_SERVICE_BASE_URL"
  echo "  task_runner url: $PROJECT_SERVICE_TASK_RUNNER_BASE_URL"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $PROJECT_SERVICE_RUNTIME_DIR"
  echo "  backend pid: ${backend_pid:-N/A}"
  echo "  frontend pid: ${frontend_pid:-N/A}"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  backend url: http://localhost:$PROJECT_SERVICE_PORT"
  echo "  frontend url: http://localhost:$PROJECT_SERVICE_FRONTEND_PORT"
  echo "  database url: $PROJECT_SERVICE_DATABASE_URL"
}

CMD="${1:-restart}"
prepare "$CMD"

case "$CMD" in
  restart|start)
    do_stop
    if run_start_sequence; then
      print_runtime_info
    else
      echo "[WARN] startup failed, cleaning up..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] project_management_service stopped"
    ;;
  status)
    status
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
