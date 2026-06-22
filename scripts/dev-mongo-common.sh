#!/usr/bin/env bash

DEV_MONGO_COMMON_SCRIPT_PATH="${BASH_SOURCE[0]}"
DEV_MONGO_COMMON_ROOT_DIR="$(cd "$(dirname "$DEV_MONGO_COMMON_SCRIPT_PATH")/.." && pwd)"
DEV_MONGO_LOCAL_SCRIPT_DEFAULT="$DEV_MONGO_COMMON_ROOT_DIR/scripts/restart_local_mongo.sh"
DEV_MONGO_LOCAL_VERSION_DEFAULT="${LOCAL_MONGO_VERSION:-7.0.37}"
DEV_MONGO_LOCAL_MONGOD_BIN_DEFAULT="${HOME}/.local/opt/chatos-mongo/mongodb-linux-x86_64-ubuntu2204-${DEV_MONGO_LOCAL_VERSION_DEFAULT}/bin/mongod"

dev_mongo_is_falsey() {
  case "${1:-}" in
    0|false|False|FALSE|off|OFF|no|NO)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

dev_mongo_is_auto() {
  case "${1:-}" in
    ""|auto|Auto|AUTO)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

dev_mongo_is_local_host() {
  case "${1:-}" in
    ""|127.0.0.1|localhost|0.0.0.0|::|[::]|::1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

dev_mongo_client_host() {
  case "${1:-}" in
    ""|localhost|0.0.0.0|::|[::]|::1)
      printf '127.0.0.1\n'
      ;;
    *)
      printf '%s\n' "$1"
      ;;
  esac
}

dev_mongo_bind_host() {
  case "${1:-}" in
    ""|localhost|::|[::]|::1)
      printf '127.0.0.1\n'
      ;;
    *)
      printf '%s\n' "$1"
      ;;
  esac
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

dev_mongo_print_existing_listener() {
  local host="$1"
  local port="$2"

  echo "[INFO] dev Mongo already reachable on ${host}:${port}; reusing existing instance"
  if command -v lsof >/dev/null 2>&1; then
    local listeners
    listeners="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$listeners" ]]; then
      echo "[INFO] existing listener on port ${port}:"
      printf '%s\n' "$listeners"
    fi
  fi
}

resolve_dev_mongo_docker_cmd() {
  if command -v docker >/dev/null 2>&1; then
    command -v docker
    return 0
  fi
  if command -v docker.exe >/dev/null 2>&1; then
    command -v docker.exe
    return 0
  fi
  return 1
}

resolve_dev_mongo_local_script() {
  local script_path="${DEV_MONGO_LOCAL_SCRIPT:-$DEV_MONGO_LOCAL_SCRIPT_DEFAULT}"
  if [[ -x "$script_path" ]]; then
    printf '%s\n' "$script_path"
    return 0
  fi
  return 1
}

dev_mongo_prefers_local() {
  case "${DEV_MONGO_MODE:-auto}" in
    local|Local|LOCAL)
      return 0
      ;;
    auto|Auto|AUTO|"")
      local mongod_bin="${LOCAL_MONGO_MONGOD_BIN:-$DEV_MONGO_LOCAL_MONGOD_BIN_DEFAULT}"
      [[ -x "$mongod_bin" ]]
      return
      ;;
    *)
      return 1
      ;;
  esac
}

dev_mongo_requires_local() {
  case "${DEV_MONGO_MODE:-auto}" in
    local|Local|LOCAL)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

dev_mongo_prefers_docker() {
  case "${DEV_MONGO_MODE:-auto}" in
    docker|Docker|DOCKER|auto|Auto|AUTO|"")
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

ensure_dev_mongo_service() {
  local start_mode="${1:-auto}"
  local requested_host="${2:-127.0.0.1}"
  local port="${3:-27018}"
  local container_name="${4:-chatos-dev-mongo}"
  local client_host bind_host docker_cmd

  if dev_mongo_is_falsey "$start_mode"; then
    return 0
  fi

  client_host="$(dev_mongo_client_host "$requested_host")"
  bind_host="$(dev_mongo_bind_host "$requested_host")"

  if wait_tcp_ready "$client_host" "$port" 1; then
    dev_mongo_print_existing_listener "$client_host" "$port"
    return 0
  fi

  if ! dev_mongo_is_local_host "$requested_host"; then
    if dev_mongo_is_auto "$start_mode"; then
      echo "[WARN] skipping dev Mongo auto-start because host is not local: $requested_host"
      return 0
    fi
    echo "[ERROR] cannot auto-start Mongo for non-local host: $requested_host"
    return 1
  fi

  if dev_mongo_prefers_local; then
    local local_script
    if local_script="$(resolve_dev_mongo_local_script)"; then
      LOCAL_MONGO_HOST="$client_host" \
        LOCAL_MONGO_PORT="$port" \
        "$local_script" start
      if wait_tcp_ready "$client_host" "$port" 30; then
        return 0
      fi
      echo "[WARN] local mongod helper did not make Mongo ready on ${client_host}:${port}"
      if dev_mongo_requires_local; then
        return 1
      fi
    fi
  fi

  if ! dev_mongo_prefers_docker; then
    echo "[ERROR] dev Mongo mode forbids Docker fallback: ${DEV_MONGO_MODE:-auto}"
    return 1
  fi

  if ! docker_cmd="$(resolve_dev_mongo_docker_cmd)"; then
    if dev_mongo_is_auto "$start_mode"; then
      echo "[WARN] docker not found; skip dev Mongo auto-start"
      return 0
    fi
    echo "[ERROR] docker is required to auto-start dev Mongo"
    return 1
  fi

  echo "[INFO] starting dev Mongo container: $container_name"
  if "$docker_cmd" container inspect "$container_name" >/dev/null 2>&1; then
    "$docker_cmd" start "$container_name" >/dev/null
  else
    "$docker_cmd" run -d \
      --name "$container_name" \
      -p "${bind_host}:${port}:27017" \
      -e MONGO_INITDB_ROOT_USERNAME=admin \
      -e MONGO_INITDB_ROOT_PASSWORD=admin \
      mongo:7 >/dev/null
  fi

  if ! wait_tcp_ready "$client_host" "$port" 30; then
    echo "[ERROR] dev Mongo did not become ready on ${client_host}:${port}"
    return 1
  fi
}
