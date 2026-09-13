#!/usr/bin/env bash

STACK_DISPLAY_NAME="ChatOS full development stack"
STACK_EXCLUSIVE_INFRA=false

INFRA_SERVICES=(consul mongodb minio rabbitmq valkey cadvisor prometheus alertmanager tempo grafana harness apisix-gateway)
BACKEND_SERVICE_NAMES=(
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  memory-engine-worker
  plugin-management-backend
  local-connector-service-backend
  mcp-management-service-backend
  task-runner-backend
  task-runner-worker
  task-runner-scheduler
  chatos-backend
  official-website-backend
)

FRONTEND_SERVICES=(
  "admin-console-frontend|admin_console|39200"
  "official-website-frontend|official_website_service/frontend|39251"
)

stack_print_urls() {
  cat <<EOF

[OK] Full local development stack is ready.

Official website:         http://localhost:39251
Unified admin console:    http://localhost:39200
Unified gateway:          http://localhost:${APISIX_GATEWAY_PORT:-9080}
Prometheus:               http://127.0.0.1:${PROMETHEUS_PORT:-9090}
Alertmanager:             http://127.0.0.1:${ALERTMANAGER_PORT:-9093}
Grafana:                  http://127.0.0.1:${GRAFANA_PORT:-3001}
Main backend:             http://localhost:3997
Harness:                  http://localhost:3000
Local Connector Service:  http://localhost:39230
MCP Management Service:   http://localhost:39280

Status:  $STACK_COMMAND status
Logs:    $STACK_COMMAND logs <service-name>
Stop:    $STACK_COMMAND down
EOF
}
