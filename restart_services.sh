#!/usr/bin/env bash
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

SCRIPT_PATH="${BASH_SOURCE[0]}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
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
load_optional_env "$ROOT_DIR/memory_engine/.env"
load_optional_env "$ROOT_DIR/task_runner_service/.env"

MAIN_BACKEND_DIR="$ROOT_DIR/chat_app_server_rs"
MAIN_FRONTEND_DIR="$ROOT_DIR/chat_app"
USER_SERVICE_SCRIPT="$ROOT_DIR/user_service/restart_services.sh"

DEV_MONGO_HOST="${DEV_MONGO_HOST:-127.0.0.1}"
DEV_MONGO_PORT="${DEV_MONGO_PORT:-27018}"
DEV_MONGO_CONTAINER_NAME="${DEV_MONGO_CONTAINER_NAME:-chatos-dev-mongo}"
START_DEV_MONGO="${START_DEV_MONGO:-auto}"
MAIN_BACKEND_PORT="${MAIN_BACKEND_PORT:-${BACKEND_PORT:-3997}}"
LEGACY_MAIN_BACKEND_PORT=3001
MAIN_FRONTEND_PORT="${FRONTEND_PORT:-8088}"
TASK_RUNNER_CALLBACK_SECRET_DEFAULT="${TASK_RUNNER_CALLBACK_SECRET_DEFAULT:-chatos-task-runner-dev-secret}"
CHATOS_TASK_RUNNER_CALLBACK_SECRET="${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-$TASK_RUNNER_CALLBACK_SECRET_DEFAULT}}"
CHATOS_DATABASE_TYPE_EFFECTIVE="${DATABASE_TYPE:-mongodb}"
CHATOS_MONGODB_HOST_REQUESTED="${MONGODB_HOST:-$DEV_MONGO_HOST}"
CHATOS_MONGODB_HOST_EFFECTIVE="$(dev_mongo_client_host "$CHATOS_MONGODB_HOST_REQUESTED")"
CHATOS_MONGODB_PORT_EFFECTIVE="${MONGODB_PORT:-$DEV_MONGO_PORT}"
CHATOS_MONGODB_DB_EFFECTIVE="${MONGODB_DB:-chatos}"
CHATOS_MONGODB_USER_EFFECTIVE="${MONGODB_USER:-admin}"
CHATOS_MONGODB_PASSWORD_EFFECTIVE="${MONGODB_PASSWORD:-admin}"
CHATOS_MONGODB_AUTH_SOURCE_EFFECTIVE="${MONGODB_AUTH_SOURCE:-admin}"
CHATOS_MONGODB_CONNECTION_STRING_EFFECTIVE="${MONGODB_CONNECTION_STRING:-}"
CHATOS_USER_SERVICE_BASE_URL_EFFECTIVE="${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-}}"
MEMORY_ENGINE_HOST_EFFECTIVE="${MEMORY_ENGINE_HOST:-127.0.0.1}"
if [[ "$MEMORY_ENGINE_HOST_EFFECTIVE" == "0.0.0.0" || "$MEMORY_ENGINE_HOST_EFFECTIVE" == "::" || "$MEMORY_ENGINE_HOST_EFFECTIVE" == "[::]" ]]; then
  MEMORY_ENGINE_HOST_EFFECTIVE="127.0.0.1"
fi
MEMORY_ENGINE_PORT_EFFECTIVE="${MEMORY_ENGINE_PORT:-7081}"
MEMORY_ENGINE_BASE_URL_EFFECTIVE="${MEMORY_ENGINE_BASE_URL:-http://${MEMORY_ENGINE_HOST_EFFECTIVE}:${MEMORY_ENGINE_PORT_EFFECTIVE}/api/memory-engine/v1}"
MEMORY_ENGINE_OPERATOR_TOKEN_EFFECTIVE="${MEMORY_ENGINE_OPERATOR_TOKEN:-${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-dev-operator-token}}"
START_USER_SERVICE="${START_USER_SERVICE:-}"

