#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
STATE_DIR="${CHATOS_LOCAL_DEV_STATE_DIR:-$ROOT_DIR/.chatos-local-dev}"
LOG_DIR="$STATE_DIR/logs"
PID_DIR="$STATE_DIR/pids"
ENV_FILE="${CHATOS_LOCAL_DEV_ENV_FILE:-$ROOT_DIR/docker/.env}"
ACTION="${1:-up}"

COMPOSE_FILE="$ROOT_DIR/docker/compose.yml"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-chatos-rs}"

INFRA_SERVICES=(consul mongodb harness)
DOCKER_APP_SERVICES=(
  user-service-backend
  memory-engine-backend
  project-management-backend
  plugin-management-backend
  local-connector-service-backend
  sandbox-manager-backend
  task-runner-backend
  chatos-backend
  official-website-backend
  chatos-frontend
  user-service-frontend
  memory-engine-frontend
  project-management-frontend
  plugin-management-frontend
  task-runner-frontend
  sandbox-manager-frontend
  official-website-frontend
)

BACKEND_SERVICES=(
  "user-service-backend|user-service|user_service/backend/Cargo.toml|/api/health|39190|user_service_backend"
  "memory-engine-backend|memory-engine|memory_engine/backend/Cargo.toml|/health|7081|memory_engine"
  "project-management-backend|project-service|project_management_service/backend/Cargo.toml|/api/health|39210|project_management_service_backend"
  "plugin-management-backend|plugin-management-service|plugin_management_service/backend/Cargo.toml|/api/health|39260|plugin_management_service_backend"
  "local-connector-service-backend|local-connector-service|local_connector_service/backend/Cargo.toml|/api/health|39230|local_connector_service_backend"
  "sandbox-manager-backend|sandbox-manager|sandbox_manager_service/backend/Cargo.toml|/health|8095|sandbox_manager_service_backend"
  "task-runner-backend|task-runner|task_runner_service/backend/Cargo.toml|/api/health|39090|task_runner_service_backend"
  "chatos-backend|chatos-backend|chatos/backend/Cargo.toml|/health|3997|chat_app_server_rs"
  "official-website-backend|official-website|official_website_service/backend/Cargo.toml|/health|39250|official_website_service_backend"
)

FRONTEND_SERVICES=(
  "chatos-frontend|chatos/frontend|8088"
  "user-service-frontend|user_service/frontend|39191"
  "memory-engine-frontend|memory_engine/frontend|4178"
  "project-management-frontend|project_management_service/frontend|39211"
  "plugin-management-frontend|plugin_management_service/frontend|39261"
  "task-runner-frontend|task_runner_service/frontend|39091"
  "sandbox-manager-frontend|sandbox_manager_service/frontend|8096"
  "official-website-frontend|official_website_service/frontend|39251"
  "local-connector-client-frontend|local_connector_client/frontend|39233"
)

LOCAL_CONNECTOR_CORE_PORT="${LOCAL_CONNECTOR_CORE_API_PORT:-39232}"

load_env_file() {
  local file="$1"
  if [[ -f "$file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$file"
    set +a
  fi
}

env_value() {
  local key="$1"
  local default_value="$2"
  if [[ -n "${!key:-}" ]]; then
    printf '%s' "${!key}"
  else
    printf '%s' "$default_value"
  fi
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $cmd" >&2
    exit 1
  fi
}

compose() {
  local args=(-p "$COMPOSE_PROJECT_NAME" -f "$COMPOSE_FILE")
  if [[ -f "$ENV_FILE" ]]; then
    args+=(--env-file "$ENV_FILE")
  fi
  docker compose "${args[@]}" "$@"
}

pid_file_for() {
  printf '%s/%s.pid\n' "$PID_DIR" "$1"
}

log_file_for() {
  printf '%s/%s.log\n' "$LOG_DIR" "$1"
}

spawn_detached() {
  local cwd="$1"
  local log_file="$2"
  shift 2
  python3 - "$cwd" "$log_file" "$@" <<'PY'
import os
import subprocess
import sys

cwd = sys.argv[1]
log_path = sys.argv[2]
command = sys.argv[3:]

with open(log_path, "ab", buffering=0) as log:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=os.environ.copy(),
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

print(process.pid)
PY
}

target_binary_for() {
  local bin="$1"
  local target_dir="${CARGO_TARGET_DIR:-$ROOT_DIR/target-shared}"
  local binary="$target_dir/debug/$bin"
  if [[ ! -x "$binary" && -x "$binary.exe" ]]; then
    binary="$binary.exe"
  fi
  printf '%s\n' "$binary"
}

pid_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n 1 || true
  fi
}

pids_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true
  fi
}

