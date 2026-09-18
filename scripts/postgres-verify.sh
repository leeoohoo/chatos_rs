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
postgres_admin_database="${POSTGRES_ADMIN_DATABASE:-postgres}"
postgres_admin_user="${POSTGRES_ADMIN_USER:-${POSTGRES_USER:-chatos_admin}}"
postgres_admin_password="${POSTGRES_ADMIN_PASSWORD:-${POSTGRES_PASSWORD:-}}"

if [[ -z "$postgres_admin_password" ]]; then
  echo "POSTGRES_ADMIN_PASSWORD is required" >&2
  exit 2
fi

export PGPASSWORD="$postgres_admin_password"
postgres_container=""
if ! command -v psql >/dev/null 2>&1; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "psql or Docker is required" >&2
    exit 2
  fi
  postgres_container="${POSTGRES_CONTAINER_NAME:-$(docker ps \
    --filter "label=com.docker.compose.project=${COMPOSE_PROJECT_NAME:-chatos-rs}" \
    --filter "label=com.docker.compose.service=postgres" \
    --format '{{.Names}}' | sed -n '1p')}"
  if [[ -z "$postgres_container" ]]; then
    echo "PostgreSQL container is not running" >&2
    exit 2
  fi
fi

run_psql() {
  local database_name="$1"
  shift
  if [[ -n "$postgres_container" ]]; then
    docker exec \
      --env "PGPASSWORD=$postgres_admin_password" \
      "$postgres_container" \
      psql \
      --host 127.0.0.1 \
      --port 5432 \
      --username "$postgres_admin_user" \
      --dbname "$database_name" \
      --tuples-only \
      --no-align \
      --set ON_ERROR_STOP=1 \
      "$@"
  else
    psql \
      --host "$postgres_host" \
      --port "$postgres_port" \
      --username "$postgres_admin_user" \
      --dbname "$database_name" \
      --tuples-only \
      --no-align \
      --set ON_ERROR_STOP=1 \
      "$@"
  fi
}

expected_databases=(
  chatos
  configuration_center
  user_service
  plugin_management_service
  local_connector_service
  task_runner_service
  mcp_management_service
  memory_engine
)
expected_app_roles=(
  chatos_app
  config_center_app
  user_service_app
  plugin_management_app
  local_connector_app
  task_runner_app
  mcp_management_app
  memory_engine_app
)

for database_index in "${!expected_databases[@]}"; do
  database_name="${expected_databases[$database_index]}"
  app_role="${expected_app_roles[$database_index]}"
  schema_version_count="$(run_psql "$database_name" \
    --command "SELECT count(*) FROM _sqlx_migrations WHERE success = TRUE")"
  if [[ "$schema_version_count" -lt 1 ]]; then
    echo "$database_name has no successful SQLx migrations" >&2
    exit 1
  fi
  echo "$database_name: $schema_version_count migration(s) applied"

  dml_violation_count="$(run_psql "$database_name" \
    --command "
SELECT count(*)
FROM pg_tables
WHERE schemaname = 'public'
  AND tablename <> '_sqlx_migrations'
  AND NOT (
    has_table_privilege('$app_role', format('%I.%I', schemaname, tablename), 'SELECT')
    AND has_table_privilege('$app_role', format('%I.%I', schemaname, tablename), 'INSERT')
    AND has_table_privilege('$app_role', format('%I.%I', schemaname, tablename), 'UPDATE')
    AND has_table_privilege('$app_role', format('%I.%I', schemaname, tablename), 'DELETE')
  )")"
  if [[ "$dml_violation_count" -ne 0 ]]; then
    echo "$app_role is missing DML privileges on $dml_violation_count table(s) in $database_name" >&2
    exit 1
  fi

  migration_write_violation_count="$(run_psql "$database_name" \
    --command "
SELECT count(*)
FROM (VALUES ('INSERT'), ('UPDATE'), ('DELETE'), ('TRUNCATE'), ('REFERENCES'), ('TRIGGER')) AS privilege(name)
WHERE has_table_privilege('$app_role', 'public._sqlx_migrations', privilege.name)")"
  if [[ "$migration_write_violation_count" -ne 0 ]]; then
    echo "$app_role can modify _sqlx_migrations in $database_name" >&2
    exit 1
  fi

  ddl_violation_count="$(run_psql "$database_name" \
    --command "
SELECT count(*)
WHERE has_schema_privilege('$app_role', 'public', 'CREATE')
   OR has_database_privilege('$app_role', '$database_name', 'CREATE')
   OR has_database_privilege('$app_role', '$database_name', 'TEMP')")"
  if [[ "$ddl_violation_count" -ne 0 ]]; then
    echo "$app_role has runtime DDL or temporary-object privileges in $database_name" >&2
    exit 1
  fi
done

isolation_violation_count="$(run_psql "$postgres_admin_database" \
  --command "
WITH expected(role_name, database_name) AS (
  VALUES
    ('chatos_app', 'chatos'),
    ('config_center_app', 'configuration_center'),
    ('user_service_app', 'user_service'),
    ('plugin_management_app', 'plugin_management_service'),
    ('local_connector_app', 'local_connector_service'),
    ('task_runner_app', 'task_runner_service'),
    ('mcp_management_app', 'mcp_management_service'),
    ('memory_engine_app', 'memory_engine')
)
SELECT count(*)
FROM expected AS role_scope
CROSS JOIN expected AS database_scope
WHERE has_database_privilege(
        role_scope.role_name,
        database_scope.database_name,
        'CONNECT'
      ) IS DISTINCT FROM (role_scope.database_name = database_scope.database_name)
   OR (
        role_scope.database_name = database_scope.database_name
        AND (
          has_database_privilege(role_scope.role_name, database_scope.database_name, 'CREATE')
          OR has_database_privilege(role_scope.role_name, database_scope.database_name, 'TEMP')
        )
      )")"

if [[ "$isolation_violation_count" -ne 0 ]]; then
  echo "PostgreSQL application-role database isolation has $isolation_violation_count violation(s)" >&2
  exit 1
fi

role_attribute_violation_count="$(run_psql "$postgres_admin_database" \
  --command "
SELECT count(*)
FROM pg_roles
WHERE rolname = ANY(ARRAY['chatos_app','config_center_app','user_service_app','plugin_management_app','local_connector_app','task_runner_app','mcp_management_app','memory_engine_app'])
  AND (rolsuper OR rolcreaterole OR rolcreatedb OR rolreplication OR rolbypassrls)")"
if [[ "$role_attribute_violation_count" -ne 0 ]]; then
  echo "PostgreSQL application roles have $role_attribute_violation_count unsafe role attribute(s)" >&2
  exit 1
fi

echo "PostgreSQL application roles are isolated and cannot modify schema migration metadata."
