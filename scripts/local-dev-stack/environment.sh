#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

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
  export CONFIG_CENTER_HOST="${CONFIG_CENTER_HOST:-0.0.0.0}"
  export CONFIG_CENTER_PORT="${CONFIG_CENTER_PORT:-39270}"
  export CONFIG_CENTER_BASE_URL="${CONFIG_CENTER_BASE_URL:-http://127.0.0.1:${CONFIG_CENTER_PORT}}"
  export CONFIG_CENTER_INTERNAL_API_SECRET="${CONFIG_CENTER_INTERNAL_API_SECRET:-change_me_configuration_center_internal_secret}"
  export AGENT_MAX_ITERATIONS="${AGENT_MAX_ITERATIONS:-600}"
  export CONFIG_CENTER_CONSUL_REQUIRED="${CONFIG_CENTER_CONSUL_REQUIRED:-false}"
  export VITE_CONFIG_CENTER_URL="${VITE_CONFIG_CENTER_URL:-http://localhost:39271}"

  export OPENAI_API_KEY="${OPENAI_API_KEY:-}"
  export OPENAI_BASE_URL="${OPENAI_BASE_URL:-https://api.openai.com/v1}"
  export CHATOS_OBJECT_STORAGE_ENDPOINT="${CHATOS_OBJECT_STORAGE_ENDPOINT:-https://oss.jgoool.com}"
  export CHATOS_OBJECT_STORAGE_REGION="${CHATOS_OBJECT_STORAGE_REGION:-us-east-1}"
  export CHATOS_OBJECT_STORAGE_BUCKET="${CHATOS_OBJECT_STORAGE_BUCKET:-chatos-attachments}"
  export CHATOS_OBJECT_STORAGE_ACCESS_KEY="${CHATOS_OBJECT_STORAGE_ACCESS_KEY:-${MINIO_ACCESS_KEY:-${MINIO_ROOT_USER:-}}}"
  export CHATOS_OBJECT_STORAGE_SECRET_KEY="${CHATOS_OBJECT_STORAGE_SECRET_KEY:-${MINIO_SECRET_KEY:-${MINIO_ROOT_PASSWORD:-}}}"
  export CHATOS_OBJECT_STORAGE_FORCE_PATH_STYLE="${CHATOS_OBJECT_STORAGE_FORCE_PATH_STYLE:-true}"
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
  export USER_SERVICE_INTERNAL_API_SECRET="${USER_SERVICE_INTERNAL_API_SECRET:-}"
  export PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET="${PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET:-change_me_project_service_user_service_secret}"
  export TASK_RUNNER_INTERNAL_API_SECRET="${TASK_RUNNER_INTERNAL_API_SECRET:-}"
  export PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET="${PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_project_service_task_runner_secret}"
  export CHATOS_TASK_RUNNER_INTERNAL_API_SECRET="${CHATOS_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_chatos_task_runner_internal_secret}"
  export PLUGIN_MANAGEMENT_INTERNAL_API_SECRET="${PLUGIN_MANAGEMENT_INTERNAL_API_SECRET:-change_me_plugin_management_internal_secret}"
  export PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET="${PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_plugin_management_memory_engine_secret}"
  export TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-change_me_chatos_task_runner_secret}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}"
  export CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET="${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET:-change_me_chatos_project_service_secret}"
  export TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET="${TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET:-change_me_task_runner_project_service_secret}"
  export PROJECT_SERVICE_SELF_INTERNAL_API_SECRET="${PROJECT_SERVICE_SELF_INTERNAL_API_SECRET:-change_me_project_service_self_secret}"
  export CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_chatos_local_connector_secret}"
  export TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_task_runner_local_connector_secret}"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_project_service_local_connector_secret}"
  export MEMORY_ENGINE_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${MEMORY_ENGINE_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_memory_engine_local_connector_secret}"
  export CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET="${CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_chatos_memory_engine_secret}"
  export TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET="${TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_task_runner_memory_engine_secret}"
  export PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET="${PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_project_service_memory_engine_secret}"
  export USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET="${USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_user_service_memory_engine_secret}"
  export LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET="${LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_local_connector_memory_engine_secret}"
  export MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-dev-operator-token}"
  export SANDBOX_MANAGER_OPERATOR_TOKEN="${SANDBOX_MANAGER_OPERATOR_TOKEN:-chatos-sandbox-manager-dev-operator-token}"
  export TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET="${TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET:-change_me_task_runner_sandbox_manager_secret}"
  export PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET="${PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET:-change_me_project_service_sandbox_manager_secret}"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID="task-runner"
  export TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY="$TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID="project-service"
  export PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY="$PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET"

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
  export CONFIG_CENTER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/configuration_center?authSource=admin"
  export CONFIG_CENTER_MONGODB_DATABASE="${CONFIG_CENTER_MONGODB_DATABASE:-configuration_center}"
  export PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE="${PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE:-plugin_management_service}"
  export LOCAL_CONNECTOR_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/local_connector_service?authSource=admin"
  export SANDBOX_MANAGER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/sandbox_manager_service?authSource=admin"
  export TASK_RUNNER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/task_runner_service?authSource=admin"
  export LEGACY_AUTH_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin"

  export MEMORY_ENGINE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export CONFIG_CENTER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export MEMORY_ENGINE_BASE_URL="http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1"
  export TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export CHATOS_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET="$PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET="$PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_BASE_URL"
  export PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  export PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
  export PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME="$CHATOS_ADMIN_USERNAME"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD="$CHATOS_ADMIN_PASSWORD"
  export PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES="${PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES:-true}"
  export LOCAL_CONNECTOR_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export LOCAL_CONNECTOR_PUBLIC_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export LOCAL_CONNECTOR_INTERNAL_API_SECRET="${LOCAL_CONNECTOR_INTERNAL_API_SECRET:-}"
  export SANDBOX_MANAGER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export SANDBOX_MANAGER_SYSTEM_CLIENT_ID="${SANDBOX_MANAGER_SYSTEM_CLIENT_ID:-}"
  export SANDBOX_MANAGER_SYSTEM_CLIENT_KEY="${SANDBOX_MANAGER_SYSTEM_CLIENT_KEY:-}"
  export SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE="${SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE:-published}"
  export SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT="${SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT:-true}"
  export SANDBOX_MANAGER_DOCKER_CONFIG="${SANDBOX_MANAGER_DOCKER_CONFIG:-$STATE_DIR/docker-public-config}"
  if [[ -z "${SANDBOX_MANAGER_DOCKER_HOST:-}" && -S "$HOME/.docker/run/docker.sock" ]]; then
    export SANDBOX_MANAGER_DOCKER_HOST="unix://$HOME/.docker/run/docker.sock"
  else
    export SANDBOX_MANAGER_DOCKER_HOST="${SANDBOX_MANAGER_DOCKER_HOST:-${DOCKER_HOST:-}}"
  fi
  export TASK_RUNNER_STORE_MODE="${TASK_RUNNER_STORE_MODE:-mongo}"
  export TASK_RUNNER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET="$TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET"
  export TASK_RUNNER_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_BASE_URL"
  export TASK_RUNNER_SANDBOX_MANAGER_BASE_URL="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  export TASK_RUNNER_SANDBOX_BASE_IMAGE_ID="${TASK_RUNNER_SANDBOX_BASE_IMAGE_ID:-dev-java21}"
  export TASK_RUNNER_CHATOS_CALLBACK_URL="http://127.0.0.1:${BACKEND_PORT}/api/agent/chat/task-runner/callback"
  export CHATOS_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET="$CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET"
  export CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export HARNESS_PROVISIONING_ENABLED="${CHATOS_LOCAL_DEV_HARNESS_PROVISIONING_ENABLED:-true}"
  export HARNESS_BASE_URL="${CHATOS_LOCAL_DEV_HARNESS_BASE_URL:-http://127.0.0.1:3000}"
  export OFFICIAL_WEBSITE_STATUS_HOST="${OFFICIAL_WEBSITE_STATUS_HOST:-127.0.0.1}"
}

ensure_dirs() {
  mkdir -p "$LOG_DIR" "$PID_DIR" "$STATE_DIR/task-runner" "$STATE_DIR/chatos" "$STATE_DIR/sandboxes" "$STATE_DIR/docker-public-config"
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
    "configuration-center",
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
