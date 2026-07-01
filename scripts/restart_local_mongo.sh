#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"

LOCAL_MONGO_VERSION="${LOCAL_MONGO_VERSION:-7.0.37}"
LOCAL_MONGO_ARCHIVE_NAME="${LOCAL_MONGO_ARCHIVE_NAME:-mongodb-linux-x86_64-ubuntu2204-${LOCAL_MONGO_VERSION}}"
LOCAL_MONGO_DOWNLOAD_URL="${LOCAL_MONGO_DOWNLOAD_URL:-https://fastdl.mongodb.org/linux/${LOCAL_MONGO_ARCHIVE_NAME}.tgz}"
LOCAL_MONGO_INSTALL_ROOT="${LOCAL_MONGO_INSTALL_ROOT:-$HOME/.local/opt/chatos-mongo}"
LOCAL_MONGO_ARCHIVE_PATH="${LOCAL_MONGO_ARCHIVE_PATH:-$LOCAL_MONGO_INSTALL_ROOT/${LOCAL_MONGO_ARCHIVE_NAME}.tgz}"
LOCAL_MONGO_HOME="${LOCAL_MONGO_HOME:-$LOCAL_MONGO_INSTALL_ROOT/${LOCAL_MONGO_ARCHIVE_NAME}}"
LOCAL_MONGO_MONGOD_BIN="${LOCAL_MONGO_MONGOD_BIN:-$LOCAL_MONGO_HOME/bin/mongod}"
LOCAL_MONGO_PYTHON_BIN="${LOCAL_MONGO_PYTHON_BIN:-python3}"
LOCAL_MONGO_PIP_BIN="${LOCAL_MONGO_PIP_BIN:-$LOCAL_MONGO_PYTHON_BIN -m pip}"
LOCAL_MONGO_HOST="${LOCAL_MONGO_HOST:-127.0.0.1}"
LOCAL_MONGO_PORT="${LOCAL_MONGO_PORT:-27018}"
LOCAL_MONGO_ROOT_USERNAME="${LOCAL_MONGO_ROOT_USERNAME:-admin}"
LOCAL_MONGO_ROOT_PASSWORD="${LOCAL_MONGO_ROOT_PASSWORD:-admin}"
LOCAL_MONGO_AUTH_SOURCE="${LOCAL_MONGO_AUTH_SOURCE:-admin}"
LOCAL_MONGO_DATA_DIR="${LOCAL_MONGO_DATA_DIR:-$HOME/.local/share/chatos-dev-mongo/data}"
LOCAL_MONGO_RUNTIME_DIR="${LOCAL_MONGO_RUNTIME_DIR:-/tmp/chatos_local_mongo}"
LOCAL_MONGO_LOG_FILE="${LOCAL_MONGO_LOG_FILE:-$LOCAL_MONGO_RUNTIME_DIR/mongod.log}"
LOCAL_MONGO_PID_FILE="${LOCAL_MONGO_PID_FILE:-$LOCAL_MONGO_RUNTIME_DIR/mongod.pid}"
LOCAL_MONGO_BOOTSTRAP_MARKER="${LOCAL_MONGO_BOOTSTRAP_MARKER:-$LOCAL_MONGO_DATA_DIR/.admin_bootstrapped}"
LOCAL_MONGO_BOOTSTRAP_SCRIPT="${LOCAL_MONGO_BOOTSTRAP_SCRIPT:-$ROOT_DIR/scripts/bootstrap_local_mongo_admin.py}"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $cmd"
    exit 1
  fi
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
    elif (echo >"/dev/tcp/$host/$port") >/dev/null 2>&1; then
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

wait_port_released() {
  local host="$1"
  local port="$2"
  local timeout_sec="${3:-15}"

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while wait_tcp_ready "$host" "$port" 1; do
    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      return 1
    fi
    sleep 1
  done
}

install_local_mongo() {
  need_cmd curl
  need_cmd tar

  mkdir -p "$LOCAL_MONGO_INSTALL_ROOT"
  if [[ -x "$LOCAL_MONGO_MONGOD_BIN" ]]; then
    return 0
  fi

  echo "[INFO] downloading local MongoDB: $LOCAL_MONGO_DOWNLOAD_URL"
  rm -f "$LOCAL_MONGO_ARCHIVE_PATH"
  curl -L --fail --output "$LOCAL_MONGO_ARCHIVE_PATH" "$LOCAL_MONGO_DOWNLOAD_URL"

  echo "[INFO] extracting local MongoDB to $LOCAL_MONGO_INSTALL_ROOT"
  rm -rf "$LOCAL_MONGO_HOME"
  tar -xzf "$LOCAL_MONGO_ARCHIVE_PATH" -C "$LOCAL_MONGO_INSTALL_ROOT"
}

