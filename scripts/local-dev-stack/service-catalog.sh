#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

ALL_DOCKER_APP_SERVICES=(
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  memory-engine-worker
  plugin-management-backend
  local-connector-service-backend
  mcp-management-service-backend
  chatos-backend
  official-website-backend
  admin-console-frontend
  official-website-frontend
)

stack_service_definition() {
  case "$1" in
    configuration-center-backend)
      printf '%s\n' "configuration-center-backend|configuration-center|config_center_service/backend/Cargo.toml|/health|39270|config_center_service_backend|-"
      ;;
    user-service-backend)
      printf '%s\n' "user-service-backend|user-service|user_service/backend/Cargo.toml|/api/health|39190|user_service_backend|-"
      ;;
    memory-engine-backend)
      printf '%s\n' "memory-engine-backend|memory-engine|memory_engine/backend/Cargo.toml|/health|7081|memory_engine|MEMORY_ENGINE_API_ENABLED=true MEMORY_ENGINE_WORKER_ENABLED=false"
      ;;
    memory-engine-worker)
      printf '%s\n' "memory-engine-worker|memory-engine|memory_engine/backend/Cargo.toml|-|-|memory_engine|MEMORY_ENGINE_API_ENABLED=false MEMORY_ENGINE_WORKER_ENABLED=true"
      ;;
    plugin-management-backend)
      printf '%s\n' "plugin-management-backend|plugin-management-service|plugin_management_service/backend/Cargo.toml|/api/health|39260|plugin_management_service_backend|-"
      ;;
    local-connector-service-backend)
      printf '%s\n' "local-connector-service-backend|local-connector-service|local_connector_service/backend/Cargo.toml|/api/health|39230|local_connector_service_backend|-"
      ;;
    mcp-management-service-backend)
      printf '%s\n' "mcp-management-service-backend|mcp-management-service|mcp_management_service/backend/Cargo.toml|/health|39280|mcp_management_service_backend|-"
      ;;
    chatos-backend)
      printf '%s\n' "chatos-backend|chatos-backend|chatos/backend/Cargo.toml|/health|3997|chat_app_server_rs|-"
      ;;
    official-website-backend)
      printf '%s\n' "official-website-backend|official-website|official_website_service/backend/Cargo.toml|/health|39250|official_website_service_backend|-"
      ;;
    *)
      echo "[ERROR] unknown host-side service in stack profile: $1" >&2
      return 2
      ;;
  esac
}

resolve_stack_service_profile() {
  BACKEND_SERVICES=()
  DOCKER_APP_SERVICES=("${ALL_DOCKER_APP_SERVICES[@]}")

  local name definition
  for name in "${BACKEND_SERVICE_NAMES[@]}"; do
    definition="$(stack_service_definition "$name")" || return
    BACKEND_SERVICES+=("$definition")
  done
}
