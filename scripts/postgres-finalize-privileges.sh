#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap_file="${CHATOS_BOOTSTRAP_FILE:-$root_dir/docker/bootstrap.conf}"
if [[ -z "${POSTGRES_ADMIN_PASSWORD:-}" && -f "$bootstrap_file" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$bootstrap_file"
  set +a
fi

postgres_host="${POSTGRES_HOST:-127.0.0.1}"
postgres_port="${POSTGRES_PORT:-5433}"
postgres_admin_user="${POSTGRES_ADMIN_USER:-${POSTGRES_USER:-chatos_admin}}"
postgres_admin_password="${POSTGRES_ADMIN_PASSWORD:-${POSTGRES_PASSWORD:-}}"
if [[ -z "$postgres_admin_password" ]]; then
  echo "POSTGRES_ADMIN_PASSWORD is required" >&2
  exit 2
fi

database_specs=(
  "chatos:chatos_app"
  "configuration_center:config_center_app"
  "user_service:user_service_app"
  "plugin_management_service:plugin_management_app"
  "local_connector_service:local_connector_app"
  "task_runner_service:task_runner_app"
  "mcp_management_service:mcp_management_app"
  "memory_engine:memory_engine_app"
)

run_psql() {
  local database_name="$1"
  shift
  if command -v psql >/dev/null 2>&1; then
    PGPASSWORD="$postgres_admin_password" psql \
      --host "$postgres_host" --port "$postgres_port" \
      --username "$postgres_admin_user" --dbname "$database_name" \
      --set ON_ERROR_STOP=1 "$@"
    return
  fi
  local postgres_container
  postgres_container="${POSTGRES_CONTAINER_NAME:-$(docker ps \
    --filter "label=com.docker.compose.project=${COMPOSE_PROJECT_NAME:-chatos-rs}" \
    --filter "label=com.docker.compose.service=postgres" \
    --format '{{.Names}}' | sed -n '1p')}"
  if [[ -z "$postgres_container" ]]; then
    echo "psql is unavailable and the PostgreSQL container is not running" >&2
    exit 2
  fi
  docker exec --interactive --env "PGPASSWORD=$postgres_admin_password" "$postgres_container" \
    psql --host 127.0.0.1 --port 5432 \
    --username "$postgres_admin_user" --dbname "$database_name" \
    --set ON_ERROR_STOP=1 "$@"
}

for spec in "${database_specs[@]}"; do
  IFS=':' read -r database_name app_role <<< "$spec"
  run_psql "$database_name" \
    --set app_role="$app_role" \
    --set database_name="$database_name" <<'SQL'
SELECT format('REVOKE CREATE ON SCHEMA public FROM %I', :'app_role') \gexec
SELECT format('REVOKE CREATE, TEMPORARY ON DATABASE %I FROM %I', :'database_name', :'app_role') \gexec
SELECT format(
  'REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public._sqlx_migrations FROM %I',
  :'app_role'
) \gexec
SELECT format('GRANT SELECT ON TABLE public._sqlx_migrations TO %I', :'app_role') \gexec
SQL
done

echo "Finalized PostgreSQL runtime privileges for 8 application roles."
