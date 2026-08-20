#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

resolve_local_dev_host_address() {
  local bridge_address
  if [[ -n "${CHATOS_LOCAL_DEV_HOST_ADDRESS:-}" ]]; then
    printf '%s\n' "$CHATOS_LOCAL_DEV_HOST_ADDRESS"
    return 0
  fi
  if [[ "$(uname -s)" == "Darwin" ]] && command -v ifconfig >/dev/null 2>&1; then
    bridge_address="$(ifconfig bridge100 2>/dev/null | awk '/inet / { print $2; exit }')"
    if [[ -n "$bridge_address" ]]; then
      printf '%s\n' "$bridge_address"
      return 0
    fi
  fi
  printf '%s\n' "host.docker.internal"
}

configure_local_dev_cloud_browser_runtime() {
  local platform package_os agent_browser chrome
  case "$(uname -s):$(uname -m)" in
    Darwin:arm64) platform="macos-arm64"; package_os="macos" ;;
    Darwin:x86_64) platform="macos-x64"; package_os="macos" ;;
    Linux:x86_64) platform="linux-x64"; package_os="linux" ;;
    Linux:aarch64|Linux:arm64) platform="linux-arm64"; package_os="linux" ;;
    *) platform=""; package_os="" ;;
  esac

  if [[ -z "${AGENT_BROWSER_BIN:-}" ]] && command -v agent-browser >/dev/null 2>&1; then
    export AGENT_BROWSER_BIN="$(command -v agent-browser)"
  fi
  if [[ -z "${AGENT_BROWSER_BIN:-}" && -n "$platform" ]]; then
    for agent_browser in \
      "$ROOT_DIR/bundled-tools/$platform/agent-browser" \
      "$ROOT_DIR/local_connector_client/.package/$package_os/bundled-tools/$platform/agent-browser"; do
      if [[ -x "$agent_browser" ]]; then
        export AGENT_BROWSER_BIN="$agent_browser"
        break
      fi
    done
  fi

  if [[ -z "${AGENT_BROWSER_EXECUTABLE_PATH:-}" && -n "$platform" ]]; then
    for chrome in \
      "$ROOT_DIR/bundled-tools/$platform/browser/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" \
      "$ROOT_DIR/local_connector_client/.package/$package_os/bundled-tools/$platform/browser/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" \
      "/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" \
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
      "/usr/bin/chromium" \
      "/usr/bin/chromium-browser"; do
      if [[ -x "$chrome" ]]; then
        export AGENT_BROWSER_EXECUTABLE_PATH="$chrome"
        break
      fi
    done
  fi
}

