#!/usr/bin/env bash
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

SCRIPT_PATH="${BASH_SOURCE[0]}"
USER_SERVICE_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
ROOT_DIR="$(cd "$USER_SERVICE_DIR/.." && pwd)"
BACKEND_DIR="$USER_SERVICE_DIR/backend"
FRONTEND_DIR="$USER_SERVICE_DIR/frontend"
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
load_optional_env "$USER_SERVICE_DIR/.env"
load_optional_env "$BACKEND_DIR/.env"

USER_SERVICE_HOST="${USER_SERVICE_HOST:-127.0.0.1}"
USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
USER_SERVICE_FRONTEND_PORT="${USER_SERVICE_FRONTEND_PORT:-39191}"
DEV_MONGO_HOST="${DEV_MONGO_HOST:-127.0.0.1}"
DEV_MONGO_PORT="${DEV_MONGO_PORT:-27018}"
DEV_MONGO_CONTAINER_NAME="${DEV_MONGO_CONTAINER_NAME:-chatos-dev-mongo}"
USER_SERVICE_START_DEV_MONGO="${USER_SERVICE_START_DEV_MONGO:-${START_DEV_MONGO:-auto}}"
USER_SERVICE_DEV_MONGO_HOST="${USER_SERVICE_DEV_MONGO_HOST:-$DEV_MONGO_HOST}"
USER_SERVICE_DEV_MONGO_PORT="${USER_SERVICE_DEV_MONGO_PORT:-$DEV_MONGO_PORT}"
USER_SERVICE_DEV_MONGO_CONTAINER_NAME="${USER_SERVICE_DEV_MONGO_CONTAINER_NAME:-$DEV_MONGO_CONTAINER_NAME}"
USER_SERVICE_DEV_MONGO_CLIENT_HOST="$(dev_mongo_client_host "$USER_SERVICE_DEV_MONGO_HOST")"
USER_SERVICE_MONGODB_DATABASE="${USER_SERVICE_MONGODB_DATABASE:-user_service}"
USER_SERVICE_DATABASE_URL="${USER_SERVICE_DATABASE_URL:-mongodb://admin:admin@${USER_SERVICE_DEV_MONGO_CLIENT_HOST}:${USER_SERVICE_DEV_MONGO_PORT}/${USER_SERVICE_MONGODB_DATABASE}?authSource=admin}"
USER_SERVICE_API_PROXY_TARGET="${USER_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${USER_SERVICE_PORT}}"
USER_SERVICE_STOP_BY_PORT="${USER_SERVICE_STOP_BY_PORT:-1}"
TASK_RUNNER_CALLBACK_SECRET_DEFAULT="${TASK_RUNNER_CALLBACK_SECRET_DEFAULT:-chatos-task-runner-dev-secret}"
MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT="${MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT:-chatos-memory-engine-dev-operator-token}"
USER_SERVICE_MEMORY_ENGINE_HOST="${MEMORY_ENGINE_HOST:-127.0.0.1}"
if [[ "$USER_SERVICE_MEMORY_ENGINE_HOST" == "0.0.0.0" || "$USER_SERVICE_MEMORY_ENGINE_HOST" == "::" || "$USER_SERVICE_MEMORY_ENGINE_HOST" == "[::]" ]]; then
  USER_SERVICE_MEMORY_ENGINE_HOST="127.0.0.1"
fi
USER_SERVICE_MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
USER_SERVICE_MEMORY_ENGINE_BASE_URL="${MEMORY_ENGINE_BASE_URL:-http://${USER_SERVICE_MEMORY_ENGINE_HOST}:${USER_SERVICE_MEMORY_ENGINE_PORT}/api/memory-engine/v1}"
USER_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-$MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT}"
USER_SERVICE_TASK_RUNNER_BASE_URL="${TASK_RUNNER_BASE_URL:-${CHATOS_TASK_RUNNER_BASE_URL:-http://127.0.0.1:39090}}"
USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-$TASK_RUNNER_CALLBACK_SECRET_DEFAULT}}"
USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS="${USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS:-5000}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="default"
fi

USER_SERVICE_RUNTIME_DIR="${USER_SERVICE_RUNTIME_DIR:-/tmp/chatos_rs_user_service_${ROOT_HASH}}"
BACKEND_PID_FILE="$USER_SERVICE_RUNTIME_DIR/backend.pid"
FRONTEND_PID_FILE="$USER_SERVICE_RUNTIME_DIR/frontend.pid"
BACKEND_LOG_FILE="$USER_SERVICE_RUNTIME_DIR/backend.log"
FRONTEND_LOG_FILE="$USER_SERVICE_RUNTIME_DIR/frontend.log"

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
    echo "[ERROR] user_service directory is incomplete"
    exit 1
  fi

  mkdir -p "$USER_SERVICE_RUNTIME_DIR"

  case "$cmd" in
    restart|start)
      need_cmd cargo
      need_cmd npm
      ;;
  esac
}

