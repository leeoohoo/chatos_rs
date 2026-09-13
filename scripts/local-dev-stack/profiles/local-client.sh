#!/usr/bin/env bash

STACK_DISPLAY_NAME="ChatOS 3.0.2 local-client backend stack"
STACK_EXCLUSIVE_INFRA=true

# These are the only server-side dependencies of the local-agent client. RabbitMQ
# remains the durable queue used by Memory Engine summary/rollup workers; Agent Run
# scheduling and tool execution do not use it.
INFRA_SERVICES=(consul mongodb minio rabbitmq apisix-gateway)

# The profile selects services by stable catalog key. Build metadata, ports and
# binary names are resolved centrally by service-catalog.sh.
BACKEND_SERVICE_NAMES=(
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  memory-engine-worker
  plugin-management-backend
  chatos-backend
)

FRONTEND_SERVICES=()

# Local projects and tools are owned by the native client. The final client stack
# must not provision a server-side Harness workspace.
export CHATOS_LOCAL_DEV_HARNESS_PROVISIONING_ENABLED=false

stack_after_start() {
  local item forbidden_name forbidden_port forbidden_pid
  local forbidden_services=(
    "task-runner-backend|39090"
    "local-connector-service-backend|39230"
    "mcp-management-service-backend|39280"
    "official-website-backend|39250"
    "admin-console-frontend|39200"
    "official-website-frontend|39251"
  )

  for item in "${forbidden_services[@]}"; do
    IFS='|' read -r forbidden_name forbidden_port <<<"$item"
    forbidden_pid="$(pid_for_port "$forbidden_port")"
    if [[ -n "$forbidden_pid" ]]; then
      echo "[ERROR] obsolete service is still listening: $forbidden_name port=$forbidden_port pid=$forbidden_pid" >&2
      return 1
    fi
  done

  wait_for_http \
    "unified local-client gateway" \
    "http://127.0.0.1:${APISIX_GATEWAY_PORT:-9080}/api/chatos/health" \
    "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}"
}

stack_print_urls() {
  cat <<EOF

[OK] ChatOS 3.0.2 local-client backend stack is ready.

Unified gateway:          http://localhost:${APISIX_GATEWAY_PORT:-9080}
Configuration Center:     http://127.0.0.1:${CONFIG_CENTER_PORT:-39270}
User Service:             http://127.0.0.1:${USER_SERVICE_PORT:-39190}
Memory Engine:            http://127.0.0.1:${MEMORY_ENGINE_PORT:-7081}
Plugin Management:        http://127.0.0.1:${PLUGIN_MANAGEMENT_SERVICE_PORT:-39260}
Model Gateway/API shell:  http://127.0.0.1:${BACKEND_PORT:-3997}

Remote Task Runner, MCP orchestration, Local Connector cloud execution,
admin/website services and frontends are not started by this profile.

Status:  $STACK_COMMAND status
Logs:    $STACK_COMMAND logs <service-name>
Stop:    $STACK_COMMAND down
EOF
}