if [[ -z "$START_USER_SERVICE" ]]; then
  case "$CHATOS_USER_SERVICE_BASE_URL_EFFECTIVE" in
    http://127.0.0.1:39190|http://localhost:39190)
      START_USER_SERVICE=1
      ;;
    *)
      START_USER_SERVICE=0
      ;;
  esac
fi

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

resolve_target_dir() {
  local target_dir="${CARGO_TARGET_DIR:-$ROOT_DIR/target-shared}"
  if [[ "$target_dir" != /* ]]; then
    target_dir="$ROOT_DIR/$target_dir"
  fi
  printf '%s\n' "$target_dir"
}

MAIN_BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
MAIN_FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
MAIN_BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
MAIN_FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"
MAIN_BACKEND_TARGET_DIR="$(resolve_target_dir)"
MAIN_BACKEND_BINARY="$MAIN_BACKEND_TARGET_DIR/debug/chat_app_server_rs"

LEGACY_MAIN_BACKEND_PID_FILE="$LEGACY_RUNTIME_DIR/backend.pid"
LEGACY_MAIN_FRONTEND_PID_FILE="$LEGACY_RUNTIME_DIR/frontend.pid"

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
      echo "[INFO] stopping project-owned $name process (pid=$pid, port=$port, cwd=$cwd_path)"
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
    echo "[ERROR] $name port is already in use: $port"
    if command -v lsof >/dev/null 2>&1; then
      echo "[INFO] current listener details:"
      lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
    fi
    echo "[HINT] change MAIN_BACKEND_PORT/BACKEND_PORT or stop the conflicting process first."
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
    tail -n 60 "$log_file" 2>/dev/null || true
    return 1
  fi
}

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-30}"

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
      if command -v lsof >/dev/null 2>&1; then
        lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
      fi
      return 1
    fi
    sleep 1
  done
}

ensure_chatos_dev_mongo() {
  local db_type
  db_type="$(printf '%s' "$CHATOS_DATABASE_TYPE_EFFECTIVE" | tr '[:upper:]' '[:lower:]')"
  if [[ "$db_type" != "mongodb" ]]; then
    return 0
  fi

  if [[ -n "$CHATOS_MONGODB_CONNECTION_STRING_EFFECTIVE" && -z "${MONGODB_HOST:-}" && -z "${MONGODB_PORT:-}" ]]; then
    if dev_mongo_is_auto "$START_DEV_MONGO"; then
      echo "[INFO] skip dev Mongo auto-start for chatos because MONGODB_CONNECTION_STRING is explicitly set"
      return 0
    fi
  fi

  ensure_dev_mongo_service \
    "$START_DEV_MONGO" \
    "$CHATOS_MONGODB_HOST_REQUESTED" \
    "$CHATOS_MONGODB_PORT_EFFECTIVE" \
    "$DEV_MONGO_CONTAINER_NAME"
}

prepare() {
  local cmd="${1:-restart}"

  need_cmd bash

  if [[ ! -d "$MAIN_BACKEND_DIR" || ! -d "$MAIN_FRONTEND_DIR" ]]; then
    echo "[ERROR] project directories are incomplete: $MAIN_BACKEND_DIR / $MAIN_FRONTEND_DIR"
    exit 1
  fi
  if [[ "$START_USER_SERVICE" == "1" && ! -f "$USER_SERVICE_SCRIPT" ]]; then
    echo "[ERROR] user_service startup script is missing: $USER_SERVICE_SCRIPT"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"

  case "$cmd" in
    restart|start)
      need_cmd npm
      need_cmd cargo
      ;;
  esac
}

start_user_service() {
  if [[ "$START_USER_SERVICE" != "1" ]]; then
    return 0
  fi
  bash "$USER_SERVICE_SCRIPT" restart
}

start_main_backend() {
  launch_service \
    "main backend" \
    "$MAIN_BACKEND_PORT" \
    "$MAIN_BACKEND_PID_FILE" \
    "$MAIN_BACKEND_LOG_FILE" \
    "cd \"$MAIN_BACKEND_DIR\" && if [[ -f .env ]]; then set -a; source .env; set +a; fi; cargo build --bin chat_app_server_rs && BACKEND_PORT=\"$MAIN_BACKEND_PORT\" DATABASE_TYPE=\"$CHATOS_DATABASE_TYPE_EFFECTIVE\" MONGODB_CONNECTION_STRING=\"$CHATOS_MONGODB_CONNECTION_STRING_EFFECTIVE\" MONGODB_HOST=\"$CHATOS_MONGODB_HOST_EFFECTIVE\" MONGODB_PORT=\"$CHATOS_MONGODB_PORT_EFFECTIVE\" MONGODB_DB=\"$CHATOS_MONGODB_DB_EFFECTIVE\" MONGODB_USER=\"$CHATOS_MONGODB_USER_EFFECTIVE\" MONGODB_PASSWORD=\"$CHATOS_MONGODB_PASSWORD_EFFECTIVE\" MONGODB_AUTH_SOURCE=\"$CHATOS_MONGODB_AUTH_SOURCE_EFFECTIVE\" MEMORY_ENGINE_BASE_URL=\"$MEMORY_ENGINE_BASE_URL_EFFECTIVE\" MEMORY_ENGINE_HOST=\"$MEMORY_ENGINE_HOST_EFFECTIVE\" MEMORY_ENGINE_PORT=\"$MEMORY_ENGINE_PORT_EFFECTIVE\" MEMORY_ENGINE_OPERATOR_TOKEN=\"$MEMORY_ENGINE_OPERATOR_TOKEN_EFFECTIVE\" TASK_RUNNER_CHATOS_CALLBACK_SECRET=\"$CHATOS_TASK_RUNNER_CALLBACK_SECRET\" CHATOS_TASK_RUNNER_CALLBACK_SECRET=\"$CHATOS_TASK_RUNNER_CALLBACK_SECRET\" exec \"$MAIN_BACKEND_BINARY\""
}

start_main_frontend() {
  launch_service \
    "main frontend" \
    "$MAIN_FRONTEND_PORT" \
    "$MAIN_FRONTEND_PID_FILE" \
    "$MAIN_FRONTEND_LOG_FILE" \
    "cd \"$MAIN_FRONTEND_DIR\" && exec npm run dev -- --host 0.0.0.0 --port \"$MAIN_FRONTEND_PORT\""
}

do_stop() {
  stop_from_pid_file "main backend" "$MAIN_BACKEND_PID_FILE"
  stop_from_pid_file "main frontend" "$MAIN_FRONTEND_PID_FILE"

  if [[ "$LEGACY_RUNTIME_DIR" != "$RUNTIME_DIR" ]]; then
    stop_from_pid_file "main backend (legacy runtime)" "$LEGACY_MAIN_BACKEND_PID_FILE"
    stop_from_pid_file "main frontend (legacy runtime)" "$LEGACY_MAIN_FRONTEND_PID_FILE"
  fi

  if [[ "$STOP_BY_PORT" == "1" ]]; then
    stop_from_port "main backend" "$MAIN_BACKEND_PORT"
    if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
      stop_from_port "main backend (legacy)" "$LEGACY_MAIN_BACKEND_PORT"
    fi
    stop_from_port "main frontend" "$MAIN_FRONTEND_PORT"
  else
    echo "[INFO] STOP_BY_PORT=${STOP_BY_PORT}; stopping only project-owned processes."
    stop_project_owned_port_processes "main backend" "$MAIN_BACKEND_PORT"
    if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
      stop_project_owned_port_processes "main backend (legacy)" "$LEGACY_MAIN_BACKEND_PORT"
    fi
    stop_project_owned_port_processes "main frontend" "$MAIN_FRONTEND_PORT"
  fi

  wait_port_released "main backend" "$MAIN_BACKEND_PORT" || return 1
  if [[ "$LEGACY_MAIN_BACKEND_PORT" != "$MAIN_BACKEND_PORT" ]]; then
    wait_port_released "main backend (legacy)" "$LEGACY_MAIN_BACKEND_PORT" || return 1
  fi
  wait_port_released "main frontend" "$MAIN_FRONTEND_PORT" || return 1

  if [[ "$START_USER_SERVICE" == "1" ]]; then
    bash "$USER_SERVICE_SCRIPT" stop || return 1
  fi
}

run_start_sequence() {
  local backend_timeout="${MAIN_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-120}}"
  local frontend_timeout="${MAIN_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-45}}"

  ensure_chatos_dev_mongo &&
    start_user_service &&
    start_main_backend &&
    start_main_frontend

  sleep 2 &&
    check_alive "main backend" "$MAIN_BACKEND_PID_FILE" "$MAIN_BACKEND_LOG_FILE" &&
    check_alive "main frontend" "$MAIN_FRONTEND_PID_FILE" "$MAIN_FRONTEND_LOG_FILE" &&
    wait_http_ready "main backend" "http://127.0.0.1:$MAIN_BACKEND_PORT/health" "$backend_timeout" &&
    wait_http_ready "main frontend" "http://127.0.0.1:$MAIN_FRONTEND_PORT" "$frontend_timeout"
}

print_runtime_info() {
  echo "[OK] all services are running"
  echo "  user_service enabled: $START_USER_SERVICE"
  if [[ "$START_USER_SERVICE" == "1" ]]; then
    echo "  user_service base_url: ${CHATOS_USER_SERVICE_BASE_URL_EFFECTIVE:-http://127.0.0.1:39190}"
  fi
  echo "  main backend pid: $(cat "$MAIN_BACKEND_PID_FILE")"
  echo "  main frontend pid: $(cat "$MAIN_FRONTEND_PID_FILE")"
  echo
  echo "  main backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  main frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  cargo target dir: $MAIN_BACKEND_TARGET_DIR"
  echo "  database type: $CHATOS_DATABASE_TYPE_EFFECTIVE"
  if [[ "$(printf '%s' "$CHATOS_DATABASE_TYPE_EFFECTIVE" | tr '[:upper:]' '[:lower:]')" == "mongodb" ]]; then
    echo "  mongodb target: ${CHATOS_MONGODB_HOST_EFFECTIVE}:${CHATOS_MONGODB_PORT_EFFECTIVE}/${CHATOS_MONGODB_DB_EFFECTIVE}"
  fi
  echo "  memory_engine base_url: $MEMORY_ENGINE_BASE_URL_EFFECTIVE"
  echo
  echo "  main frontend url: http://localhost:$MAIN_FRONTEND_PORT"
  echo "  main backend url: http://localhost:$MAIN_BACKEND_PORT"
}

status() {
  local main_backend_pid main_frontend_pid
  main_backend_pid="$(cat "$MAIN_BACKEND_PID_FILE" 2>/dev/null || true)"
  main_frontend_pid="$(cat "$MAIN_FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  user_service enabled: $START_USER_SERVICE"
  echo "  main backend pid: ${main_backend_pid:-N/A}"
  echo "  main frontend pid: ${main_frontend_pid:-N/A}"
  echo
  echo "  main backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  main frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  cargo target dir: $MAIN_BACKEND_TARGET_DIR"
  echo "  database type: $CHATOS_DATABASE_TYPE_EFFECTIVE"
  if [[ "$(printf '%s' "$CHATOS_DATABASE_TYPE_EFFECTIVE" | tr '[:upper:]' '[:lower:]')" == "mongodb" ]]; then
    echo "  mongodb target: ${CHATOS_MONGODB_HOST_EFFECTIVE}:${CHATOS_MONGODB_PORT_EFFECTIVE}/${CHATOS_MONGODB_DB_EFFECTIVE}"
  fi
  echo "  memory_engine base_url: $MEMORY_ENGINE_BASE_URL_EFFECTIVE"
  if [[ "$START_USER_SERVICE" == "1" ]]; then
    echo
    bash "$USER_SERVICE_SCRIPT" status || true
  fi
}

CMD="${1:-restart}"
prepare "$CMD"

case "$CMD" in
  restart|start)
    do_stop
    if run_start_sequence; then
      print_runtime_info
    else
      echo "[WARN] startup failed, rolling back..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] all services stopped"
    ;;
  status)
    status
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
