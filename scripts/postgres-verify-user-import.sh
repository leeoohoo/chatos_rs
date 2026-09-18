#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap_file="${CHATOS_BOOTSTRAP_FILE:-$root_dir/docker/bootstrap.conf}"
if [[ -f "$bootstrap_file" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$bootstrap_file"
  set +a
fi

expected_users="${EXPECTED_MIGRATED_USER_COUNT:-}"
expected_identities="${EXPECTED_MIGRATED_WECHAT_IDENTITY_COUNT:-}"
if [[ ! "$expected_users" =~ ^[1-9][0-9]*$ ]]; then
  echo "EXPECTED_MIGRATED_USER_COUNT must be an integer greater than zero" >&2
  exit 2
fi
if [[ ! "$expected_identities" =~ ^[0-9]+$ ]]; then
  echo "EXPECTED_MIGRATED_WECHAT_IDENTITY_COUNT must be a non-negative integer" >&2
  exit 2
fi

postgres_admin_user="${POSTGRES_ADMIN_USER:-${POSTGRES_USER:-chatos_admin}}"
postgres_admin_password="${POSTGRES_ADMIN_PASSWORD:-${POSTGRES_PASSWORD:-}}"
postgres_database="${USER_SERVICE_DATABASE_NAME:-user_service}"
postgres_host="${POSTGRES_HOST:-127.0.0.1}"
postgres_port="${POSTGRES_PORT:-5433}"
if [[ -z "$postgres_admin_password" ]]; then
  echo "POSTGRES_ADMIN_PASSWORD is required" >&2
  exit 2
fi

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
  if [[ -n "$postgres_container" ]]; then
    docker exec \
      --env "PGPASSWORD=$postgres_admin_password" \
      "$postgres_container" \
      psql --host 127.0.0.1 --port 5432 \
      --username "$postgres_admin_user" --dbname "$postgres_database" \
      --tuples-only --no-align --set ON_ERROR_STOP=1 --command "$1"
  else
    PGPASSWORD="$postgres_admin_password" psql \
      --host "$postgres_host" --port "$postgres_port" \
      --username "$postgres_admin_user" --dbname "$postgres_database" \
      --tuples-only --no-align --set ON_ERROR_STOP=1 --command "$1"
  fi
}

IFS='|' read -r actual_users invalid_users actual_identities orphan_identities <<< "$(run_psql "
SELECT
  (SELECT count(*) FROM users),
  (SELECT count(*) FROM users
   WHERE btrim(id) = ''
      OR btrim(username) = ''
      OR btrim(display_name) = ''
      OR btrim(password_hash) = ''
      OR password_hash NOT LIKE '\$argon2%'
      OR btrim(role) = ''),
  (SELECT count(*) FROM user_external_identities WHERE revoked_at IS NULL),
  (SELECT count(*)
   FROM user_external_identities AS identity
   LEFT JOIN users AS app_user ON app_user.id = identity.user_id
   WHERE identity.revoked_at IS NULL AND app_user.id IS NULL)")"

if [[ "$actual_users" != "$expected_users" ]]; then
  echo "User migration verification failed: target has $actual_users user(s), expected $expected_users" >&2
  exit 1
fi
if [[ "$invalid_users" != "0" ]]; then
  echo "User migration verification failed: $invalid_users user row(s) have missing fields or an invalid password hash" >&2
  exit 1
fi
if [[ "$actual_identities" != "$expected_identities" ]]; then
  echo "User migration verification failed: target has $actual_identities active WeChat identity record(s), expected $expected_identities" >&2
  exit 1
fi
if [[ "$orphan_identities" != "0" ]]; then
  echo "User migration verification failed: $orphan_identities active identity record(s) have no user" >&2
  exit 1
fi

echo "User migration verified: $actual_users user(s), $actual_identities active WeChat identity record(s)."
