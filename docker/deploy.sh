#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/compose.yml"
COMPOSE_BUILD_FILE="$SCRIPT_DIR/compose.build.yml"
ENV_FILE="${CHATOS_DOCKER_ENV_FILE:-$SCRIPT_DIR/.env}"
ACTION="${1:-up}"

compose_with_files() {
  local args=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --)
        shift
        break
        ;;
      *)
        args+=(-f "$1")
        shift
        ;;
    esac
  done
  if [[ -f "$ENV_FILE" ]]; then
    args+=(--env-file "$ENV_FILE")
  fi
  docker compose "${args[@]}" "$@"
}

compose() {
  compose_with_files "$COMPOSE_FILE" -- "$@"
}

compose_build() {
  compose_with_files "$COMPOSE_FILE" "$COMPOSE_BUILD_FILE" -- "$@"
}

compose_build_limited() {
  local build_parallel_limit="${CHATOS_DOCKER_BUILD_PARALLEL_LIMIT:-1}"
  (
    export COMPOSE_PARALLEL_LIMIT="$build_parallel_limit"
    compose_build "$@"
  )
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $cmd" >&2
    exit 1
  fi
}

ensure_docker_ready() {
  need_cmd docker
  if ! docker info >/dev/null 2>&1; then
    echo "[ERROR] Docker is not running or this user cannot access it." >&2
    exit 1
  fi
}

env_value() {
  local key="$1"
  local default_value="$2"
  local value=""
  if [[ -n "${!key:-}" ]]; then
    printf '%s' "${!key}"
    return 0
  fi
  if [[ -f "$ENV_FILE" ]]; then
    value="$(
      awk -F= -v key="$key" '
        /^[[:space:]]*(#|$)/ { next }
        {
          name = $1
          sub(/^[[:space:]]+/, "", name)
          sub(/[[:space:]]+$/, "", name)
          if (name == key) {
            sub(/^[^=]*=/, "", $0)
            sub(/\r$/, "", $0)
            sub(/^[[:space:]]+/, "", $0)
            sub(/[[:space:]]+$/, "", $0)
            gsub(/^"|"$/, "", $0)
            print $0
            exit
          }
        }
      ' "$ENV_FILE"
    )"
  fi
  if [[ -n "$value" ]]; then
    printf '%s' "$value"
  else
    printf '%s' "$default_value"
  fi
}

print_urls() {
  local frontend_port main_backend_port user_service_frontend_port
  local memory_engine_frontend_port task_runner_frontend_port project_service_frontend_port
  local sandbox_manager_frontend_port local_connector_service_port db_hub_frontend_port
  local official_website_frontend_port harness_port harness_ssh_port consul_port
  frontend_port="$(env_value FRONTEND_PORT 8088)"
  main_backend_port="$(env_value MAIN_BACKEND_PORT 3997)"
  consul_port="$(env_value CONSUL_HTTP_PORT 8500)"
  harness_port="$(env_value HARNESS_PORT 3000)"
  harness_ssh_port="$(env_value HARNESS_SSH_PORT 3022)"
  user_service_frontend_port="$(env_value USER_SERVICE_FRONTEND_PORT 39191)"
  memory_engine_frontend_port="$(env_value MEMORY_ENGINE_FRONTEND_PORT 4178)"
  task_runner_frontend_port="$(env_value TASK_RUNNER_FRONTEND_PORT 39091)"
  project_service_frontend_port="$(env_value PROJECT_SERVICE_FRONTEND_PORT 39211)"
  sandbox_manager_frontend_port="$(env_value SANDBOX_MANAGER_FRONTEND_PORT 8096)"
  local_connector_service_port="$(env_value LOCAL_CONNECTOR_SERVICE_PORT 39230)"
  db_hub_frontend_port="$(env_value DB_HUB_FRONTEND_PORT 5174)"
  official_website_frontend_port="$(env_value OFFICIAL_WEBSITE_FRONTEND_PORT 39251)"
  cat <<EOF

[OK] Chatos Docker stack is running.

Main app:                 http://localhost:${frontend_port}
Main backend:             http://localhost:${main_backend_port}
Consul:                   http://localhost:${consul_port}
Harness:                  http://localhost:${harness_port}
Harness SSH:              ssh://git@localhost:${harness_ssh_port}
User Service:             http://localhost:${user_service_frontend_port}
Memory Engine:            http://localhost:${memory_engine_frontend_port}
Task Runner:              http://localhost:${task_runner_frontend_port}
Project Management:       http://localhost:${project_service_frontend_port}
Sandbox Manager:          http://localhost:${sandbox_manager_frontend_port}
Local Connector Service:  http://localhost:${local_connector_service_port}
DB Connection Hub:        http://localhost:${db_hub_frontend_port}
Official Website:         http://localhost:${official_website_frontend_port}

Logs:    $0 logs
Status:  $0 ps
Stop:    $0 down
EOF
}

build_local_images() {
  echo "[INFO] building sandbox runtime image"
  compose_build_limited --profile image build sandbox-agent-image
  echo "[INFO] building Chatos cloud service images"
  local services=(
    user-service-backend
    memory-engine-backend
    project-management-backend
    local-connector-service-backend
    sandbox-manager-backend
    task-runner-backend
    chatos-backend
    db-connection-hub-backend
    official-website-backend
    chatos-frontend
    user-service-frontend
    memory-engine-frontend
    project-management-frontend
    task-runner-frontend
    sandbox-manager-frontend
    db-connection-hub-frontend
    official-website-frontend
  )
  local service
  for service in "${services[@]}"; do
    echo "[INFO] building image: $service"
    compose_build_limited build "$service"
  done
}

pull_prebuilt_images() {
  echo "[INFO] pulling prebuilt Chatos cloud images"
  compose --profile image pull
}

start_from_prebuilt_images() {
  pull_prebuilt_images
  echo "[INFO] starting Chatos cloud services from prebuilt images"
  compose up -d --no-build --remove-orphans
  print_urls
}

start_from_local_build() {
  build_local_images
  echo "[INFO] starting Chatos cloud services from local build"
  compose_build up -d --no-build --remove-orphans
  print_urls
}

start_default() {
  case "${CHATOS_DOCKER_MODE:-prebuilt}" in
    build|local|dev)
      start_from_local_build
      ;;
    prebuilt|pull|image|images)
      start_from_prebuilt_images
      ;;
    *)
      echo "[ERROR] unsupported CHATOS_DOCKER_MODE=${CHATOS_DOCKER_MODE}" >&2
      echo "        expected: prebuilt or build" >&2
      exit 2
      ;;
  esac
}

ensure_docker_ready
cd "$ROOT_DIR"

case "$ACTION" in
  up|start)
    start_default
    ;;
  restart)
    compose down --remove-orphans
    start_default
    ;;
  dev|local|build-up)
    start_from_local_build
    ;;
  restart-dev|restart-local)
    compose down --remove-orphans
    start_from_local_build
    ;;
  build)
    build_local_images
    ;;
  down|stop)
    compose down --remove-orphans
    ;;
  reset)
    compose down --remove-orphans --volumes
    ;;
  logs)
    shift || true
    compose logs -f "$@"
    ;;
  ps|status)
    compose ps
    ;;
  pull)
    pull_prebuilt_images
    ;;
  *)
    echo "Usage: $0 [up|restart|dev|restart-dev|build|down|reset|logs|ps|pull]" >&2
    echo "  up/restart use CHATOS_DOCKER_MODE=prebuilt by default." >&2
    echo "  set CHATOS_DOCKER_MODE=build or use dev/restart-dev for local image builds." >&2
    exit 2
    ;;
esac