repo_managed_pids() {
  python3 - "$ROOT_DIR" <<'PY'
import os
import subprocess
import sys

root = os.path.realpath(sys.argv[1])
service_bins = {
    "user_service_backend",
    "memory_engine",
    "project_management_service_backend",
    "plugin_management_service_backend",
    "local_connector_service_backend",
    "sandbox_manager_service_backend",
    "task_runner_service_backend",
    "chat_app_server_rs",
    "official_website_service_backend",
    "local_connector_client_core",
}

current = os.getpid()
parent = os.getppid()
rows = []
output = subprocess.check_output(["ps", "-axo", "pid=,ppid=,command="], text=True)
for line in output.splitlines():
    parts = line.strip().split(None, 2)
    if len(parts) < 3:
        continue
    pid, ppid, command = int(parts[0]), int(parts[1]), parts[2]
    if pid in {current, parent}:
        continue
    rows.append((pid, ppid, command))

matched = set()
for pid, _ppid, command in rows:
    if root not in command:
        continue
    if any(f"/{name}" in command or command.endswith(name) for name in service_bins):
        matched.add(pid)
        continue
    if "/node_modules/.bin/vite" in command or "/node_modules/@esbuild/" in command:
        matched.add(pid)

matched_ppids = {ppid for pid, ppid, _command in rows if pid in matched}
for pid, _ppid, command in rows:
    if pid in matched_ppids and command.startswith("npm run dev"):
        matched.add(pid)

for pid in sorted(matched, reverse=True):
    print(pid)
PY
}

stop_pid() {
  local pid="$1"
  local name="$2"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  echo "[INFO] stopping $name (pid=$pid)"
  kill "-$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  sleep 1
  if kill -0 "$pid" 2>/dev/null; then
    kill -9 "-$pid" 2>/dev/null || true
    kill -9 "$pid" 2>/dev/null || true
  fi
}

stop_service_pid() {
  local name="$1"
  local file
  file="$(pid_file_for "$name")"
  if [[ -f "$file" ]]; then
    stop_pid "$(cat "$file")" "$name"
    rm -f "$file"
  fi
}

stop_port_if_needed() {
  local port="$1"
  local name="$2"
  local pid
  while IFS= read -r pid; do
    if [[ -n "$pid" ]]; then
      stop_pid "$pid" "$name on port $port"
    fi
  done < <(pids_for_port "$port")
}

stop_repo_managed_processes() {
  local pid
  while IFS= read -r pid; do
    if [[ -n "$pid" ]]; then
      stop_pid "$pid" "stale local dev process"
    fi
  done < <(repo_managed_pids)
}

stop_managed_ports() {
  local item name unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused port <<<"$item"
    stop_port_if_needed "$port" "$name"
  done
  stop_port_if_needed "$LOCAL_CONNECTOR_CORE_PORT" "local-connector-client-core"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    stop_port_if_needed "$port" "$name"
  done
}

