#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_CENTER_BASE_URL="${LOCAL_PROJECT_ENTRY_CONFIG_CENTER_BASE_URL:-http://127.0.0.1:${CONFIG_CENTER_PORT:-39270}}"
CHATOS_BASE_URL="${LOCAL_PROJECT_ENTRY_CHATOS_BASE_URL:-http://127.0.0.1:${BACKEND_PORT:-3997}}"
CONFIG_ENVIRONMENT="${LOCAL_PROJECT_ENTRY_CONFIG_ENVIRONMENT:-local}"
CONFIG_CENTER_SECRET="${CONFIG_CENTER_INTERNAL_API_SECRET:-change_me_configuration_center_internal_secret}"
JWT_SECRET="${USER_SERVICE_JWT_SECRET:-change_me_user_service_secret}"

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "[ERROR] $command_name is required" >&2
    exit 1
  fi
}

require_command curl
require_command docker
require_command node

if ! curl -fsS --max-time 5 "${CONFIG_CENTER_BASE_URL%/}/health" >/dev/null; then
  echo "[ERROR] Configuration Center is not healthy at $CONFIG_CENTER_BASE_URL" >&2
  exit 1
fi
if ! curl -fsS --max-time 5 "${CHATOS_BASE_URL%/}/health" >/dev/null; then
  echo "[ERROR] ChatOS is not healthy at $CHATOS_BASE_URL" >&2
  exit 1
fi

mongodb_container="$(
  docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
)"
if [[ -z "$mongodb_container" ]]; then
  echo "[ERROR] local MongoDB container is not running" >&2
  exit 1
fi

user_json="$({
  docker exec "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const user = db.getSiblingDB("user_service").users.findOne(
        {enabled: true, id: {$type: "string"}},
        {_id: 0, id: 1, username: 1, display_name: 1, role: 1}
      );
      if (user) {
        print(JSON.stringify(user));
      }
    '
} | tail -n 1)"
if [[ -z "$user_json" || "$user_json" == "null" ]]; then
  echo "[ERROR] no enabled user-service user was found" >&2
  exit 1
fi

export LOCAL_PROJECT_ENTRY_CONFIG_CENTER_BASE_URL="${CONFIG_CENTER_BASE_URL%/}"
export LOCAL_PROJECT_ENTRY_CHATOS_BASE_URL="${CHATOS_BASE_URL%/}"
export LOCAL_PROJECT_ENTRY_CONFIG_ENVIRONMENT="$CONFIG_ENVIRONMENT"
export LOCAL_PROJECT_ENTRY_CONFIG_CENTER_SECRET="$CONFIG_CENTER_SECRET"
export LOCAL_PROJECT_ENTRY_JWT_SECRET="$JWT_SECRET"
export LOCAL_PROJECT_ENTRY_USER_JSON="$user_json"

node <<'NODE'
const crypto = require('crypto');

const configCenterBaseUrl = process.env.LOCAL_PROJECT_ENTRY_CONFIG_CENTER_BASE_URL;
const chatosBaseUrl = process.env.LOCAL_PROJECT_ENTRY_CHATOS_BASE_URL;
const environment = process.env.LOCAL_PROJECT_ENTRY_CONFIG_ENVIRONMENT;
const configCenterSecret = process.env.LOCAL_PROJECT_ENTRY_CONFIG_CENTER_SECRET;
const jwtSecret = process.env.LOCAL_PROJECT_ENTRY_JWT_SECRET;
const user = JSON.parse(process.env.LOCAL_PROJECT_ENTRY_USER_JSON);
const configKey = 'chatos.ui.local_project_creation_enabled';
const envKey = 'LOCAL_PROJECT_CREATION_ENABLED';

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function encodedJson(value) {
  return Buffer.from(JSON.stringify(value)).toString('base64url');
}

function issueUserToken() {
  const now = Math.floor(Date.now() / 1000);
  const header = encodedJson({alg: 'HS256', typ: 'JWT'});
  const payload = encodedJson({
    iss: 'user_service',
    aud: 'user_service',
    sub: `user:${user.id}`,
    exp: now + 60,
    iat: now,
    jti: crypto.randomUUID(),
    principal_type: 'human_user',
    user_id: user.id,
    username: user.username || user.id,
    display_name: user.display_name || user.username || user.id,
    role: user.role || 'user',
    agent_account_id: null,
    owner_user_id: null,
    owner_username: null,
    owner_display_name: null,
    scopes: ['user_service'],
  });
  const signature = crypto
    .createHmac('sha256', jwtSecret)
    .update(`${header}.${payload}`)
    .digest('base64url');
  return `${header}.${payload}.${signature}`;
}

async function readJson(response, label) {
  const text = await response.text();
  if (!text) {
    throw new Error(`${label} returned an empty response with status ${response.status}`);
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`${label} returned non-JSON content with status ${response.status}`);
  }
}

async function main() {
  const snapshotResponse = await fetch(
    `${configCenterBaseUrl}/internal/config/v1/snapshots/chatos-backend?environment=${encodeURIComponent(environment)}`,
    {
      headers: {
        'x-config-center-service': 'chatos-backend',
        'x-config-center-internal-secret': configCenterSecret,
      },
    },
  );
  const snapshot = await readJson(snapshotResponse, 'Configuration Center snapshot');
  assert(snapshotResponse.ok, `Configuration Center snapshot failed: ${snapshotResponse.status}`);
  assert(
    Object.prototype.hasOwnProperty.call(snapshot.values || {}, configKey),
    `${configKey} is missing from the live chatos-backend snapshot`,
  );
  const managedValue = snapshot.values[configKey];
  assert(typeof managedValue === 'boolean', `${configKey} must be a boolean`);
  assert(
    snapshot.env?.[envKey] === String(managedValue),
    `${envKey} compatibility value does not match the managed value`,
  );

  const settingsResponse = await fetch(`${chatosBaseUrl}/api/user-settings`, {
    headers: {authorization: `Bearer ${issueUserToken()}`},
  });
  const settings = await readJson(settingsResponse, 'ChatOS user settings');
  assert(settingsResponse.ok, `ChatOS user settings failed: ${settingsResponse.status}`);
  assert(
    settings.effective?.[envKey] === managedValue,
    `ChatOS effective ${envKey} does not match Configuration Center`,
  );

  console.log(
    `[OK] ${configKey}=${managedValue} flows from Configuration Center to ChatOS user settings`,
  );
}

main().catch((error) => {
  console.error(`[ERROR] ${error.message}`);
  process.exitCode = 1;
});
NODE
