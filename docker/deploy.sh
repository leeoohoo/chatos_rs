#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/compose.yml"
COMPOSE_PLATFORM_FILE="$SCRIPT_DIR/compose.platform.yml"
COMPOSE_BUILD_FILE="$SCRIPT_DIR/compose.build.yml"
ENV_FILE="${CHATOS_DOCKER_ENV_FILE:-$SCRIPT_DIR/.env}"
EXTRA_COMPOSE_FILES="${CHATOS_DOCKER_EXTRA_COMPOSE_FILES:-${CHATOS_DOCKER_EXTRA_COMPOSE_FILE:-}}"
ACTION="${1:-up}"

LOCAL_BUILD_SERVICES=(
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  project-management-backend
  plugin-management-backend
  local-connector-service-backend
  mcp-management-service-backend
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
  compose_with_files "$COMPOSE_FILE" "$COMPOSE_PLATFORM_FILE" -- "$@"
}

compose_build() {
  compose_with_files "$COMPOSE_FILE" "$COMPOSE_PLATFORM_FILE" "$COMPOSE_BUILD_FILE" -- "$@"
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

ensure_cloud_network() {
  local network_name
  network_name="$(env_value CHATOS_DOCKER_NETWORK chatos-cloud)"
  if docker network inspect "$network_name" >/dev/null 2>&1; then
    return 0
  fi
  echo "[INFO] creating shared Docker network: $network_name"
  docker network create "$network_name" >/dev/null
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

is_production_environment() {
  local environment
  environment="$(env_value CHATOS_ENV "$(env_value NODE_ENV local)")"
  case "$environment" in
    production|prod|PRODUCTION|PROD|Production|Prod)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

validate_https_origin() {
  local key="$1"
  local origin="$2"
  local authority lowercase_authority host port
  local hostname_pattern='^([A-Za-z0-9]|[A-Za-z0-9][A-Za-z0-9.-]*[A-Za-z0-9])(:([0-9]{1,5}))?$'
  local ipv6_pattern='^(\[[0-9A-Fa-f:.]+\])(:([0-9]{1,5}))?$'

  if [[ "$origin" != https://* ]]; then
    echo "[ERROR] $key must use an exact https:// origin" >&2
    return 1
  fi
  authority="${origin#https://}"
  if [[ -z "$authority" || "$authority" == *[/?#@]* || "$authority" =~ [[:space:]] ]]; then
    echo "[ERROR] $key must not contain credentials, path, query, fragment, or whitespace" >&2
    return 1
  fi
  lowercase_authority="$(printf '%s' "$authority" | tr '[:upper:]' '[:lower:]')"
  if [[ "$authority" != "$lowercase_authority" ]]; then
    echo "[ERROR] $key must use a lowercase canonical authority" >&2
    return 1
  fi
  if [[ "$authority" =~ $ipv6_pattern ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[3]:-}"
  elif [[ "$authority" =~ $hostname_pattern ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[3]:-}"
    if [[ "$host" == *..* || "$host" == *.-* || "$host" == *-. ]]; then
      echo "[ERROR] $key contains an invalid hostname" >&2
      return 1
    fi
  else
    echo "[ERROR] $key must be a canonical HTTPS origin with an optional explicit port" >&2
    return 1
  fi
  if [[ -n "$port" ]] && (( 10#$port < 1 || 10#$port > 65535 )); then
    echo "[ERROR] $key contains an invalid port" >&2
    return 1
  fi
  if [[ "$port" == "443" ]]; then
    echo "[ERROR] $key must omit the default HTTPS port" >&2
    return 1
  fi
}

validate_plugin_ui_origins() {
  local require_pair="${1:-auto}"
  local parent_origin resource_origin failures=0
  parent_origin="$(env_value CHATOS_PLUGIN_UI_PARENT_ORIGIN "")"
  resource_origin="$(env_value CHATOS_PLUGIN_UI_RESOURCE_ORIGIN "")"

  if [[ -z "$parent_origin" && -z "$resource_origin" ]]; then
    if [[ "$require_pair" == "true" ]] || is_production_environment; then
      echo "[ERROR] production requires CHATOS_PLUGIN_UI_PARENT_ORIGIN and CHATOS_PLUGIN_UI_RESOURCE_ORIGIN" >&2
      return 1
    fi
    return 0
  fi
  if [[ -z "$parent_origin" || -z "$resource_origin" ]]; then
    echo "[ERROR] CHATOS_PLUGIN_UI_PARENT_ORIGIN and CHATOS_PLUGIN_UI_RESOURCE_ORIGIN must be configured together" >&2
    return 1
  fi
  validate_https_origin CHATOS_PLUGIN_UI_PARENT_ORIGIN "$parent_origin" || failures=1
  validate_https_origin CHATOS_PLUGIN_UI_RESOURCE_ORIGIN "$resource_origin" || failures=1
  if [[ "$parent_origin" == "$resource_origin" ]]; then
    echo "[ERROR] Plugin UI parent and resource origins must be different" >&2
    failures=1
  fi
  (( failures == 0 ))
}

validate_production_secrets() {
  local failures=0
  validate_plugin_ui_origins || failures=1
  if ! is_production_environment; then
    if (( failures > 0 )); then
      exit 2
    fi
    return 0
  fi

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
CONFIG_CENTER_CHATOS_BACKEND_CALLER_SIGNING_SECRET|change_me_config_center_chatos_backend_signing_secret
CONFIG_CENTER_LOCAL_CONNECTOR_SERVICE_CALLER_SIGNING_SECRET|change_me_config_center_local_connector_signing_secret
CONFIG_CENTER_MCP_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET|change_me_config_center_mcp_management_signing_secret
CONFIG_CENTER_MEMORY_ENGINE_CALLER_SIGNING_SECRET|change_me_config_center_memory_engine_signing_secret
CONFIG_CENTER_OFFICIAL_WEBSITE_CALLER_SIGNING_SECRET|change_me_config_center_official_website_signing_secret
CONFIG_CENTER_PLUGIN_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET|change_me_config_center_plugin_management_signing_secret
CONFIG_CENTER_PROJECT_SERVICE_CALLER_SIGNING_SECRET|change_me_config_center_project_service_signing_secret
CONFIG_CENTER_SANDBOX_MANAGER_CALLER_SIGNING_SECRET|change_me_config_center_sandbox_manager_signing_secret
CONFIG_CENTER_TASK_RUNNER_CALLER_SIGNING_SECRET|change_me_config_center_task_runner_signing_secret
CONFIG_CENTER_USER_SERVICE_CALLER_SIGNING_SECRET|change_me_config_center_user_service_signing_secret
PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET|change_me_plugin_management_task_runner_secret
PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_plugin_management_project_service_secret
PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET|change_me_plugin_management_local_connector_secret
PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_plugin_management_memory_engine_secret
PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET|change_me_plugin_management_mcp_management_secret
CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_chatos_project_service_secret
TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_task_runner_project_service_secret
PROJECT_SERVICE_SELF_INTERNAL_API_SECRET|change_me_project_service_self_secret
MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET|change_me_mcp_management_project_service_secret
MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET|change_me_mcp_management_task_runner_secret
CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_chatos_local_connector_secret
TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_task_runner_local_connector_secret
PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_project_service_local_connector_secret
MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET|change_me_mcp_management_local_connector_secret
MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET|change_me_configuration_center_mcp_management_secret
MCP_MANAGEMENT_RUNTIME_GRANT_SECRET|change_me_mcp_management_runtime_grant_secret
CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_chatos_memory_engine_secret
TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_task_runner_memory_engine_secret
PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_project_service_memory_engine_secret
USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET|change_me_user_service_memory_engine_secret
SANDBOX_MANAGER_AGENT_TOKEN_SECRET|chatos-sandbox-agent-dev-secret
TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET|change_me_task_runner_sandbox_manager_secret
PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET|change_me_project_service_sandbox_manager_secret
MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET|change_me_mcp_management_sandbox_manager_secret
EOF

  if (( failures > 0 )); then
    echo "[ERROR] refusing to start the production stack with insecure credentials" >&2
    exit 2
  fi
}

ensure_config_center_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value CONFIG_CENTER_MTLS_DIR ./secrets/config-center-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    local-connector-service.identity.pem \
    mcp-management-service.identity.pem \
    memory-engine.identity.pem \
    official-website.identity.pem \
    plugin-management-service.identity.pem \
    project-service.identity.pem \
    sandbox-manager.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-config-center-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Configuration Center mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -CAfile "$resolved_dir/ca.crt" "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Configuration Center server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    local-connector-service.identity.pem \
    mcp-management-service.identity.pem \
    memory-engine.identity.pem \
    official-website.identity.pem \
    plugin-management-service.identity.pem \
    project-service.identity.pem \
    sandbox-manager.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Configuration Center client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Configuration Center client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_mcp_management_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value MCP_MANAGEMENT_MTLS_DIR ./secrets/mcp-management-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    configuration-center.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-mcp-management-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] MCP Management mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] MCP Management server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    configuration-center.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] MCP Management client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] MCP Management client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_task_runner_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value TASK_RUNNER_MTLS_DIR ./secrets/task-runner-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos.identity.pem \
    mcp-management-service.identity.pem \
    project-service.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-task-runner-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Task Runner mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Task Runner server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos.identity.pem \
    mcp-management-service.identity.pem \
    project-service.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Task Runner client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Task Runner client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_project_service_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value PROJECT_SERVICE_MTLS_DIR ./secrets/project-service-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-project-service-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Project Service mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Project Service server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] Project Service server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Project Service client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Project Service client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_chatos_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value CHATOS_MTLS_DIR ./secrets/chatos-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-chatos-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] ChatOS mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] ChatOS server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] ChatOS server key is unreadable" >&2
    return 1
  fi
  for required_file in task-runner.identity.pem mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] ChatOS client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] ChatOS client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_local_connector_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value LOCAL_CONNECTOR_MTLS_DIR ./secrets/local-connector-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-local-connector-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Local Connector mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Local Connector server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] Local Connector server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Local Connector client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Local Connector client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_user_service_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value USER_SERVICE_MTLS_DIR ./secrets/user-service-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in ca.crt server.crt server.key project-service.identity.pem; do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-user-service-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] User Service mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] User Service server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] User Service server key is unreadable" >&2
    return 1
  fi
  if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/project-service.identity.pem" >/dev/null; then
    echo "[ERROR] User Service Project Service client certificate is invalid" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/project-service.identity.pem" -noout >/dev/null 2>&1; then
    echo "[ERROR] User Service Project Service identity has no readable private key" >&2
    return 1
  fi
}

ensure_plugin_management_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value PLUGIN_MANAGEMENT_MTLS_DIR ./secrets/plugin-management-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    local-connector-service.identity.pem \
    memory-engine.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-plugin-management-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Plugin Management mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Plugin Management server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] Plugin Management server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    project-service.identity.pem \
    local-connector-service.identity.pem \
    memory-engine.identity.pem \
    mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Plugin Management client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Plugin Management client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_sandbox_manager_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir required_file failures=0
  configured_dir="$(env_value SANDBOX_MANAGER_MTLS_DIR ./secrets/sandbox-manager-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in ca.crt server.crt server.key task-runner.identity.pem \
    project-service.identity.pem mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done
  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-sandbox-manager-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Sandbox Manager mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null || return 1
  openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1 || return 1
  for required_file in task-runner.identity.pem project-service.identity.pem \
    mcp-management-service.identity.pem
  do
    openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null || return 1
    openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1 || return 1
  done
}

ensure_memory_engine_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value MEMORY_ENGINE_MTLS_DIR ./secrets/memory-engine-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    configuration-center.identity.pem \
    project-service.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-memory-engine-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Memory Engine mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Memory Engine server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    configuration-center.identity.pem \
    project-service.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Memory Engine client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Memory Engine client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

print_urls() {
  local frontend_port main_backend_port user_service_frontend_port
  local memory_engine_frontend_port task_runner_frontend_port project_service_frontend_port
  local plugin_management_frontend_port sandbox_manager_frontend_port local_connector_service_port
  local official_website_frontend_port config_center_frontend_port mcp_management_port
  local harness_port harness_ssh_host harness_ssh_port consul_port
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
  mcp_management_port="$(env_value MCP_MANAGEMENT_PORT 39280)"
  official_website_frontend_port="$(env_value OFFICIAL_WEBSITE_FRONTEND_PORT 39251)"
  config_center_frontend_port="$(env_value CONFIG_CENTER_FRONTEND_PORT 39271)"
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
Configuration Center:     http://localhost:${config_center_frontend_port}
Sandbox Manager:          http://localhost:${sandbox_manager_frontend_port}
Local Connector Service:  http://localhost:${local_connector_service_port}
MCP Management Service:   http://localhost:${mcp_management_port}
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

sandbox_manager_requested() {
  if [[ $# -eq 0 ]]; then
    return 0
  fi
  local service
  for service in "$@"; do
    if [[ "$service" == "sandbox-manager-backend" ]]; then
      return 0
    fi
  done
  return 1
}

ensure_sandbox_manager_docker_runtime() {
  local buildkit_image="moby/buildkit:buildx-stable-1"
  if ! docker image inspect "$buildkit_image" >/dev/null 2>&1; then
    echo "[INFO] pulling Sandbox Manager BuildKit runtime image"
    docker pull "$buildkit_image"
  fi
  echo "[INFO] starting Sandbox Manager Docker socket proxy"
  compose up -d --no-build --pull missing sandbox-docker-socket-proxy
}

ensure_sandbox_manager_docker_runtime_if_requested() {
  if sandbox_manager_requested "$@"; then
    ensure_sandbox_manager_docker_runtime
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

clean_build_cache() {
  local max_used_space
  local reserved_space
  local timeout
  local prune_help
  max_used_space="$(env_value CHATOS_DOCKER_BUILD_CACHE_MAX_USED_SPACE 32gb)"
  reserved_space="$(env_value CHATOS_DOCKER_BUILD_CACHE_RESERVED_SPACE 8gb)"
  timeout="$(env_value CHATOS_DOCKER_BUILD_CACHE_TIMEOUT 180s)"
  prune_help="$(docker builder prune --help 2>&1 || true)"
  if grep -q -- '--max-used-space' <<<"$prune_help"; then
    echo "[INFO] enforcing Docker BuildKit cache limit: max=$max_used_space reserved=$reserved_space"
    docker builder prune --force --all \
      --max-used-space "$max_used_space" \
      --reserved-space "$reserved_space" \
      --timeout "$timeout"
  elif grep -q -- '--keep-storage' <<<"$prune_help"; then
    echo "[INFO] enforcing legacy Docker build cache reserve: $reserved_space"
    docker builder prune --force --all --keep-storage "$reserved_space"
  else
    echo "[WARN] Docker builder cache pruning is unsupported by this Docker CLI; skipping"
  fi
}

clean_build_cache_if_enabled() {
  if ! env_flag_enabled CHATOS_DOCKER_PRUNE_BUILD_CACHE true; then
    return 0
  fi
  clean_build_cache
}

clean_docker_artifacts_if_enabled() {
  clean_dangling_images_if_enabled
  clean_build_cache_if_enabled
}

start_from_prebuilt_images() {
  pull_prebuilt_images "$@"
  ensure_sandbox_manager_docker_runtime_if_requested "$@"
  echo "[INFO] starting Chat OS cloud services from prebuilt images"
  compose up -d --no-build --remove-orphans "$@"
  clean_docker_artifacts_if_enabled
  print_urls
}

start_from_local_build() {
  build_local_images "$@"
  ensure_sandbox_manager_docker_runtime_if_requested "$@"
  echo "[INFO] starting Chat OS cloud services from local build"
  compose_build up -d --no-build --remove-orphans "$@"
  clean_docker_artifacts_if_enabled
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
  ensure_sandbox_manager_docker_runtime_if_requested "${services[@]}"
  if [[ ${#services[@]} -eq 0 ]]; then
    echo "[INFO] starting Chat OS cloud services from rebuilt local images"
    compose_build up -d --no-build --remove-orphans
  else
    echo "[INFO] recreating selected Chat OS services from rebuilt local images"
    compose_build up -d --no-build --pull never --no-deps --force-recreate "${services[@]}"
  fi
  clean_docker_artifacts_if_enabled
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

if [[ "$ACTION" == "validate-plugin-ui-origin" ]]; then
  if validate_plugin_ui_origins true; then
    echo "[OK] Plugin UI parent/resource origin configuration is valid."
    exit 0
  fi
  exit 2
fi

ensure_docker_ready
cd "$ROOT_DIR"

case "$ACTION" in
  up|start|restart|fast|quick|up-fast|up-quick|restart-fast|restart-quick|dev|local|build-up|restart-dev|restart-local|rebuild)
    validate_production_secrets
    ensure_config_center_mtls_material
    ensure_mcp_management_mtls_material
    ensure_task_runner_mtls_material
    ensure_project_service_mtls_material
    ensure_chatos_mtls_material
    ensure_local_connector_mtls_material
    ensure_user_service_mtls_material
    ensure_plugin_management_mtls_material
    ensure_sandbox_manager_mtls_material
    ensure_memory_engine_mtls_material
    ensure_cloud_network
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
    clean_docker_artifacts_if_enabled
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
  clean-build-cache|prune-build-cache)
    clean_build_cache
    ;;
  services)
    compose_build config --services
    ;;
  build-services)
    print_build_services
    ;;
  *)
    echo "Usage: $0 [up|fast|restart|restart-fast|dev|restart-dev|rebuild|build|down|reset|logs|ps|pull|clean-images|clean-build-cache|services|build-services|validate-plugin-ui-origin] [service...]" >&2
    echo "  up/restart pull prebuilt images by default." >&2
    echo "  fast/restart-fast reuse existing images and skip pull/build." >&2
    echo "  dev/restart-dev build local images; rebuild builds only the given build-service names." >&2
    echo "  clean-images removes dangling <none>:<none> images." >&2
    echo "  clean-build-cache enforces the configured BuildKit cache size limit." >&2
    echo "  service names can be listed with: $0 services" >&2
    echo "  buildable service names can be listed with: $0 build-services" >&2
    echo "  Plugin UI origins can be checked without Docker using: $0 validate-plugin-ui-origin" >&2
    exit 2
    ;;
esac