export_local_env() {
  local mongo_user mongo_password mongo_port host_address
  mongo_user="$(env_value MONGODB_USER admin)"
  mongo_password="$(env_value MONGODB_PASSWORD admin)"
  mongo_port="$(env_value MONGODB_HOST_PORT 27018)"
  host_address="$(resolve_local_dev_host_address)"
  export CHATOS_LOCAL_DEV_HOST_ADDRESS="$host_address"
  configure_local_dev_cloud_browser_runtime

  export CHATOS_ENV="${CHATOS_LOCAL_DEV_ENV:-local}"
  export CHATOS_SERVICE_RUNTIME_ENABLED="${CHATOS_LOCAL_DEV_SERVICE_RUNTIME_ENABLED:-true}"
  export CHATOS_SERVICE_DISCOVERY_MODE="${CHATOS_LOCAL_DEV_DISCOVERY_MODE:-consul,static}"
  export CHATOS_CONSUL_HTTP_ADDR="${CHATOS_LOCAL_DEV_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  export CHATOS_SERVICE_ADDRESS="${CHATOS_LOCAL_DEV_SERVICE_ADDRESS:-127.0.0.1}"
  export CHATOS_SERVICE_CHECK_ADDRESS="${CHATOS_LOCAL_DEV_SERVICE_CHECK_ADDRESS:-$host_address}"
  export CONFIG_CENTER_HOST="${CONFIG_CENTER_HOST:-0.0.0.0}"
  export CONFIG_CENTER_PORT="${CONFIG_CENTER_PORT:-39270}"
  export CONFIG_CENTER_INTERNAL_MTLS_PORT="${CONFIG_CENTER_INTERNAL_MTLS_PORT:-39272}"
  export CONFIG_CENTER_BASE_URL="${CONFIG_CENTER_BASE_URL:-https://127.0.0.1:${CONFIG_CENTER_INTERNAL_MTLS_PORT}}"
  export CONFIG_CENTER_MTLS_DIR="${CONFIG_CENTER_MTLS_DIR:-$STATE_DIR/config-center-mtls}"
  export CONFIG_CENTER_MTLS_SERVER_CERT_PATH="${CONFIG_CENTER_MTLS_SERVER_CERT_PATH:-$CONFIG_CENTER_MTLS_DIR/server.crt}"
  export CONFIG_CENTER_MTLS_SERVER_KEY_PATH="${CONFIG_CENTER_MTLS_SERVER_KEY_PATH:-$CONFIG_CENTER_MTLS_DIR/server.key}"
  export CONFIG_CENTER_MTLS_CLIENT_CA_CERT_PATH="${CONFIG_CENTER_MTLS_CLIENT_CA_CERT_PATH:-$CONFIG_CENTER_MTLS_DIR/ca.crt}"
  export CONFIG_CENTER_MTLS_CA_CERT_PATH="${CONFIG_CENTER_MTLS_CA_CERT_PATH:-$CONFIG_CENTER_MTLS_DIR/ca.crt}"
  export MCP_MANAGEMENT_MTLS_DIR="${MCP_MANAGEMENT_MTLS_DIR:-$STATE_DIR/mcp-management-mtls}"
  export MCP_MANAGEMENT_MTLS_SERVER_CERT_PATH="${MCP_MANAGEMENT_MTLS_SERVER_CERT_PATH:-$MCP_MANAGEMENT_MTLS_DIR/server.crt}"
  export MCP_MANAGEMENT_MTLS_SERVER_KEY_PATH="${MCP_MANAGEMENT_MTLS_SERVER_KEY_PATH:-$MCP_MANAGEMENT_MTLS_DIR/server.key}"
  export MCP_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH="${MCP_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH:-$MCP_MANAGEMENT_MTLS_DIR/ca.crt}"
  export MCP_MANAGEMENT_MTLS_CA_CERT_PATH="${MCP_MANAGEMENT_MTLS_CA_CERT_PATH:-$MCP_MANAGEMENT_MTLS_DIR/ca.crt}"
  export TASK_RUNNER_MTLS_DIR="${TASK_RUNNER_MTLS_DIR:-$STATE_DIR/task-runner-mtls}"
  export TASK_RUNNER_MTLS_SERVER_CERT_PATH="${TASK_RUNNER_MTLS_SERVER_CERT_PATH:-$TASK_RUNNER_MTLS_DIR/server.crt}"
  export TASK_RUNNER_MTLS_SERVER_KEY_PATH="${TASK_RUNNER_MTLS_SERVER_KEY_PATH:-$TASK_RUNNER_MTLS_DIR/server.key}"
  export TASK_RUNNER_MTLS_CLIENT_CA_CERT_PATH="${TASK_RUNNER_MTLS_CLIENT_CA_CERT_PATH:-$TASK_RUNNER_MTLS_DIR/ca.crt}"
  export TASK_RUNNER_MTLS_CA_CERT_PATH="${TASK_RUNNER_MTLS_CA_CERT_PATH:-$TASK_RUNNER_MTLS_DIR/ca.crt}"
  export PROJECT_SERVICE_MTLS_DIR="${PROJECT_SERVICE_MTLS_DIR:-$STATE_DIR/project-service-mtls}"
  export PROJECT_SERVICE_MTLS_SERVER_CERT_PATH="${PROJECT_SERVICE_MTLS_SERVER_CERT_PATH:-$PROJECT_SERVICE_MTLS_DIR/server.crt}"
  export PROJECT_SERVICE_MTLS_SERVER_KEY_PATH="${PROJECT_SERVICE_MTLS_SERVER_KEY_PATH:-$PROJECT_SERVICE_MTLS_DIR/server.key}"
  export PROJECT_SERVICE_MTLS_CLIENT_CA_CERT_PATH="${PROJECT_SERVICE_MTLS_CLIENT_CA_CERT_PATH:-$PROJECT_SERVICE_MTLS_DIR/ca.crt}"
  export PROJECT_SERVICE_MTLS_CA_CERT_PATH="${PROJECT_SERVICE_MTLS_CA_CERT_PATH:-$PROJECT_SERVICE_MTLS_DIR/ca.crt}"
  export CHATOS_MTLS_DIR="${CHATOS_MTLS_DIR:-$STATE_DIR/chatos-mtls}"
  export CHATOS_MTLS_SERVER_CERT_PATH="${CHATOS_MTLS_SERVER_CERT_PATH:-$CHATOS_MTLS_DIR/server.crt}"
  export CHATOS_MTLS_SERVER_KEY_PATH="${CHATOS_MTLS_SERVER_KEY_PATH:-$CHATOS_MTLS_DIR/server.key}"
  export CHATOS_MTLS_CLIENT_CA_CERT_PATH="${CHATOS_MTLS_CLIENT_CA_CERT_PATH:-$CHATOS_MTLS_DIR/ca.crt}"
  export CHATOS_MTLS_CA_CERT_PATH="${CHATOS_MTLS_CA_CERT_PATH:-$CHATOS_MTLS_DIR/ca.crt}"
  export LOCAL_CONNECTOR_MTLS_DIR="${LOCAL_CONNECTOR_MTLS_DIR:-$STATE_DIR/local-connector-mtls}"
  export LOCAL_CONNECTOR_MTLS_SERVER_CERT_PATH="${LOCAL_CONNECTOR_MTLS_SERVER_CERT_PATH:-$LOCAL_CONNECTOR_MTLS_DIR/server.crt}"
  export LOCAL_CONNECTOR_MTLS_SERVER_KEY_PATH="${LOCAL_CONNECTOR_MTLS_SERVER_KEY_PATH:-$LOCAL_CONNECTOR_MTLS_DIR/server.key}"
  export LOCAL_CONNECTOR_MTLS_CLIENT_CA_CERT_PATH="${LOCAL_CONNECTOR_MTLS_CLIENT_CA_CERT_PATH:-$LOCAL_CONNECTOR_MTLS_DIR/ca.crt}"
  export LOCAL_CONNECTOR_MTLS_CA_CERT_PATH="${LOCAL_CONNECTOR_MTLS_CA_CERT_PATH:-$LOCAL_CONNECTOR_MTLS_DIR/ca.crt}"
  export PLUGIN_MANAGEMENT_MTLS_DIR="${PLUGIN_MANAGEMENT_MTLS_DIR:-$STATE_DIR/plugin-management-mtls}"
  export PLUGIN_MANAGEMENT_MTLS_SERVER_CERT_PATH="${PLUGIN_MANAGEMENT_MTLS_SERVER_CERT_PATH:-$PLUGIN_MANAGEMENT_MTLS_DIR/server.crt}"
  export PLUGIN_MANAGEMENT_MTLS_SERVER_KEY_PATH="${PLUGIN_MANAGEMENT_MTLS_SERVER_KEY_PATH:-$PLUGIN_MANAGEMENT_MTLS_DIR/server.key}"
  export PLUGIN_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH="${PLUGIN_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH:-$PLUGIN_MANAGEMENT_MTLS_DIR/ca.crt}"
  export PLUGIN_MANAGEMENT_MTLS_CA_CERT_PATH="${PLUGIN_MANAGEMENT_MTLS_CA_CERT_PATH:-$PLUGIN_MANAGEMENT_MTLS_DIR/ca.crt}"
  export USER_SERVICE_MTLS_DIR="${USER_SERVICE_MTLS_DIR:-$STATE_DIR/user-service-mtls}"
  export USER_SERVICE_MTLS_SERVER_CERT_PATH="${USER_SERVICE_MTLS_SERVER_CERT_PATH:-$USER_SERVICE_MTLS_DIR/server.crt}"
  export USER_SERVICE_MTLS_SERVER_KEY_PATH="${USER_SERVICE_MTLS_SERVER_KEY_PATH:-$USER_SERVICE_MTLS_DIR/server.key}"
  export USER_SERVICE_MTLS_CLIENT_CA_CERT_PATH="${USER_SERVICE_MTLS_CLIENT_CA_CERT_PATH:-$USER_SERVICE_MTLS_DIR/ca.crt}"
  export USER_SERVICE_MTLS_CA_CERT_PATH="${USER_SERVICE_MTLS_CA_CERT_PATH:-$USER_SERVICE_MTLS_DIR/ca.crt}"
  export MEMORY_ENGINE_MTLS_DIR="${MEMORY_ENGINE_MTLS_DIR:-$STATE_DIR/memory-engine-mtls}"
  export MEMORY_ENGINE_MTLS_CA_CERT_PATH="${MEMORY_ENGINE_MTLS_CA_CERT_PATH:-$MEMORY_ENGINE_MTLS_DIR/ca.crt}"
  export CONFIG_CENTER_CHATOS_BACKEND_CALLER_SIGNING_SECRET="${CONFIG_CENTER_CHATOS_BACKEND_CALLER_SIGNING_SECRET:-change_me_config_center_chatos_backend_signing_secret}"
  export CONFIG_CENTER_LOCAL_CONNECTOR_SERVICE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_LOCAL_CONNECTOR_SERVICE_CALLER_SIGNING_SECRET:-change_me_config_center_local_connector_signing_secret}"
  export CONFIG_CENTER_MCP_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_MCP_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET:-change_me_config_center_mcp_management_signing_secret}"
  export CONFIG_CENTER_MEMORY_ENGINE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_MEMORY_ENGINE_CALLER_SIGNING_SECRET:-change_me_config_center_memory_engine_signing_secret}"
  export CONFIG_CENTER_OFFICIAL_WEBSITE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_OFFICIAL_WEBSITE_CALLER_SIGNING_SECRET:-change_me_config_center_official_website_signing_secret}"
  export CONFIG_CENTER_PLUGIN_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_PLUGIN_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET:-change_me_config_center_plugin_management_signing_secret}"
  export CONFIG_CENTER_PROJECT_SERVICE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_PROJECT_SERVICE_CALLER_SIGNING_SECRET:-change_me_config_center_project_service_signing_secret}"
  export CONFIG_CENTER_TASK_RUNNER_CALLER_SIGNING_SECRET="${CONFIG_CENTER_TASK_RUNNER_CALLER_SIGNING_SECRET:-change_me_config_center_task_runner_signing_secret}"
  export CONFIG_CENTER_USER_SERVICE_CALLER_SIGNING_SECRET="${CONFIG_CENTER_USER_SERVICE_CALLER_SIGNING_SECRET:-change_me_config_center_user_service_signing_secret}"
  export AGENT_MAX_ITERATIONS="${AGENT_MAX_ITERATIONS:-600}"
  export CHATOS_AI_CONNECT_TIMEOUT_SECS="${CHATOS_AI_CONNECT_TIMEOUT_SECS:-15}"
  export CHATOS_AI_READ_TIMEOUT_SECS="${CHATOS_AI_READ_TIMEOUT_SECS:-300}"
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
  if [[ -z "${CADVISOR_DOCKER_SOCKET:-}" && -S "$HOME/.docker/run/docker.sock" ]]; then
    export CADVISOR_DOCKER_SOCKET="$HOME/.docker/run/docker.sock"
  else
    export CADVISOR_DOCKER_SOCKET="${CADVISOR_DOCKER_SOCKET:-/var/run/docker.sock}"
  fi
  export MONGODB_USER="$mongo_user"
  export MONGODB_PASSWORD="$mongo_password"
  export MONGODB_HOST="${MONGODB_HOST:-127.0.0.1}"
  export MONGODB_PORT="$mongo_port"
  export MONGODB_AUTH_SOURCE="${MONGODB_AUTH_SOURCE:-admin}"
  local mongodb_query="authSource=admin&replicaSet=rs0&directConnection=true"
  export MONGODB_CONNECTION_STRING="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/chatos?${mongodb_query}"
  export MONGODB_DB="${MONGODB_DB:-chatos}"

  export CHATOS_ADMIN_USERNAME="${CHATOS_ADMIN_USERNAME:-admin}"
  export CHATOS_ADMIN_PASSWORD="${CHATOS_ADMIN_PASSWORD:-admin123456}"
  export CHATOS_ADMIN_DISPLAY_NAME="${CHATOS_ADMIN_DISPLAY_NAME:-System Admin}"
  export AUTH_JWT_SECRET="${AUTH_JWT_SECRET:-dev-only-change-me-please}"
  export USER_SERVICE_JWT_SECRET="${USER_SERVICE_JWT_SECRET:-change_me_user_service_secret}"
  export PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET="${PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET:-change_me_project_service_user_service_secret}"
  export PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET="${PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_project_service_task_runner_secret}"
  export CHATOS_TASK_RUNNER_INTERNAL_API_SECRET="${CHATOS_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_chatos_task_runner_internal_secret}"
  export PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET="${PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_plugin_management_memory_engine_secret}"
  export PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET="${PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET:-change_me_plugin_management_mcp_management_secret}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}"
  export CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET="${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET:-change_me_chatos_project_service_secret}"
  export TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET="${TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET:-change_me_task_runner_project_service_secret}"
  export PROJECT_SERVICE_SELF_INTERNAL_API_SECRET="${PROJECT_SERVICE_SELF_INTERNAL_API_SECRET:-change_me_project_service_self_secret}"
  export MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET="${MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET:-change_me_mcp_management_project_service_secret}"
  export MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET="${MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_mcp_management_task_runner_secret}"
  export USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET="${USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET:-change_me_user_service_task_runner_secret}"
  export MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS="${MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS:-7200000}"
  export MCP_MANAGEMENT_PROJECT_SERVICE_TOOL_TIMEOUT_MS="${MCP_MANAGEMENT_PROJECT_SERVICE_TOOL_TIMEOUT_MS:-7200000}"
  export MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS="${MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS:-86700000}"
  export MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET="${MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET:-change_me_mcp_management_chatos_secret}"
  export MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS="${MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS:-86700000}"
  export MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS="${MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS:-7200000}"
  export CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_chatos_local_connector_secret}"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_project_service_local_connector_secret}"
  export MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET="${MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET:-change_me_mcp_management_local_connector_secret}"
  export MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET="${MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET:-change_me_configuration_center_mcp_management_secret}"
  export MCP_MANAGEMENT_RUNTIME_GRANT_SECRET="${MCP_MANAGEMENT_RUNTIME_GRANT_SECRET:-change_me_mcp_management_runtime_grant_secret}"
  export MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET="${MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET:-change_me_mcp_management_runtime_session_encryption_secret}"
  export MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS="${MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS:-7200}"
  export TASK_RUNNER_MCP_MANAGEMENT_TOOL_TIMEOUT_MS="${TASK_RUNNER_MCP_MANAGEMENT_TOOL_TIMEOUT_MS:-7200000}"
  export TASK_RUNNER_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS="${TASK_RUNNER_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS:-86700000}"
  export CHATOS_MCP_MANAGEMENT_TOOL_TIMEOUT_MS="${CHATOS_MCP_MANAGEMENT_TOOL_TIMEOUT_MS:-7200000}"
  export CHATOS_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS="${CHATOS_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS:-86700000}"
  export CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET="${CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_chatos_memory_engine_secret}"
  export TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET="${TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_task_runner_memory_engine_secret}"
  export USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET="${USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET:-change_me_user_service_memory_engine_secret}"

  export USER_SERVICE_HOST="${USER_SERVICE_HOST:-0.0.0.0}"
  export USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
  export USER_SERVICE_INTERNAL_MTLS_PORT="${USER_SERVICE_INTERNAL_MTLS_PORT:-39192}"
  export MEMORY_ENGINE_HOST="${MEMORY_ENGINE_HOST:-0.0.0.0}"
  export MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
  export MEMORY_ENGINE_INTERNAL_MTLS_PORT="${MEMORY_ENGINE_INTERNAL_MTLS_PORT:-7083}"
  export PROJECT_SERVICE_HOST="${PROJECT_SERVICE_HOST:-0.0.0.0}"
  export PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
  export PROJECT_SERVICE_INTERNAL_MTLS_PORT="${PROJECT_SERVICE_INTERNAL_MTLS_PORT:-39212}"
  export PLUGIN_MANAGEMENT_SERVICE_HOST="${PLUGIN_MANAGEMENT_SERVICE_HOST:-0.0.0.0}"
  export PLUGIN_MANAGEMENT_SERVICE_PORT="${PLUGIN_MANAGEMENT_SERVICE_PORT:-39260}"
  export PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT="${PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT:-39262}"
  export PLUGIN_MANAGEMENT_SERVICE_URL="${PLUGIN_MANAGEMENT_SERVICE_URL:-http://127.0.0.1:${PLUGIN_MANAGEMENT_SERVICE_PORT}}"
  export PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL="${PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL:-https://127.0.0.1:${PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT}}"
  export LOCAL_CONNECTOR_SERVICE_HOST="${LOCAL_CONNECTOR_SERVICE_HOST:-0.0.0.0}"
  export LOCAL_CONNECTOR_SERVICE_PORT="${LOCAL_CONNECTOR_SERVICE_PORT:-39230}"
  export LOCAL_CONNECTOR_INTERNAL_MTLS_PORT="${LOCAL_CONNECTOR_INTERNAL_MTLS_PORT:-39231}"
  export MCP_MANAGEMENT_HOST="${MCP_MANAGEMENT_HOST:-0.0.0.0}"
  export MCP_MANAGEMENT_PORT="${MCP_MANAGEMENT_PORT:-39280}"
  export MCP_MANAGEMENT_INTERNAL_MTLS_PORT="${MCP_MANAGEMENT_INTERNAL_MTLS_PORT:-39282}"
  export TASK_RUNNER_HOST="${TASK_RUNNER_HOST:-0.0.0.0}"
  export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-39090}"
  export TASK_RUNNER_INTERNAL_MTLS_PORT="${TASK_RUNNER_INTERNAL_MTLS_PORT:-39092}"
  export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-39090}"
  export HOST="${HOST:-0.0.0.0}"
  export BACKEND_PORT="${BACKEND_PORT:-3997}"
  export CHATOS_INTERNAL_MTLS_PORT="${CHATOS_INTERNAL_MTLS_PORT:-3999}"
  export USER_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/user_service?${mongodb_query}"
  export MEMORY_ENGINE_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin?${mongodb_query}"
  export PROJECT_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/project_management_service?${mongodb_query}"
  export PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/plugin_management_service?${mongodb_query}"
  export CONFIG_CENTER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/configuration_center?${mongodb_query}"
  export CONFIG_CENTER_MONGODB_DATABASE="${CONFIG_CENTER_MONGODB_DATABASE:-configuration_center}"
  export PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE="${PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE:-plugin_management_service}"
  export LOCAL_CONNECTOR_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/local_connector_service?${mongodb_query}"
  export TASK_RUNNER_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/task_runner_service?${mongodb_query}"
  export MCP_MANAGEMENT_DATABASE_URL="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/mcp_management_service?${mongodb_query}"
  export LEGACY_AUTH_MONGODB_URI="mongodb://${mongo_user}:${mongo_password}@127.0.0.1:${mongo_port}/admin?${mongodb_query}"
  export LEGACY_AUTH_MONGODB_DATABASE="${LEGACY_AUTH_MONGODB_DATABASE:-legacy_auth}"

  export MEMORY_ENGINE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export CONFIG_CENTER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export MEMORY_ENGINE_BASE_URL="http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1"
  export MEMORY_ENGINE_INTERNAL_BASE_URL="https://127.0.0.1:${MEMORY_ENGINE_INTERNAL_MTLS_PORT}/api/memory-engine/v1"
  export CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_INTERNAL_BASE_URL"
  export USER_SERVICE_MEMORY_ENGINE_BASE_URL="$CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL"
  export CHATOS_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_INTERNAL_BASE_URL"
  export TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export CHATOS_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export CHATOS_TASK_RUNNER_INTERNAL_BASE_URL="https://127.0.0.1:${TASK_RUNNER_INTERNAL_MTLS_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL="https://127.0.0.1:${USER_SERVICE_INTERNAL_MTLS_PORT}"
  export PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET="$PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_TASK_RUNNER_BASE_URL="https://127.0.0.1:${TASK_RUNNER_INTERNAL_MTLS_PORT}"
  export PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET="$PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET"
  export PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL="https://127.0.0.1:${LOCAL_CONNECTOR_INTERNAL_MTLS_PORT}"
  export CHATOS_RUN_WORKSPACE_ROOT="${CHATOS_RUN_WORKSPACE_ROOT:-$STATE_DIR/run-workspaces}"
  export PROJECT_SERVICE_SYNC_SECRET="$CHATOS_PROJECT_SERVICE_SYNC_SECRET"
  export PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL="http://127.0.0.1:${TASK_RUNNER_PORT}"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME="$CHATOS_ADMIN_USERNAME"
  export PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD="$CHATOS_ADMIN_PASSWORD"
  export PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES="${PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES:-true}"
  export LOCAL_CONNECTOR_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export LOCAL_CONNECTOR_PUBLIC_BASE_URL="http://127.0.0.1:${LOCAL_CONNECTOR_SERVICE_PORT}"
  export LOCAL_CONNECTOR_INTERNAL_API_SECRET="${LOCAL_CONNECTOR_INTERNAL_API_SECRET:-}"
  export MCP_MANAGEMENT_PUBLIC_BASE_URL="http://127.0.0.1:${MCP_MANAGEMENT_PORT}"
  export MCP_MANAGEMENT_SERVICE_BASE_URL="https://127.0.0.1:${MCP_MANAGEMENT_INTERNAL_MTLS_PORT}"
  export CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL="$MCP_MANAGEMENT_SERVICE_BASE_URL"
  export MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL="$PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL"
  export MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL="https://127.0.0.1:${PROJECT_SERVICE_INTERNAL_MTLS_PORT}"
  export MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL="https://127.0.0.1:${TASK_RUNNER_INTERNAL_MTLS_PORT}"
  export USER_SERVICE_TASK_RUNNER_BASE_URL="https://127.0.0.1:${TASK_RUNNER_INTERNAL_MTLS_PORT}"
  export MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL="https://127.0.0.1:${CHATOS_INTERNAL_MTLS_PORT}"
  export MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL="https://127.0.0.1:${LOCAL_CONNECTOR_INTERNAL_MTLS_PORT}"
  export TASK_RUNNER_STORE_MODE="${TASK_RUNNER_STORE_MODE:-mongo}"
  # Do not inject a local default for TASK_RUNNER_WORKER_CONCURRENCY here.
  # Task Runner loads the authoritative value from Configuration Center at
  # startup unless the operator explicitly exports an environment override.
  export TASK_RUNNER_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL="https://127.0.0.1:${PROJECT_SERVICE_INTERNAL_MTLS_PORT}"
  export TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET="$TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET"
  export TASK_RUNNER_MEMORY_ENGINE_BASE_URL="$MEMORY_ENGINE_INTERNAL_BASE_URL"
  export TASK_RUNNER_CHATOS_CALLBACK_URL="https://127.0.0.1:${CHATOS_INTERNAL_MTLS_PORT}/api/agent/chat/task-runner/callback"
  export CHATOS_USER_SERVICE_BASE_URL="http://127.0.0.1:${USER_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_BASE_URL="http://127.0.0.1:${PROJECT_SERVICE_PORT}"
  export CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL="https://127.0.0.1:${PROJECT_SERVICE_INTERNAL_MTLS_PORT}"
  export CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET="$CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET"
  export CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL="https://127.0.0.1:${LOCAL_CONNECTOR_INTERNAL_MTLS_PORT}"
  export USER_SERVICE_HARNESS_PROVISIONING_ENABLED="${CHATOS_LOCAL_DEV_HARNESS_PROVISIONING_ENABLED:-true}"
  export USER_SERVICE_HARNESS_BASE_URL="${CHATOS_LOCAL_DEV_HARNESS_BASE_URL:-http://127.0.0.1:3000}"
}