ensure_pymongo() {
  if "$LOCAL_MONGO_PYTHON_BIN" -c 'import pymongo' >/dev/null 2>&1; then
    return 0
  fi

  echo "[INFO] installing pymongo for local Mongo bootstrap..."
  # shellcheck disable=SC2086
  $LOCAL_MONGO_PIP_BIN install --user pymongo >/dev/null
}

prepare_dirs() {
  mkdir -p "$LOCAL_MONGO_DATA_DIR" "$LOCAL_MONGO_RUNTIME_DIR"
}

start_mongod() {
  local auth_mode="$1"
  local extra_args=()

  if [[ "$auth_mode" == "auth" ]]; then
    extra_args+=(--auth)
  fi

  "$LOCAL_MONGO_MONGOD_BIN" \
    --bind_ip "$LOCAL_MONGO_HOST" \
    --port "$LOCAL_MONGO_PORT" \
    --dbpath "$LOCAL_MONGO_DATA_DIR" \
    --logpath "$LOCAL_MONGO_LOG_FILE" \
    --pidfilepath "$LOCAL_MONGO_PID_FILE" \
    --fork \
    "${extra_args[@]}"
}

stop_mongod() {
  if [[ -f "$LOCAL_MONGO_PID_FILE" ]]; then
    local pid
    pid="$(cat "$LOCAL_MONGO_PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      echo "[INFO] stopping local mongod (pid=$pid)"
      kill "$pid" >/dev/null 2>&1 || true
      sleep 1
      if kill -0 "$pid" >/dev/null 2>&1; then
        kill -9 "$pid" >/dev/null 2>&1 || true
      fi
    fi
    rm -f "$LOCAL_MONGO_PID_FILE"
  fi

  wait_port_released "$LOCAL_MONGO_HOST" "$LOCAL_MONGO_PORT" 15 || true
}

bootstrap_admin_user() {
  ensure_pymongo
  LOCAL_MONGO_HOST="$LOCAL_MONGO_HOST" \
    LOCAL_MONGO_PORT="$LOCAL_MONGO_PORT" \
    LOCAL_MONGO_ROOT_USERNAME="$LOCAL_MONGO_ROOT_USERNAME" \
    LOCAL_MONGO_ROOT_PASSWORD="$LOCAL_MONGO_ROOT_PASSWORD" \
    "$LOCAL_MONGO_PYTHON_BIN" "$LOCAL_MONGO_BOOTSTRAP_SCRIPT"
  touch "$LOCAL_MONGO_BOOTSTRAP_MARKER"
}

ensure_started() {
  install_local_mongo
  prepare_dirs

  if wait_tcp_ready "$LOCAL_MONGO_HOST" "$LOCAL_MONGO_PORT" 1; then
    echo "[INFO] local mongod already listening on ${LOCAL_MONGO_HOST}:${LOCAL_MONGO_PORT}"
    return 0
  fi

  if [[ ! -f "$LOCAL_MONGO_BOOTSTRAP_MARKER" ]]; then
    echo "[INFO] first-time local Mongo bootstrap"
    start_mongod "noauth"
    if ! wait_tcp_ready "$LOCAL_MONGO_HOST" "$LOCAL_MONGO_PORT" 30; then
      echo "[ERROR] local mongod did not become ready during bootstrap"
      return 1
    fi
    bootstrap_admin_user
    stop_mongod
  fi

  start_mongod "auth"
  if ! wait_tcp_ready "$LOCAL_MONGO_HOST" "$LOCAL_MONGO_PORT" 30; then
    echo "[ERROR] local mongod did not become ready with auth enabled"
    return 1
  fi
}

status() {
  local pid="N/A"
  if [[ -f "$LOCAL_MONGO_PID_FILE" ]]; then
    pid="$(cat "$LOCAL_MONGO_PID_FILE" 2>/dev/null || true)"
    pid="${pid:-N/A}"
  fi

  echo "[INFO] local Mongo runtime dir: $LOCAL_MONGO_RUNTIME_DIR"
  echo "  install root: $LOCAL_MONGO_INSTALL_ROOT"
  echo "  version: $LOCAL_MONGO_VERSION"
  echo "  host: $LOCAL_MONGO_HOST"
  echo "  port: $LOCAL_MONGO_PORT"
  echo "  data dir: $LOCAL_MONGO_DATA_DIR"
  echo "  log file: $LOCAL_MONGO_LOG_FILE"
  echo "  pid: $pid"
  echo "  auth source: $LOCAL_MONGO_AUTH_SOURCE"
  echo "  bootstrap marker: $LOCAL_MONGO_BOOTSTRAP_MARKER"
}

CMD="${1:-start}"

case "$CMD" in
  start|restart)
    if [[ "$CMD" == "restart" ]]; then
      stop_mongod
    fi
    ensure_started
    status
    ;;
  stop)
    stop_mongod
    echo "[OK] local Mongo stopped"
    ;;
  status)
    status
    ;;
  *)
    echo "usage: $0 [start|restart|stop|status]"
    exit 1
    ;;
esac