managed_ports_busy() {
  local item _name _unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r _name _unused port <<<"$item"
    if [[ -n "$(pids_for_port "$port")" ]]; then
      return 0
    fi
  done
  if [[ -n "$(pids_for_port "$LOCAL_CONNECTOR_CORE_PORT")" ]]; then
    return 0
  fi
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r _name _unused _unused _unused port _unused <<<"$item"
    if [[ -n "$(pids_for_port "$port")" ]]; then
      return 0
    fi
  done
  return 1
}

cleanup_local_dev_processes() {
  local attempt
  for attempt in 1 2 3 4 5; do
    stop_repo_managed_processes
    stop_managed_ports
    if ! managed_ports_busy && [[ -z "$(repo_managed_pids)" ]]; then
      return 0
    fi
    sleep 1
  done
}

wait_for_http() {
  local name="$1"
  local url="$2"
  local timeout="${3:-90}"
  local start
  start="$(date +%s)"
  while true; do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "[OK] $name is ready: $url"
      return 0
    fi
    if (( "$(date +%s)" - start >= timeout )); then
      echo "[WARN] $name did not become healthy within ${timeout}s: $url" >&2
      echo "       log: $(log_file_for "$name")" >&2
      return 1
    fi
    sleep 2
  done
}

wait_for_port() {
  local name="$1"
  local port="$2"
  local timeout="${3:-90}"
  local start
  start="$(date +%s)"
  while true; do
    if [[ -n "$(pid_for_port "$port")" ]]; then
      echo "[OK] $name is listening on port $port"
      return 0
    fi
    if (( "$(date +%s)" - start >= timeout )); then
      echo "[WARN] $name did not listen within ${timeout}s on port $port" >&2
      echo "       log: $(log_file_for "$name")" >&2
      return 1
    fi
    sleep 2
  done
}

wait_for_consul() {
  local consul_addr="${CHATOS_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  wait_for_http "consul" "${consul_addr%/}/v1/status/leader" "${CHATOS_LOCAL_DEV_INFRA_TIMEOUT_SECONDS:-120}" || true
}