config_center_caller_signing_secret() {
  case "$1" in
    chatos-backend) printf '%s' "$CONFIG_CENTER_CHATOS_BACKEND_CALLER_SIGNING_SECRET" ;;
    local-connector-service) printf '%s' "$CONFIG_CENTER_LOCAL_CONNECTOR_SERVICE_CALLER_SIGNING_SECRET" ;;
    mcp-management-service) printf '%s' "$CONFIG_CENTER_MCP_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET" ;;
    memory-engine) printf '%s' "$CONFIG_CENTER_MEMORY_ENGINE_CALLER_SIGNING_SECRET" ;;
    official-website) printf '%s' "$CONFIG_CENTER_OFFICIAL_WEBSITE_CALLER_SIGNING_SECRET" ;;
    plugin-management-service) printf '%s' "$CONFIG_CENTER_PLUGIN_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET" ;;
    project-service) printf '%s' "$CONFIG_CENTER_PROJECT_SERVICE_CALLER_SIGNING_SECRET" ;;
    task-runner) printf '%s' "$CONFIG_CENTER_TASK_RUNNER_CALLER_SIGNING_SECRET" ;;
    user-service) printf '%s' "$CONFIG_CENTER_USER_SERVICE_CALLER_SIGNING_SECRET" ;;
    *) return 1 ;;
  esac
}

