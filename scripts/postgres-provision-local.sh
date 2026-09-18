#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

postgres_host="${POSTGRES_HOST:-127.0.0.1}"
postgres_port="${POSTGRES_PORT:-5433}"
postgres_admin_database="${POSTGRES_ADMIN_DATABASE:-postgres}"
postgres_admin_user="${POSTGRES_ADMIN_USER:-${POSTGRES_USER:-chatos_admin}}"
postgres_admin_password="${POSTGRES_ADMIN_PASSWORD:-${POSTGRES_PASSWORD:-}}"
default_app_password="${POSTGRES_APP_PASSWORD:-}"
default_migration_password="${POSTGRES_MIGRATION_PASSWORD:-}"

if [[ -z "$postgres_admin_password" || -z "$default_app_password" || -z "$default_migration_password" ]]; then
  echo "POSTGRES_ADMIN_PASSWORD, POSTGRES_APP_PASSWORD, and POSTGRES_MIGRATION_PASSWORD are required" >&2
  exit 2
fi

database_specs=(
  "chatos:chatos_app:chatos_migrator"
  "configuration_center:config_center_app:config_center_migrator"
  "user_service:user_service_app:user_service_migrator"
  "plugin_management_service:plugin_management_app:plugin_management_migrator"
  "local_connector_service:local_connector_app:local_connector_migrator"
  "task_runner_service:task_runner_app:task_runner_migrator"
  "mcp_management_service:mcp_management_app:mcp_management_migrator"
  "memory_engine:memory_engine_app:memory_engine_migrator"
)

export PGPASSWORD="$postgres_admin_password"

for spec in "${database_specs[@]}"; do
  IFS=':' read -r database_name app_role migration_role <<< "$spec"
  app_password_variable="${app_role^^}_PASSWORD"
  migration_password_variable="${migration_role^^}_PASSWORD"
  app_password="${!app_password_variable:-$default_app_password}"
  migration_password="${!migration_password_variable:-$default_migration_password}"

  psql \
    --host "$postgres_host" \
    --port "$postgres_port" \
    --username "$postgres_admin_user" \
    --dbname "$postgres_admin_database" \
    --set ON_ERROR_STOP=1 \
    --set app_role="$app_role" \
    --set app_password="$app_password" \
    --set migration_role="$migration_role" \
    --set migration_password="$migration_password" \
    --set database_name="$database_name" <<'SQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'app_role', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'app_role') \gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'app_role', :'app_password') \gexec
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'migration_role', :'migration_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'migration_role') \gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'migration_role', :'migration_password') \gexec
SELECT format('CREATE DATABASE %I OWNER %I', :'database_name', :'migration_role')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'database_name') \gexec
SELECT format('ALTER DATABASE %I OWNER TO %I', :'database_name', :'migration_role') \gexec
SELECT format('REVOKE ALL PRIVILEGES ON DATABASE %I FROM PUBLIC', :'database_name') \gexec
SELECT format('GRANT CONNECT ON DATABASE %I TO %I', :'database_name', :'app_role') \gexec
SQL

  psql \
    --host "$postgres_host" \
    --port "$postgres_port" \
    --username "$postgres_admin_user" \
    --dbname "$database_name" \
    --set ON_ERROR_STOP=1 \
    --set app_role="$app_role" \
    --set migration_role="$migration_role" <<'SQL'
SELECT format('GRANT USAGE ON SCHEMA public TO %I', :'app_role') \gexec
SELECT format(
  'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO %I',
  :'migration_role', :'app_role'
) \gexec
SELECT format(
  'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO %I',
  :'migration_role', :'app_role'
) \gexec
SQL
done

echo "Provisioned 8 PostgreSQL databases and isolated application roles."
