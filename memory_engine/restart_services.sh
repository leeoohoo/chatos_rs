#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"

SCRIPT_PATH="${BASH_SOURCE[0]}"
MEMORY_ENGINE_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
ROOT_DIR="$(cd "$MEMORY_ENGINE_DIR/.." && pwd)"
BACKEND_DIR="$MEMORY_ENGINE_DIR/backend"
FRONTEND_DIR="$MEMORY_ENGINE_DIR/frontend"
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
load_optional_env "$MEMORY_ENGINE_DIR/.env"
load_optional_env "$BACKEND_DIR/.env"
load_optional_env "$FRONTEND_DIR/.env"

DEV_MONGO_HOST="${DEV_MONGO_HOST:-127.0.0.1}"
DEV_MONGO_PORT="${DEV_MONGO_PORT:-27018}"
DEV_MONGO_CONTAINER_NAME="${DEV_MONGO_CONTAINER_NAME:-chatos-dev-mongo}"
MEMORY_ENGINE_HOST="${MEMORY_ENGINE_HOST:-127.0.0.1}"
MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
MEMORY_ENGINE_FRONTEND_PORT="${MEMORY_ENGINE_FRONTEND_PORT:-4178}"
MEMORY_ENGINE_BASE_URL="${MEMORY_ENGINE_BASE_URL:-http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1}"
MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT="${MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT:-chatos-memory-engine-dev-operator-token}"
MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-$MEMORY_ENGINE_OPERATOR_TOKEN_DEFAULT}"
MEMORY_ENGINE_USER_SERVICE_BASE_URL="${MEMORY_ENGINE_USER_SERVICE_BASE_URL:-${MEMORY_ENGINE_USER_SERVICE_API_BASE:-${VITE_USER_SERVICE_API_BASE:-${CHATOS_USER_SERVICE_BASE_URL:-${USER_SERVICE_BASE_URL:-http://127.0.0.1:39190}}}}}"
MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS="${MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS:-${CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS:-${USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS:-5000}}}"
MEMORY_ENGINE_MONGODB_DATABASE="${MEMORY_ENGINE_MONGODB_DATABASE:-memory_engine}"
MEMORY_ENGINE_STOP_BY_PORT="${MEMORY_ENGINE_STOP_BY_PORT:-1}"
MEMORY_ENGINE_START_DEV_MONGO="${MEMORY_ENGINE_START_DEV_MONGO:-${START_DEV_MONGO:-auto}}"
MEMORY_ENGINE_DEV_MONGO_HOST="${MEMORY_ENGINE_DEV_MONGO_HOST:-$DEV_MONGO_HOST}"
MEMORY_ENGINE_DEV_MONGO_PORT="${MEMORY_ENGINE_DEV_MONGO_PORT:-$DEV_MONGO_PORT}"
MEMORY_ENGINE_DEV_MONGO_CONTAINER_NAME="${MEMORY_ENGINE_DEV_MONGO_CONTAINER_NAME:-$DEV_MONGO_CONTAINER_NAME}"
MEMORY_ENGINE_DEV_MONGO_CLIENT_HOST="$(dev_mongo_client_host "$MEMORY_ENGINE_DEV_MONGO_HOST")"
MEMORY_ENGINE_MONGODB_URI="${MEMORY_ENGINE_MONGODB_URI:-mongodb://admin:admin@${MEMORY_ENGINE_DEV_MONGO_CLIENT_HOST}:${MEMORY_ENGINE_DEV_MONGO_PORT}/admin}"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:memory-engine" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:memory-engine" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="memoryengine"
fi

MEMORY_ENGINE_RUNTIME_DIR="${MEMORY_ENGINE_RUNTIME_DIR:-/tmp/chatos_rs_memory_engine_${ROOT_HASH}}"
BACKEND_PID_FILE="$MEMORY_ENGINE_RUNTIME_DIR/backend.pid"
FRONTEND_PID_FILE="$MEMORY_ENGINE_RUNTIME_DIR/frontend.pid"
BACKEND_LOG_FILE="$MEMORY_ENGINE_RUNTIME_DIR/backend.log"
FRONTEND_LOG_FILE="$MEMORY_ENGINE_RUNTIME_DIR/frontend.log"