config_center_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend|local-connector-service|mcp-management-service|memory-engine|official-website|plugin-management-service|project-service|task-runner|user-service)
      printf '%s/%s.identity.pem' "$CONFIG_CENTER_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

mcp_management_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend) printf '%s/chatos.identity.pem' "$MCP_MANAGEMENT_MTLS_DIR" ;;
    task-runner|project-service|configuration-center)
      printf '%s/%s.identity.pem' "$MCP_MANAGEMENT_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

task_runner_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend) printf '%s/chatos.identity.pem' "$TASK_RUNNER_MTLS_DIR" ;;
    mcp-management-service|project-service|user-service)
      printf '%s/%s.identity.pem' "$TASK_RUNNER_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

memory_engine_client_identity_path() {
  local caller="$1"
  case "$caller" in
    configuration-center|user-service|chatos-backend|task-runner)
      printf '%s/%s.identity.pem' "$MEMORY_ENGINE_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

project_service_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend|task-runner|mcp-management-service)
      printf '%s/%s.identity.pem' "$PROJECT_SERVICE_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

chatos_client_identity_path() {
  local caller="$1"
  case "$caller" in
    task-runner|mcp-management-service)
      printf '%s/%s.identity.pem' "$CHATOS_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

local_connector_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend|task-runner|project-service|mcp-management-service)
      printf '%s/%s.identity.pem' "$LOCAL_CONNECTOR_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