export_local_env() {
  local mongo_user mongo_password mongo_port
  mongo_user="$(env_value MONGODB_USER admin)"
  mongo_password="$(env_value MONGODB_PASSWORD admin)"
  mongo_port="$(env_value MONGODB_HOST_PORT 27018)"

  export CHATOS_ENV="${CHATOS_LOCAL_DEV_ENV:-local}"
  export CHATOS_SERVICE_RUNTIME_ENABLED="${CHATOS_LOCAL_DEV_SERVICE_RUNTIME_ENABLED:-true}"
  export CHATOS_SERVICE_DISCOVERY_MODE="${CHATOS_LOCAL_DEV_DISCOVERY_MODE:-consul,static}"
  export CHATOS_CONSUL_HTTP_ADDR="${CHATOS_LOCAL_DEV_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  export CHATOS_SERVICE_ADDRESS="${CHATOS_LOCAL_DEV_SERVICE_ADDRESS:-127.0.0.1}"
  export CHATOS_SERVICE_CHECK_ADDRESS="${CHATOS_LOCAL_DEV_SERVICE_CHECK_ADDRESS:-host.docker.internal}"

  export OPENAI_API_KEY="${OPENAI_API_KEY:-}"
  export OPENAI_BASE_URL="${OPENAI_BASE_URL:-https://api.openai.com/v1}"
  export MONGODB_USER="$mongo_user"
  export MONGODB_PASSWORD="$mongo_password"
  export MONGODB_HOST="${MONGODB_HOST:-127.0.0.1}"
  export MONGODB_PORT="$mongo_port"
  export MONGODB_AUTH_SOURCE="${MONGODB_AUTH_SOURCE:-admin}"

  export CHATOS_ADMIN_USERNAME="${CHATOS_ADMIN_USERNAME:-admin}"
  export CHATOS_ADMIN_PASSWORD="${CHATOS_ADMIN_PASSWORD:-admin123456}"
  export CHATOS_ADMIN_DISPLAY_NAME="${CHATOS_ADMIN_DISPLAY_NAME:-System Admin}"
  export AUTH_JWT_SECRET="${AUTH_JWT_SECRET:-dev-only-change-me-please}"
  export USER_SERVICE_JWT_SECRET="${USER_SERVICE_JWT_SECRET:-change_me_user_service_secret}"
  export USER_SERVICE_INTERNAL_API_SECRET="${USER_SERVICE_INTERNAL_API_SECRET:-change_me_user_service_internal_secret}"
  export TASK_RUNNER_INTERNAL_API_SECRET="${TASK_RUNNER_INTERNAL_API_SECRET:-change_me_task_runner_internal_secret}"
  export PLUGIN_MANAGEMENT_INTERNAL_API_SECRET="${PLUGIN_MANAGEMENT_INTERNAL_API_SECRET:-change_me_plugin_management_internal_secret}"
  export TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-change_me_chatos_task_runner_secret}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}"
  export CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-chatos-local-connector-dev-secret}"
  export MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-dev-operator-token}"
  export SANDBOX_MANAGER_OPERATOR_TOKEN="${SANDBOX_MANAGER_OPERATOR_TOKEN:-chatos-sandbox-manager-dev-operator-token}"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID="${TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID:-task_runner}"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY="${TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY:-chatos-task-runner-sandbox-dev-key}"

  export USER_SERVICE_HOST="${USER_SERVICE_HOST:-0.0.0.0}"
  export USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
  export MEMORY_ENGINE_HOST="${MEMORY_ENGINE_HOST:-0.0.0.0}"
  export MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
  export PROJECT_SERVICE_HOST="${PROJECT_SERVICE_HOST:-0.0.0.0}"
  export PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
  export PLUGIN_MANAGEMENT_SERVICE_HOST="${PLUGIN_MANAGEMENT_SERVICE_HOST:-0.0.0.0}"
  export PLUGIN_MANAGEMENT_SERVICE_PORT="${PLUGIN_MANAGEMENT_SERVICE_PORT:-39260}"
  export PLUGIN_MANAGEMENT_SERVICE_URL="${PLUGIN_MANAGEMENT_SERVICE_URL:-http://127.0.0.1:${PLUGIN_MANAGEMENT_SERVICE_PORT}}"
  export LOCAL_CONNECTOR_SERVICE_HOST="${LOCAL_CONNECTOR_SERVICE_HOST:-0.0.0.0}"
  export LOCAL_CONNECTOR_SERVICE_PORT="${LOCAL_CONNECTOR_SERVICE_PORT:-39230}"
  export SANDBOX_MANAGER_HOST="${SANDBOX_MANAGER_HOST:-0.0.0.0}"
  export SANDBOX_MANAGER_PORT="${SANDBOX_MANAGER_PORT:-8095}"
  export TASK_RUNNER_HOST="${TASK_RUNNER_HOST:-0.0.0.0}"
  export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-39090}"
  export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-39090}"
  export HOST="${HOST:-0.0.0.0}"
  export BACKEND_PORT="${BACKEND_PORT:-3997}"
  export OFFICIAL_WEBSITE_HOST="${OFFICIAL_WEBSITE_HOST:-0.0.0.0}"
  export OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-39250}"

  export USER_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/user_service?authSource=admin"
  export MEMORY_ENGINE_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin"
  export PROJECT_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/project_management_service?authSource=admin"
  export PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/plugin_management_service?authSource=admin"
  export PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE="${PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE:-plugin_management_service}"
  export LOCAL_CONNECTOR_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/local_connector_service?authSource=admin"
  export SANDBOX_MANAGER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/sandbox_manager_service?authSource=admin"
  export TASK_RUNNER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/task_runner_service?authSource=admin"
  export LEGACY_AUTH_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin"

  export MEMORY_ENGINE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export MEMORY_ENGINE_BASE_URL="http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1"
  export TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export CHATOS_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PROJECT_SERVICE_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET="$CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_BASE_URL"
  export PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  export PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
  export PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME="$CHATOS_ADMIN_USERNAME"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD="$CHATOS_ADMIN_PASSWORD"
  export PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES="${PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES:-true}"
  export LOCAL_CONNECTOR_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export LOCAL_CONNECTOR_PUBLIC_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export LOCAL_CONNECTOR_INTERNAL_API_SECRET="$CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"
  export SANDBOX_MANAGER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export SANDBOX_MANAGER_SYSTEM_CLIENT_ID="$TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID"
  export SANDBOX_MANAGER_SYSTEM_CLIENT_KEY="$TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY"
  export SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE="${SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE:-published}"
  export SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT="${SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT:-true}"
  export TASK_RUNNER_STORE_MODE="${TASK_RUNNER_STORE_MODE:-mongo}"
  export TASK_RUNNER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
  export TASK_RUNNER_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_BASE_URL"
  export TASK_RUNNER_SANDBOX_MANAGER_BASE_URL="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  export TASK_RUNNER_CHATOS_CALLBACK_URL="http://127.0.0.1:${BACKEND_PORT}/api/agent/chat/task-runner/callback"
  export TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET="$CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"
  export CHATOS_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
  export CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET="$CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"
  export HARNESS_PROVISIONING_ENABLED="${CHATOS_LOCAL_DEV_HARNESS_PROVISIONING_ENABLED:-true}"
  export HARNESS_BASE_URL="${CHATOS_LOCAL_DEV_HARNESS_BASE_URL:-http://127.0.0.1:3000}"
  export OFFICIAL_WEBSITE_STATUS_HOST="${OFFICIAL_WEBSITE_STATUS_HOST:-127.0.0.1}"
}