resolve_target_dir() {
  local target_dir="${MEMORY_ENGINE_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-$MEMORY_ENGINE_DIR/target}}"
  if [[ "$target_dir" != /* ]]; then
    target_dir="$MEMORY_ENGINE_DIR/$target_dir"
  fi
  printf '%s\n' "$target_dir"
}

MEMORY_ENGINE_CARGO_TARGET_DIR="$(resolve_target_dir)"

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

wait_tcp_ready() {
  local host="$1"
  local port="$2"
  local timeout_sec="${3:-30}"

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while true; do
    if command -v nc >/dev/null 2>&1; then
      if nc -z "$host" "$port" >/dev/null 2>&1; then
        return 0
      fi
    elif is_port_listening "$port"; then
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      return 1
    fi
    sleep 1
  done
}

ensure_frontend_deps() {
  if [[ -d "$FRONTEND_DIR/node_modules" ]]; then
    return
  fi

  echo "[INFO] installing memory_engine frontend dependencies..."
  (
    cd "$FRONTEND_DIR"
    npm install
  )
}

ensure_dev_mongo() {
  ensure_dev_mongo_service \
    "$MEMORY_ENGINE_START_DEV_MONGO" \
    "$MEMORY_ENGINE_DEV_MONGO_HOST" \
    "$MEMORY_ENGINE_DEV_MONGO_PORT" \
    "$MEMORY_ENGINE_DEV_MONGO_CONTAINER_NAME"
}

prepare() {
  local cmd="${1:-restart}"

  need_cmd bash

  if [[ ! -d "$BACKEND_DIR" || ! -d "$FRONTEND_DIR" ]]; then
    echo "[ERROR] memory_engine directory is incomplete"
    exit 1
  fi

  mkdir -p "$MEMORY_ENGINE_RUNTIME_DIR"

  case "$cmd" in
    restart|start)
      need_cmd cargo
      need_cmd npm
      ;;
  esac
}

start_backend() {
  launch_service \
    "memory_engine backend" \
    "$MEMORY_ENGINE_PORT" \
    "$BACKEND_PID_FILE" \
    "$BACKEND_LOG_FILE" \
    "cd \"$BACKEND_DIR\" && CARGO_TARGET_DIR=\"$MEMORY_ENGINE_CARGO_TARGET_DIR\" MEMORY_ENGINE_HOST=\"$MEMORY_ENGINE_HOST\" MEMORY_ENGINE_PORT=\"$MEMORY_ENGINE_PORT\" MEMORY_ENGINE_MONGODB_URI=\"$MEMORY_ENGINE_MONGODB_URI\" MEMORY_ENGINE_MONGODB_DATABASE=\"$MEMORY_ENGINE_MONGODB_DATABASE\" MEMORY_ENGINE_OPERATOR_TOKEN=\"$MEMORY_ENGINE_OPERATOR_TOKEN\" MEMORY_ENGINE_USER_SERVICE_BASE_URL=\"$MEMORY_ENGINE_USER_SERVICE_BASE_URL\" MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS=\"$MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS\" exec cargo run --bin memory_engine"
}

start_frontend() {
  ensure_frontend_deps
  launch_service \
    "memory_engine frontend" \
    "$MEMORY_ENGINE_FRONTEND_PORT" \
    "$FRONTEND_PID_FILE" \
    "$FRONTEND_LOG_FILE" \
    "cd \"$FRONTEND_DIR\" && VITE_MEMORY_ENGINE_API_BASE=\"$MEMORY_ENGINE_BASE_URL\" VITE_MEMORY_ENGINE_PORT=\"$MEMORY_ENGINE_PORT\" VITE_MEMORY_ENGINE_OPERATOR_TOKEN=\"$MEMORY_ENGINE_OPERATOR_TOKEN\" VITE_USER_SERVICE_API_BASE=\"$MEMORY_ENGINE_USER_SERVICE_BASE_URL\" exec npm run dev -- --host 0.0.0.0 --port \"$MEMORY_ENGINE_FRONTEND_PORT\" --strictPort"
}

do_stop() {
  stop_from_pid_file "memory_engine backend" "$BACKEND_PID_FILE"
  stop_from_pid_file "memory_engine frontend" "$FRONTEND_PID_FILE"

  if [[ "$MEMORY_ENGINE_STOP_BY_PORT" == "1" ]]; then
    stop_from_port "memory_engine backend" "$MEMORY_ENGINE_PORT"
    stop_from_port "memory_engine frontend" "$MEMORY_ENGINE_FRONTEND_PORT"
  fi

  wait_port_released "memory_engine backend" "$MEMORY_ENGINE_PORT" || return 1
  wait_port_released "memory_engine frontend" "$MEMORY_ENGINE_FRONTEND_PORT" || return 1
}

run_start_sequence() {
  local backend_timeout="${MEMORY_ENGINE_BACKEND_HEALTHCHECK_TIMEOUT_SEC:-${STARTUP_HEALTHCHECK_TIMEOUT_SEC:-180}}"
  local frontend_timeout="${MEMORY_ENGINE_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-45}"

  ensure_dev_mongo &&
    start_backend &&
    start_frontend

  sleep 2 &&
    check_alive "memory_engine backend" "$BACKEND_PID_FILE" "$BACKEND_LOG_FILE" &&
    check_alive "memory_engine frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE" &&
    wait_http_ready "memory_engine backend" "http://127.0.0.1:${MEMORY_ENGINE_PORT}/health" "$backend_timeout" &&
    wait_http_ready "memory_engine frontend" "http://127.0.0.1:${MEMORY_ENGINE_FRONTEND_PORT}" "$frontend_timeout"
}

print_runtime_info() {
  echo "[OK] memory_engine is running"
  echo "  backend pid: $(cat "$BACKEND_PID_FILE")"
  echo "  frontend pid: $(cat "$FRONTEND_PID_FILE")"
  echo
  echo "  runtime dir: $MEMORY_ENGINE_RUNTIME_DIR"
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo "  cargo target dir: $MEMORY_ENGINE_CARGO_TARGET_DIR"
  echo
  echo "  backend url: http://localhost:$MEMORY_ENGINE_PORT"
  echo "  frontend url: http://localhost:$MEMORY_ENGINE_FRONTEND_PORT"
  echo "  api base url: $MEMORY_ENGINE_BASE_URL"
  echo "  mongodb uri: $MEMORY_ENGINE_MONGODB_URI"
}

status() {
  local backend_pid frontend_pid
  backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
  frontend_pid="$(cat "$FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $MEMORY_ENGINE_RUNTIME_DIR"
  echo "  backend pid: ${backend_pid:-N/A}"
  echo "  frontend pid: ${frontend_pid:-N/A}"
  echo
  echo "  backend log: $BACKEND_LOG_FILE"
  echo "  frontend log: $FRONTEND_LOG_FILE"
  echo "  cargo target dir: $MEMORY_ENGINE_CARGO_TARGET_DIR"
  echo
  echo "  backend url: http://localhost:$MEMORY_ENGINE_PORT"
  echo "  frontend url: http://localhost:$MEMORY_ENGINE_FRONTEND_PORT"
  echo "  api base url: $MEMORY_ENGINE_BASE_URL"
  echo "  mongodb uri: $MEMORY_ENGINE_MONGODB_URI"
}

CMD="${1:-restart}"
prepare "$CMD"

case "$CMD" in
  restart|start)
    do_stop
    if run_start_sequence; then
      print_runtime_info
    else
      echo "[WARN] memory_engine startup failed, cleaning up..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] memory_engine stopped"
    ;;
  status)
    status
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
