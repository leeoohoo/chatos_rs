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
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  project-management-backend
  plugin-management-backend
  local-connector-service-backend
  sandbox-manager-backend
  task-runner-backend
  chatos-backend
  official-website-backend
  configuration-center-frontend
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
  "configuration-center-backend|configuration-center|config_center_service/backend/Cargo.toml|/health|39270|config_center_service_backend"
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
  "configuration-center-frontend|config_center_service/frontend|39271"
  "chatos-frontend|chatos/frontend|8088"
  "user-service-frontend|user_service/frontend|39191"
  "memory-engine-frontend|memory_engine/frontend|4178"
  "project-management-frontend|project_management_service/frontend|39211"
  "plugin-management-frontend|plugin_management_service/frontend|39261"
  "task-runner-frontend|task_runner_service/frontend|39091"
  "sandbox-manager-frontend|sandbox_manager_service/frontend|8096"
  "official-website-frontend|official_website_service/frontend|39251"
)

# shellcheck source=local-dev-stack/support.sh
source "$SCRIPT_DIR/local-dev-stack/support.sh"
# shellcheck source=local-dev-stack/environment.sh
source "$SCRIPT_DIR/local-dev-stack/environment.sh"
# shellcheck source=local-dev-stack/services.sh
source "$SCRIPT_DIR/local-dev-stack/services.sh"

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