ensure_dirs() {
  mkdir -p "$LOG_DIR" "$PID_DIR" "$STATE_DIR/task-runner" "$STATE_DIR/chatos" "$STATE_DIR/sandboxes"
}

start_infra() {
  need_cmd docker
  echo "[INFO] starting local-dev infrastructure containers: ${INFRA_SERVICES[*]}"
  compose up -d "${INFRA_SERVICES[@]}"
}

stop_docker_app_services() {
  need_cmd docker
  echo "[INFO] stopping Docker app containers that conflict with local ports"
  compose stop "${DOCKER_APP_SERVICES[@]}" >/dev/null 2>&1 || true
  docker rm -f \
    "${COMPOSE_PROJECT_NAME}-db-connection-hub-backend-1" \
    "${COMPOSE_PROJECT_NAME}-db-connection-hub-frontend-1" \
    >/dev/null 2>&1 || true
}

deregister_local_dev_services() {
  local consul_addr="${CHATOS_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  local services_file ids_file id attempt
  if ! command -v curl >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
    return 0
  fi
  services_file="$(mktemp)"
  ids_file="$(mktemp)"
  for attempt in 1 2 3 4 5; do
    if ! curl -fsS "${consul_addr%/}/v1/agent/services" >"$services_file" 2>/dev/null; then
      rm -f "$services_file" "$ids_file"
      return 0
    fi
    python3 - "$services_file" >"$ids_file" <<'PY'
import json
import sys

managed = {
    "user-service",
    "memory-engine",
    "project-service",
    "plugin-management-service",
    "local-connector-service",
    "sandbox-manager",
    "task-runner",
    "chatos-backend",
    "official-website",
    "harness",
}

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    services = json.load(fh)

for service_id, item in services.items():
    if item.get("Service") in managed:
        print(service_id)
PY
    if [[ ! -s "$ids_file" ]]; then
      break
    fi
    while IFS= read -r id; do
      if [[ -n "$id" ]]; then
        curl -fsS -X PUT "${consul_addr%/}/v1/agent/service/deregister/$id" >/dev/null 2>&1 || true
      fi
    done <"$ids_file"
    sleep 0.2
  done
  rm -f "$services_file" "$ids_file"
}

