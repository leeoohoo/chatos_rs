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

INFRA_SERVICES=(mongodb harness)
DOCKER_APP_SERVICES=(
  user-service-backend
  memory-engine-backend
  project-management-backend
  local-connector-service-backend
  sandbox-manager-backend
  task-runner-backend
  chatos-backend
  official-website-backend
  chatos-frontend
  user-service-frontend
  memory-engine-frontend
  project-management-frontend
  task-runner-frontend
  sandbox-manager-frontend
  official-website-frontend
)

BACKEND_SERVICES=(
  "user-service-backend|user_service/backend/Cargo.toml|/api/health|39190"
  "memory-engine-backend|memory_engine/backend/Cargo.toml|/health|7081"
  "project-management-backend|project_management_service/backend/Cargo.toml|/api/health|39210"
  "local-connector-service-backend|local_connector_service/backend/Cargo.toml|/api/health|39230"
  "sandbox-manager-backend|sandbox_manager_service/backend/Cargo.toml|/health|8095"
  "task-runner-backend|task_runner_service/backend/Cargo.toml|/api/health|39090"
  "chatos-backend|chatos/backend/Cargo.toml|/health|3997"
  "official-website-backend|official_website_service/backend/Cargo.toml|/health|39250"
)

FRONTEND_SERVICES=(
  "chatos-frontend|chatos/frontend|8088"
  "user-service-frontend|user_service/frontend|39191"
  "memory-engine-frontend|memory_engine/frontend|4178"
  "project-management-frontend|project_management_service/frontend|39211"
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

pid_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n 1 || true
  fi
}

stop_pid() {
  local pid="$1"
  local name="$2"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  echo "[INFO] stopping $name (pid=$pid)"
  kill "$pid" 2>/dev/null || true
  sleep 1
  if kill -0 "$pid" 2>/dev/null; then
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
  pid="$(pid_for_port "$port")"
  if [[ -n "$pid" ]]; then
    stop_pid "$pid" "$name on port $port"
  fi
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

export_local_env() {
  local mongo_user mongo_password mongo_port
  mongo_user="$(env_value MONGODB_USER admin)"
  mongo_password="$(env_value MONGODB_PASSWORD admin)"
  mongo_port="$(env_value MONGODB_HOST_PORT 27018)"

  export CHATOS_ENV="${CHATOS_LOCAL_DEV_ENV:-local}"
  export CHATOS_SERVICE_RUNTIME_ENABLED="${CHATOS_LOCAL_DEV_SERVICE_RUNTIME_ENABLED:-false}"
  export CHATOS_SERVICE_DISCOVERY_MODE="${CHATOS_LOCAL_DEV_DISCOVERY_MODE:-static}"
  export CHATOS_CONSUL_HTTP_ADDR="${CHATOS_LOCAL_DEV_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"

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
  export TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-change_me_chatos_task_runner_secret}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}"
  export CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-chatos-local-connector-dev-secret}"
  export MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-dev-operator-token}"
  export SANDBOX_MANAGER_OPERATOR_TOKEN="${SANDBOX_MANAGER_OPERATOR_TOKEN:-chatos-sandbox-manager-dev-operator-token}"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID="${TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID:-task_runner}"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY="${TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY:-chatos-task-runner-sandbox-dev-key}"

  export USER_SERVICE_HOST="${USER_SERVICE_HOST:-127.0.0.1}"
  export USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
  export MEMORY_ENGINE_HOST="${MEMORY_ENGINE_HOST:-127.0.0.1}"
  export MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
  export PROJECT_SERVICE_HOST="${PROJECT_SERVICE_HOST:-127.0.0.1}"
  export PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
  export LOCAL_CONNECTOR_SERVICE_HOST="${LOCAL_CONNECTOR_SERVICE_HOST:-127.0.0.1}"
  export LOCAL_CONNECTOR_SERVICE_PORT="${LOCAL_CONNECTOR_SERVICE_PORT:-39230}"
  export SANDBOX_MANAGER_HOST="${SANDBOX_MANAGER_HOST:-127.0.0.1}"
  export SANDBOX_MANAGER_PORT="${SANDBOX_MANAGER_PORT:-8095}"
  export TASK_RUNNER_HOST="${TASK_RUNNER_HOST:-127.0.0.1}"
  export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-39090}"
  export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-39090}"
  export HOST="${HOST:-127.0.0.1}"
  export BACKEND_PORT="${BACKEND_PORT:-3997}"
  export OFFICIAL_WEBSITE_HOST="${OFFICIAL_WEBSITE_HOST:-127.0.0.1}"
  export OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-39250}"

  export USER_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/user_service?authSource=admin"
  export MEMORY_ENGINE_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin"
  export PROJECT_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/project_management_service?authSource=admin"
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
  export PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_BASE_URL"
  export PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  export PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
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

start_backend() {
  local name="$1"
  local manifest="$2"
  local health_path="$3"
  local port="$4"
  local log_file pid_file
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  stop_port_if_needed "$port" "$name"
  echo "[INFO] starting $name on 127.0.0.1:$port"
  (
    cd "$ROOT_DIR"
    exec cargo run --manifest-path "$manifest"
  ) >"$log_file" 2>&1 &
  echo "$!" >"$pid_file"
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
  (
    cd "$ROOT_DIR"
    export LOCAL_CONNECTOR_CORE_API_PORT="$LOCAL_CONNECTOR_CORE_PORT"
    export LOCAL_CONNECTOR_CLOUD_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
    export LOCAL_CONNECTOR_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
    exec cargo run --manifest-path local_connector_client/core/Cargo.toml
  ) >"$log_file" 2>&1 &
  echo "$!" >"$pid_file"
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
  (
    cd "$ROOT_DIR/$app_dir"
    if [[ "$name" == "local-connector-client-frontend" ]]; then
      export LOCAL_CONNECTOR_CORE_API_PROXY_TARGET="http://127.0.0.1:${LOCAL_CONNECTOR_CORE_PORT}"
      export LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT="$port"
    fi
    exec npm run dev -- --host 0.0.0.0 --port "$port" --strictPort
  ) >"$log_file" 2>&1 &
  echo "$!" >"$pid_file"
  wait_for_port "$name" "$port" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
}

start_all() {
  need_cmd cargo
  need_cmd npm
  need_cmd curl
  load_env_file "$ENV_FILE"
  export_local_env
  ensure_dirs
  start_infra
  stop_docker_app_services

  local item name package health_path port app_dir
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name package health_path port <<<"$item"
    start_backend "$name" "$package" "$health_path" "$port"
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
  local item name unused
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused <<<"$item"
    stop_service_pid "$name"
  done
  stop_service_pid "local-connector-client-core"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused <<<"$item"
    stop_service_pid "$name"
  done
}

status_all() {
  ensure_dirs
  local item name port pid
  echo "[INFO] local dev stack status"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name _ _ port <<<"$item"
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