ensure_user_service_mongo() {
  case "$USER_SERVICE_DATABASE_URL" in
    mongodb://*|mongodb+srv://*)
      ensure_dev_mongo_service \
        "$USER_SERVICE_START_DEV_MONGO" \
        "$USER_SERVICE_DEV_MONGO_HOST" \
        "$USER_SERVICE_DEV_MONGO_PORT" \
        "$USER_SERVICE_DEV_MONGO_CONTAINER_NAME"
      ;;
    *)
      echo "[ERROR] user_service requires a MongoDB USER_SERVICE_DATABASE_URL, got: $USER_SERVICE_DATABASE_URL"
      return 1
      ;;
  esac
}

start_backend() {
  ensure_user_service_mongo

  launch_service \
    "user_service backend" \
    "$USER_SERVICE_PORT" \
    "$BACKEND_PID_FILE" \
    "$BACKEND_LOG_FILE" \
    "cd \"$BACKEND_DIR\" && USER_SERVICE_HOST=\"$USER_SERVICE_HOST\" USER_SERVICE_PORT=\"$USER_SERVICE_PORT\" USER_SERVICE_DATABASE_URL=\"$USER_SERVICE_DATABASE_URL\" USER_SERVICE_MONGODB_DATABASE=\"$USER_SERVICE_MONGODB_DATABASE\" MEMORY_ENGINE_BASE_URL=\"$USER_SERVICE_MEMORY_ENGINE_BASE_URL\" MEMORY_ENGINE_OPERATOR_TOKEN=\"$USER_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN\" TASK_RUNNER_BASE_URL=\"$USER_SERVICE_TASK_RUNNER_BASE_URL\" CHATOS_TASK_RUNNER_BASE_URL=\"$USER_SERVICE_TASK_RUNNER_BASE_URL\" TASK_RUNNER_CHATOS_CALLBACK_SECRET=\"$USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET\" CHATOS_TASK_RUNNER_CALLBACK_SECRET=\"$USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET\" USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS=\"$USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS\" exec cargo run"
}

start_frontend() {
  launch_service \
    "user_service frontend" \
    "$USER_SERVICE_FRONTEND_PORT" \
    "$FRONTEND_PID_FILE" \
    "$FRONTEND_LOG_FILE" \
    "cd \"$FRONTEND_DIR\" && USER_SERVICE_FRONTEND_PORT=\"$USER_SERVICE_FRONTEND_PORT\" USER_SERVICE_API_PROXY_TARGET=\"$USER_SERVICE_API_PROXY_TARGET\" exec npm run dev -- --host 0.0.0.0 --port \"$USER_SERVICE_FRONTEND_PORT\""
}

do_stop() {
  stop_from_pid_file "user_service backend" "$BACKEND_PID_FILE"
  stop_from_pid_file "user_service frontend" "$FRONTEND_PID_FILE"

  if [[ "$USER_SERVICE_STOP_BY_PORT" == "1" ]]; then
    stop_from_port "user_service backend" "$USER_SERVICE_PORT"
    stop_from_port "user_service frontend" "$USER_SERVICE_FRONTEND_PORT"
  fi

  wait_port_released "user_service backend" "$USER_SERVICE_PORT" || return 1
  wait_port_released "user_service frontend" "$USER_SERVICE_FRONTEND_PORT" || return 1
}

run_start_sequence() {
  local backend_timeout="${USER_SERVICE_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-120}}"
  local frontend_timeout="${USER_SERVICE_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}}"

  start_backend &&
    start_frontend

  sleep 2 &&
    check_alive "user_service backend" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE" &&
    check_alive "user_service frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE" &&
    wait_http_ready "user_service backend" "http://127.0.0.1:${USER_SERVICE_PORT}/api/health" "$backend_timeout" &&
    wait_http_ready "user_service frontend" "http://127.0.0.1:${USER_SERVICE_FRONTEND_PORT}" "$frontend_timeout"
}

print_runtime_info() {
  echo "[OK] user_service is running"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE")"
  echo "  frontend pid: $(cat "$FRONTEND_PID_FILE")"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  backend url: http://localhost:$USER_SERVICE_PORT"
  echo "  frontend url: http://localhost:$USER_SERVICE_FRONTEND_PORT"
  echo "  mongodb database: $USER_SERVICE_MONGODB_DATABASE"
  echo "  mongodb url: $USER_SERVICE_DATABASE_URL"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $USER_SERVICE_RUNTIME_DIR"
  echo "  backend pid: ${backend_pid:-N/A}"
  echo "  frontend pid: ${frontend_pid:-N/A}"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo
  echo "  mongodb database: $USER_SERVICE_MONGODB_DATABASE"
  echo "  mongodb url: $USER_SERVICE_DATABASE_URL"
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
    echo "[OK] user_service stopped"
    ;;
  status)
    status
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