register_local_dev_harness_service() {
  local consul_addr="${CHATOS_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  local harness_port="${HARNESS_PORT:-3000}"
  local body_file
  if ! command -v curl >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
    return 0
  fi
  body_file="$(mktemp)"
  python3 - "$body_file" "$harness_port" <<'PY'
import json
import sys

harness_port = int(sys.argv[2])

body = {
    "ID": "harness-docker",
    "Name": "harness",
    "Address": "127.0.0.1",
    "Port": harness_port,
    "Tags": ["local"],
    "Check": {
        "HTTP": "http://harness:3000/api/v1/system/health",
        "Interval": "10s",
        "Timeout": "3s",
        "DeregisterCriticalServiceAfter": "1m",
    },
}

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(body, fh)
PY
  curl -fsS -X PUT \
    -H "Content-Type: application/json" \
    --data-binary "@$body_file" \
    "${consul_addr%/}/v1/agent/service/register" >/dev/null 2>&1 || true
  rm -f "$body_file"
}

start_backend() {
  local name="$1"
  local service_name="$2"
  local manifest="$3"
  local health_path="$4"
  local port="$5"
  local bin="${6:-}"
  local log_file pid_file
  local binary
  local -a cargo_args=(build --manifest-path "$manifest")
  if [[ -z "$bin" ]]; then
    echo "[ERROR] missing binary name for $name" >&2
    exit 1
  fi
  cargo_args+=(--bin "$bin")
  binary="$(target_binary_for "$bin")"
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  stop_port_if_needed "$port" "$name"
  echo "[INFO] starting $name on 127.0.0.1:$port"
  : >"$log_file"
  (
    cd "$ROOT_DIR"
    cargo "${cargo_args[@]}"
  ) >>"$log_file" 2>&1
  local spawned_pid
  spawned_pid="$(
    export CHATOS_SERVICE_NAME="$service_name"
    export CHATOS_SERVICE_ID="${service_name}-local"
    export CHATOS_SERVICE_PORT="$port"
    export CHATOS_SERVICE_HEALTH_PATH="$health_path"
    spawn_detached "$ROOT_DIR" "$log_file" "$binary"
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_http "$name" "http://127.0.0.1:${port}${health_path}" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
}

start_local_connector_core() {
  local name="local-connector-client-core"
  local log_file pid_file
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  stop_port_if_needed "$LOCAL_CONNECTOR_CORE_PORT" "$name"
  echo "[INFO] starting $name on 127.0.0.1:$LOCAL_CONNECTOR_CORE_PORT"
  : >"$log_file"
  (
    cd "$ROOT_DIR"
    cargo build --manifest-path local_connector_client/core/Cargo.toml --bin local_connector_client_core
  ) >>"$log_file" 2>&1
  local spawned_pid
  spawned_pid="$(
    export LOCAL_CONNECTOR_CORE_API_PORT="$LOCAL_CONNECTOR_CORE_PORT"
    export LOCAL_CONNECTOR_CLOUD_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
    export LOCAL_CONNECTOR_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
    spawn_detached "$ROOT_DIR" "$log_file" "$(target_binary_for local_connector_client_core)"
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_port "$name" "$LOCAL_CONNECTOR_CORE_PORT" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
}

start_frontend() {
  local name="$1"
  local app_dir="$2"
  local port="$3"
  local log_file pid_file
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  stop_port_if_needed "$port" "$name"
  echo "[INFO] starting $name on 0.0.0.0:$port"
  : >"$log_file"
  local spawned_pid
  spawned_pid="$(
    if [[ "$name" == "local-connector-client-frontend" ]]; then
      export LOCAL_CONNECTOR_CORE_API_PROXY_TARGET="http://127.0.0.1:${LOCAL_CONNECTOR_CORE_PORT}"
      export LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT="$port"
    fi
    spawn_detached "$ROOT_DIR/$app_dir" "$log_file" npm run dev -- --host 0.0.0.0 --port "$port" --strictPort
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_port "$name" "$port" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
}