user_service_client_identity_path() {
  local caller="$1"
  case "$caller" in
    project-service) printf '%s/project-service.identity.pem' "$USER_SERVICE_MTLS_DIR" ;;
    *) return 1 ;;
  esac
}

plugin_management_client_identity_path() {
  local caller="$1"
  case "$caller" in
    chatos-backend|task-runner|project-service|local-connector-service|memory-engine|mcp-management-service)
      printf '%s/%s.identity.pem' "$PLUGIN_MANAGEMENT_MTLS_DIR" "$caller"
      ;;
    *) return 1 ;;
  esac
}

ensure_dirs() {
  mkdir -p \
    "$LOG_DIR" \
    "$PID_DIR" \
    "$STATE_DIR/task-runner" \
    "$STATE_DIR/chatos" \
    "$STATE_DIR/local-connector"
}

prepare_local_dev_apisix_config() {
  local source_config="$ROOT_DIR/docker/apisix/apisix.yaml"
  local target_config="$CHATOS_LOCAL_DEV_APISIX_CONFIG_PATH"
  local host_address="${CHATOS_LOCAL_DEV_HOST_ADDRESS:?CHATOS_LOCAL_DEV_HOST_ADDRESS is required}"
  if [[ ! -f "$source_config" ]]; then
    echo "[ERROR] APISIX route config is missing: $source_config" >&2
    return 1
  fi
  mkdir -p "$(dirname "$target_config")"
  sed \
    -e "s/\"chatos-backend:3997\"/\"${host_address}:3997\"/g" \
    -e "s/\"user-service-backend:39190\"/\"${host_address}:39190\"/g" \
    -e "s/\"project-management-backend:39210\"/\"${host_address}:39210\"/g" \
    -e "s/\"plugin-management-backend:39260\"/\"${host_address}:39260\"/g" \
    -e "s/\"mcp-management-service-backend:39280\"/\"${host_address}:39280\"/g" \
    -e "s/\"local-connector-service-backend:39230\"/\"${host_address}:39230\"/g" \
    -e "s/\"task-runner-backend:39090\"/\"${host_address}:39090\"/g" \
    -e "s/\"memory-engine-backend:7081\"/\"${host_address}:7081\"/g" \
    -e "s/\"chatos-frontend:80\"/\"${host_address}:8088\"/g" \
    -e "s/\"official-website-frontend:80\"/\"${host_address}:8088\"/g" \
    "$source_config" >"$target_config"
}

