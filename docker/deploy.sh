#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/compose.yml"
COMPOSE_BUILD_FILE="$SCRIPT_DIR/compose.build.yml"
ENV_FILE="${CHATOS_DOCKER_ENV_FILE:-$SCRIPT_DIR/.env}"
EXTRA_COMPOSE_FILES="${CHATOS_DOCKER_EXTRA_COMPOSE_FILES:-${CHATOS_DOCKER_EXTRA_COMPOSE_FILE:-}}"
ACTION="${1:-up}"

LOCAL_BUILD_SERVICES=(
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

compose_with_files() {
  local args=()
  local extra_file
  local -a extra_files=()
  local configured_extra_files="$EXTRA_COMPOSE_FILES"
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
  if [[ -z "$configured_extra_files" ]]; then
    configured_extra_files="$(env_value CHATOS_DOCKER_EXTRA_COMPOSE_FILES "")"
  fi
  if [[ -z "$configured_extra_files" ]]; then
    configured_extra_files="$(env_value CHATOS_DOCKER_EXTRA_COMPOSE_FILE "")"
  fi
  if [[ -n "$configured_extra_files" ]]; then
    IFS=':' read -r -a extra_files <<< "$configured_extra_files"
    for extra_file in "${extra_files[@]}"; do
      if [[ -n "$extra_file" ]]; then
        args+=(-f "$extra_file")
      fi
    done
  fi
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

print_build_services() {
  printf '%s\n' sandbox-agent-image
  printf '%s\n' "${LOCAL_BUILD_SERVICES[@]}"
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

env_flag_enabled() {
  local key="$1"
  local default_value="$2"
  case "$(env_value "$key" "$default_value")" in
    1|true|TRUE|True|yes|YES|Yes|on|ON|On)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

validate_production_secrets() {
  local environment
  environment="$(env_value CHATOS_ENV "$(env_value NODE_ENV local)")"
  case "${environment,,}" in
    production|prod)
      ;;
    *)
      return 0
      ;;
  esac

  local failures=0
  local key value default_value
  while IFS='|' read -r key default_value; do
    value="$(env_value "$key" "$default_value")"
    if [[ -z "$value" || "$value" == "$default_value" || ${#value} -lt 16 ]]; then
      echo "[ERROR] production secret $key is missing, uses the development default, or is shorter than 16 characters" >&2
      failures=1
    fi
  done <<'EOF'
MONGODB_PASSWORD|admin
CHATOS_ADMIN_PASSWORD|admin123456
HARNESS_ADMIN_PASSWORD|admin123456
AUTH_JWT_SECRET|dev-only-change-me-please
USER_SERVICE_JWT_SECRET|change_me_user_service_secret
PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET|change_me_project_service_user_service_secret
PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET|change_me_project_service_task_runner_secret
CHATOS_TASK_RUNNER_INTERNAL_API_SECRET|change_me_chatos_task_runner_internal_secret
PLUGIN_MANAGEMENT_INTERNAL_API_SECRET|change_me_plugin_management_internal_secret
PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET|change_me_plugin_management_task_runner_secret
PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_plugin_management_project_service_secret
PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET|change_me_plugin_management_local_connector_secret
TASK_RUNNER_CHATOS_CALLBACK_SECRET|change_me_chatos_task_runner_secret
CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_chatos_project_service_secret
TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_task_runner_project_service_secret
PROJECT_SERVICE_SELF_INTERNAL_API_SECRET|change_me_project_service_self_secret
CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_chatos_local_connector_secret
TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_task_runner_local_connector_secret
PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_project_service_local_connector_secret
MEMORY_ENGINE_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_memory_engine_local_connector_secret
CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_chatos_memory_engine_secret
TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_task_runner_memory_engine_secret
PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_project_service_memory_engine_secret
USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_user_service_memory_engine_secret
LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_local_connector_memory_engine_secret
SANDBOX_MANAGER_AGENT_TOKEN_SECRET|chatos-sandbox-agent-dev-secret
TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET|change_me_task_runner_sandbox_manager_secret
PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET|change_me_project_service_sandbox_manager_secret
EOF

  if (( failures > 0 )); then
    echo "[ERROR] refusing to start the production stack with insecure credentials" >&2
    exit 2
  fi
}

print_urls() {
  local frontend_port main_backend_port user_service_frontend_port
  local memory_engine_frontend_port task_runner_frontend_port project_service_frontend_port
  local plugin_management_frontend_port sandbox_manager_frontend_port local_connector_service_port
  local official_website_frontend_port harness_port harness_ssh_host harness_ssh_port consul_port
  frontend_port="$(env_value FRONTEND_PORT 8088)"
  main_backend_port="$(env_value MAIN_BACKEND_PORT 3997)"
  consul_port="$(env_value CONSUL_HTTP_PORT 8500)"
  harness_port="$(env_value HARNESS_PORT 3000)"
  harness_ssh_host="$(env_value HARNESS_SSH_PUBLIC_HOST "$(env_value HARNESS_SSH_HOST localhost)")"
  harness_ssh_port="$(env_value HARNESS_SSH_PORT 3022)"
  user_service_frontend_port="$(env_value USER_SERVICE_FRONTEND_PORT 39191)"
  memory_engine_frontend_port="$(env_value MEMORY_ENGINE_FRONTEND_PORT 4178)"
  task_runner_frontend_port="$(env_value TASK_RUNNER_FRONTEND_PORT 39091)"
  project_service_frontend_port="$(env_value PROJECT_SERVICE_FRONTEND_PORT 39211)"
  plugin_management_frontend_port="$(env_value PLUGIN_MANAGEMENT_FRONTEND_PORT 39261)"
  sandbox_manager_frontend_port="$(env_value SANDBOX_MANAGER_FRONTEND_PORT 8096)"
  local_connector_service_port="$(env_value LOCAL_CONNECTOR_SERVICE_PORT 39230)"
  official_website_frontend_port="$(env_value OFFICIAL_WEBSITE_FRONTEND_PORT 39251)"
  cat <<EOF

[OK] Chat OS Docker stack is running.

Main app:                 http://localhost:${frontend_port}
Main backend:             http://localhost:${main_backend_port}
Consul:                   http://localhost:${consul_port}
Harness:                  http://localhost:${harness_port}
Harness SSH:              ssh://git@${harness_ssh_host}:${harness_ssh_port}
User Service:             http://localhost:${user_service_frontend_port}
Memory Engine:            http://localhost:${memory_engine_frontend_port}
Task Runner:              http://localhost:${task_runner_frontend_port}
Project Management:       http://localhost:${project_service_frontend_port}
Plugin Management:        http://localhost:${plugin_management_frontend_port}
Sandbox Manager:          http://localhost:${sandbox_manager_frontend_port}
Local Connector Service:  http://localhost:${local_connector_service_port}
Official Website:         http://localhost:${official_website_frontend_port}

Logs:    $0 logs
Status:  $0 ps
Stop:    $0 down
EOF
}

build_local_images() {
  local services=("$@")
  if [[ ${#services[@]} -eq 0 ]]; then
    echo "[INFO] building sandbox runtime image"
    compose_build_limited --profile image build sandbox-agent-image
    echo "[INFO] building Chat OS cloud service images"
    services=("${LOCAL_BUILD_SERVICES[@]}")
  else
    echo "[INFO] building selected Chat OS service images"
  fi

  local service
  for service in "${services[@]}"; do
    echo "[INFO] building image: $service"
    if [[ "$service" == "sandbox-agent-image" ]]; then
      compose_build_limited --profile image build "$service"
    else
      compose_build_limited build "$service"
    fi
  done
}

pull_prebuilt_images() {
  if [[ $# -gt 0 ]]; then
    echo "[INFO] pulling selected prebuilt Chat OS images"
    compose --profile image pull "$@"
  else
    echo "[INFO] pulling prebuilt Chat OS cloud images"
    compose --profile image pull
  fi
}

clean_dangling_images() {
  echo "[INFO] removing dangling Docker images (<none>:<none>)"
  docker image prune -f
}

clean_dangling_images_if_enabled() {
  if ! env_flag_enabled CHATOS_DOCKER_PRUNE_DANGLING_IMAGES true; then
    return 0
  fi
  if [[ -z "$(docker image ls -q --filter dangling=true)" ]]; then
    return 0
  fi
  clean_dangling_images
}

start_from_prebuilt_images() {
  pull_prebuilt_images "$@"
  echo "[INFO] starting Chat OS cloud services from prebuilt images"
  compose up -d --no-build --remove-orphans "$@"
  clean_dangling_images_if_enabled
  print_urls
}

start_from_local_build() {
  build_local_images "$@"
  echo "[INFO] starting Chat OS cloud services from local build"
  compose_build up -d --no-build --remove-orphans "$@"
  clean_dangling_images_if_enabled
  print_urls
}

start_without_refresh() {
  echo "[INFO] starting Chat OS cloud services without pulling or building images"
  compose up -d --no-build --pull never --remove-orphans "$@"
  print_urls
}

restart_without_refresh() {
  if [[ $# -gt 0 ]]; then
    echo "[INFO] recreating selected Chat OS services without pulling or building images"
    compose up -d --no-build --pull never --no-deps --force-recreate "$@"
    print_urls
  else
    compose down --remove-orphans
    start_without_refresh
  fi
}

rebuild_services() {
  local services=("$@")
  build_local_images "${services[@]}"
  if [[ ${#services[@]} -eq 0 ]]; then
    echo "[INFO] starting Chat OS cloud services from rebuilt local images"
    compose_build up -d --no-build --remove-orphans
  else
    echo "[INFO] recreating selected Chat OS services from rebuilt local images"
    compose_build up -d --no-build --pull never --no-deps --force-recreate "${services[@]}"
  fi
  clean_dangling_images_if_enabled
  print_urls
}

start_default() {
  case "${CHATOS_DOCKER_MODE:-prebuilt}" in
    build|local|dev)
      start_from_local_build "$@"
      ;;
    prebuilt|pull|image|images)
      start_from_prebuilt_images "$@"
      ;;
    *)
      echo "[ERROR] unsupported CHATOS_DOCKER_MODE=${CHATOS_DOCKER_MODE}" >&2
      echo "        expected: prebuilt or build" >&2
      exit 2
      ;;
  esac
}

if [[ "$ACTION" == "build-services" ]]; then
  print_build_services
  exit 0
fi

ensure_docker_ready
cd "$ROOT_DIR"

case "$ACTION" in
  up|start|restart|fast|quick|up-fast|up-quick|restart-fast|restart-quick|dev|local|build-up|restart-dev|restart-local|rebuild)
    validate_production_secrets
    ;;
esac

case "$ACTION" in
  up|start)
    shift || true
    start_default "$@"
    ;;
  restart)
    shift || true
    compose down --remove-orphans
    start_default "$@"
    ;;
  fast|quick|up-fast|up-quick)
    shift || true
    start_without_refresh "$@"
    ;;
  restart-fast|restart-quick)
    shift || true
    restart_without_refresh "$@"
    ;;
  dev|local|build-up)
    shift || true
    start_from_local_build "$@"
    ;;
  restart-dev|restart-local)
    shift || true
    compose down --remove-orphans
    start_from_local_build "$@"
    ;;
  rebuild)
    shift || true
    rebuild_services "$@"
    ;;
  build)
    shift || true
    build_local_images "$@"
    clean_dangling_images_if_enabled
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
    shift || true
    pull_prebuilt_images "$@"
    ;;
  clean-images|prune-images)
    clean_dangling_images
    ;;
  services)
    compose_build config --services
    ;;
  build-services)
    print_build_services
    ;;
  *)
    echo "Usage: $0 [up|fast|restart|restart-fast|dev|restart-dev|rebuild|build|down|reset|logs|ps|pull|clean-images|services|build-services] [service...]" >&2
    echo "  up/restart pull prebuilt images by default." >&2
    echo "  fast/restart-fast reuse existing images and skip pull/build." >&2
    echo "  dev/restart-dev build local images; rebuild builds only the given build-service names." >&2
    echo "  clean-images removes dangling <none>:<none> images." >&2
    echo "  service names can be listed with: $0 services" >&2
    echo "  buildable service names can be listed with: $0 build-services" >&2
    exit 2
    ;;
esac