start_all() {
  need_cmd cargo
  need_cmd npm
  need_cmd curl
  need_cmd python3
  load_env_file "$ENV_FILE"
  export_local_env
  ensure_dirs
  start_infra
  wait_for_consul
  deregister_local_dev_services
  stop_docker_app_services
  cleanup_local_dev_processes
  deregister_local_dev_services
  register_local_dev_harness_service

  local item name service_name package health_path port bin app_dir
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name service_name package health_path port bin <<<"$item"
    start_backend "$name" "$service_name" "$package" "$health_path" "$port" "$bin"
  done
  start_local_connector_core
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name app_dir port <<<"$item"
    start_frontend "$name" "$app_dir" "$port"
  done
  print_urls
}

stop_all() {
  ensure_dirs
  deregister_local_dev_services
  local item name unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused port <<<"$item"
    stop_service_pid "$name"
    stop_port_if_needed "$port" "$name"
  done
  stop_service_pid "local-connector-client-core"
  stop_port_if_needed "$LOCAL_CONNECTOR_CORE_PORT" "local-connector-client-core"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    stop_service_pid "$name"
    stop_port_if_needed "$port" "$name"
  done
  cleanup_local_dev_processes
  deregister_local_dev_services
}

status_all() {
  ensure_dirs
  local item name port pid unused
  echo "[INFO] local dev stack status"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    pid="$(pid_for_port "$port")"
    if [[ -n "$pid" ]]; then
      printf '  %-36s port=%-5s running pid=%s\n' "$name" "$port" "$pid"
    else
      printf '  %-36s port=%-5s not listening\n' "$name" "$port"
    fi
  done
  pid="$(pid_for_port "$LOCAL_CONNECTOR_CORE_PORT")"
  if [[ -n "$pid" ]]; then
    printf '  %-36s port=%-5s running pid=%s\n' "local-connector-client-core" "$LOCAL_CONNECTOR_CORE_PORT" "$pid"
  else
    printf '  %-36s port=%-5s not listening\n' "local-connector-client-core" "$LOCAL_CONNECTOR_CORE_PORT"
  fi
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name _ port <<<"$item"
    pid="$(pid_for_port "$port")"
    if [[ -n "$pid" ]]; then
      printf '  %-36s port=%-5s running pid=%s\n' "$name" "$port" "$pid"
    else
      printf '  %-36s port=%-5s not listening\n' "$name" "$port"
    fi
  done
  echo
  echo "Logs: $LOG_DIR"
}

logs_for() {
  local name="${1:-}"
  if [[ -z "$name" ]]; then
    ls -1 "$LOG_DIR" 2>/dev/null || true
    echo
    echo "Usage: $0 logs <service-name>"
    return 0
  fi
  tail -f "$(log_file_for "$name")"
}

print_urls() {
  cat <<EOF

[OK] Local dev stack startup requested.

Main app:                 http://localhost:8088
Main backend:             http://localhost:3997
Harness:                  http://localhost:3000
User Service:             http://localhost:39191
Memory Engine:            http://localhost:4178
Task Runner:              http://localhost:39091
Project Management:       http://localhost:39211
Plugin Management:        http://localhost:39261
Sandbox Manager:          http://localhost:8096
Local Connector Service:  http://localhost:39230
Local Connector Client:   http://localhost:39233
Official Website:         http://localhost:39251

Status:  $0 status
Logs:    $0 logs <service-name>
Stop:    $0 down
EOF
}

case "$ACTION" in
  up|start)
    start_all
    ;;
  restart)
    stop_all
    start_all
    ;;
  down|stop)
    stop_all
    ;;
  status|ps)
    status_all
    ;;
  logs)
    shift || true
    logs_for "${1:-}"
    ;;
  *)
    echo "Usage: $0 [up|restart|down|status|logs <service-name>]" >&2
    exit 2
    ;;
esac