infra_service_host_port() {
  case "$1" in
    consul)
      printf '%s\n' "${CONSUL_HTTP_PORT:-8500}"
      ;;
    mongodb)
      printf '%s\n' "${MONGODB_HOST_PORT:-27018}"
      ;;
    rabbitmq)
      printf '%s\n' "${RABBITMQ_PORT:-5672}"
      ;;
    valkey)
      printf '%s\n' "${VALKEY_PORT:-6379}"
      ;;
    harness)
      printf '%s\n' "${HARNESS_PORT:-3000}"
      ;;
    apisix-gateway)
      printf '%s\n' "${APISIX_GATEWAY_PORT:-9080}"
      ;;
    prometheus)
      printf '%s\n' "${PROMETHEUS_PORT:-9090}"
      ;;
    alertmanager)
      printf '%s\n' "${ALERTMANAGER_PORT:-9093}"
      ;;
    grafana)
      printf '%s\n' "${GRAFANA_PORT:-3001}"
      ;;
    *)
      return 1
      ;;
  esac
}

start_infra() {
  need_cmd docker
  local network_name="${CHATOS_DOCKER_NETWORK:-chatos-cloud}"
  if ! docker network inspect "$network_name" >/dev/null 2>&1; then
    echo "[INFO] creating shared Docker network: $network_name"
    docker network create "$network_name" >/dev/null
  fi
  echo "[INFO] reconciling local-dev infrastructure containers: ${INFRA_SERVICES[*]}"
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
    "mcp-management-service",
    "task-runner",
    "chatos-backend",
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
