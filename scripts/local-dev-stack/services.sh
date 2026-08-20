#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

start_backend() {
  local name="$1"
  local service_name="$2"
  local manifest="$3"
  local health_path="$4"
  local port="$5"
  local bin="${6:-}"
  local env_overrides="${7:-}"
  local log_file pid_file
  local binary
  local -a cargo_args=(build --manifest-path "$manifest")
  if [[ -z "$bin" ]]; then
    echo "[ERROR] missing binary name for $name" >&2
    exit 1
  fi
  cargo_args+=(--bin "$bin")
  binary="$(target_binary_for "$bin")"
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  if [[ -n "$port" && "$port" != "-" ]]; then
    stop_port_if_needed "$port" "$name"
    if [[ "$name" == "memory-engine-backend" ]]; then
      stop_port_if_needed "$MEMORY_ENGINE_INTERNAL_MTLS_PORT" "$name internal mTLS"
    fi
    if [[ "$name" == "project-management-backend" ]]; then
      stop_port_if_needed "$PROJECT_SERVICE_INTERNAL_MTLS_PORT" "$name internal mTLS"
    fi
    if [[ "$name" == "user-service-backend" ]]; then
      stop_port_if_needed "$USER_SERVICE_INTERNAL_MTLS_PORT" "$name internal mTLS"
    fi
    if [[ "$name" == "plugin-management-backend" ]]; then
      stop_port_if_needed "$PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT" "$name internal mTLS"
    fi
    echo "[INFO] starting $name on 127.0.0.1:$port"
  else
    echo "[INFO] starting $name without HTTP listener"
  fi
  : >"$log_file"
  (
    cd "$ROOT_DIR"
    cargo "${cargo_args[@]}"
  ) >>"$log_file" 2>&1
  local spawned_pid
  spawned_pid="$(
    export CHATOS_SERVICE_NAME="$service_name"
    export CHATOS_SERVICE_ID="${service_name}-local"
    export CHATOS_SERVICE_PORT="${port:-0}"
    export CHATOS_SERVICE_HEALTH_PATH="${health_path:-/health}"
    if config_center_secret="$(config_center_caller_signing_secret "$service_name")"; then
      export CONFIG_CENTER_CALLER_SIGNING_SECRET="$config_center_secret"
      export CONFIG_CENTER_MTLS_CLIENT_IDENTITY_PATH="$(config_center_client_identity_path "$service_name")"
    fi
    if mcp_management_identity="$(mcp_management_client_identity_path "$service_name")"; then
      export MCP_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH="$mcp_management_identity"
    fi
    if task_runner_identity="$(task_runner_client_identity_path "$service_name")"; then
      export TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH="$task_runner_identity"
    fi
    if memory_engine_identity="$(memory_engine_client_identity_path "$service_name")"; then
      export MEMORY_ENGINE_MTLS_CLIENT_IDENTITY_PATH="$memory_engine_identity"
    fi
    if project_service_identity="$(project_service_client_identity_path "$service_name")"; then
      export PROJECT_SERVICE_MTLS_CLIENT_IDENTITY_PATH="$project_service_identity"
    fi
    if chatos_identity="$(chatos_client_identity_path "$service_name")"; then
      export CHATOS_MTLS_CLIENT_IDENTITY_PATH="$chatos_identity"
    fi
    if local_connector_identity="$(local_connector_client_identity_path "$service_name")"; then
      export LOCAL_CONNECTOR_MTLS_CLIENT_IDENTITY_PATH="$local_connector_identity"
    fi
    if user_service_identity="$(user_service_client_identity_path "$service_name")"; then
      export USER_SERVICE_MTLS_CLIENT_IDENTITY_PATH="$user_service_identity"
    fi
    if plugin_management_identity="$(plugin_management_client_identity_path "$service_name")"; then
      export PLUGIN_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH="$plugin_management_identity"
    fi
    if [[ "$name" == "memory-engine-backend" ]]; then
      export MEMORY_ENGINE_MTLS_SERVER_CERT_PATH="$MEMORY_ENGINE_MTLS_DIR/server.crt"
      export MEMORY_ENGINE_MTLS_SERVER_KEY_PATH="$MEMORY_ENGINE_MTLS_DIR/server.key"
      export MEMORY_ENGINE_MTLS_CLIENT_CA_CERT_PATH="$MEMORY_ENGINE_MTLS_DIR/ca.crt"
    fi
    if [[ "$name" == "project-management-backend" ]]; then
      export PROJECT_SERVICE_MTLS_SERVER_CERT_PATH="$PROJECT_SERVICE_MTLS_DIR/server.crt"
      export PROJECT_SERVICE_MTLS_SERVER_KEY_PATH="$PROJECT_SERVICE_MTLS_DIR/server.key"
      export PROJECT_SERVICE_MTLS_CLIENT_CA_CERT_PATH="$PROJECT_SERVICE_MTLS_DIR/ca.crt"
    fi
    if [[ "$name" == "chatos-backend" ]]; then
      export CHATOS_MTLS_SERVER_CERT_PATH="$CHATOS_MTLS_DIR/server.crt"
      export CHATOS_MTLS_SERVER_KEY_PATH="$CHATOS_MTLS_DIR/server.key"
      export CHATOS_MTLS_CLIENT_CA_CERT_PATH="$CHATOS_MTLS_DIR/ca.crt"
    fi
    if [[ "$name" == "local-connector-service-backend" ]]; then
      export LOCAL_CONNECTOR_MTLS_SERVER_CERT_PATH="$LOCAL_CONNECTOR_MTLS_DIR/server.crt"
      export LOCAL_CONNECTOR_MTLS_SERVER_KEY_PATH="$LOCAL_CONNECTOR_MTLS_DIR/server.key"
      export LOCAL_CONNECTOR_MTLS_CLIENT_CA_CERT_PATH="$LOCAL_CONNECTOR_MTLS_DIR/ca.crt"
    fi
    if [[ "$name" == "user-service-backend" ]]; then
      export USER_SERVICE_MTLS_SERVER_CERT_PATH="$USER_SERVICE_MTLS_DIR/server.crt"
      export USER_SERVICE_MTLS_SERVER_KEY_PATH="$USER_SERVICE_MTLS_DIR/server.key"
      export USER_SERVICE_MTLS_CLIENT_CA_CERT_PATH="$USER_SERVICE_MTLS_DIR/ca.crt"
    fi
    if [[ "$name" == "plugin-management-backend" ]]; then
      export PLUGIN_MANAGEMENT_MTLS_SERVER_CERT_PATH="$PLUGIN_MANAGEMENT_MTLS_DIR/server.crt"
      export PLUGIN_MANAGEMENT_MTLS_SERVER_KEY_PATH="$PLUGIN_MANAGEMENT_MTLS_DIR/server.key"
      export PLUGIN_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH="$PLUGIN_MANAGEMENT_MTLS_DIR/ca.crt"
    fi
    if [[ -n "$env_overrides" && "$env_overrides" != "-" ]]; then
      # shellcheck disable=SC2086
      export $env_overrides
    fi
    spawn_detached "$ROOT_DIR" "$log_file" "$binary"
  )"
  echo "$spawned_pid" >"$pid_file"
  if [[ -n "$port" && "$port" != "-" && -n "$health_path" && "$health_path" != "-" ]]; then
    wait_for_http "$name" "http://127.0.0.1:${port}${health_path}" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}"
  fi
}

ensure_config_center_mtls_material() {
  "$ROOT_DIR/scripts/generate-config-center-mtls.sh" "$CONFIG_CENTER_MTLS_DIR"
}

ensure_mcp_management_mtls_material() {
  "$ROOT_DIR/scripts/generate-mcp-management-mtls.sh" "$MCP_MANAGEMENT_MTLS_DIR"
}

ensure_task_runner_mtls_material() {
  "$ROOT_DIR/scripts/generate-task-runner-mtls.sh" "$TASK_RUNNER_MTLS_DIR"
}

ensure_project_service_mtls_material() {
  "$ROOT_DIR/scripts/generate-project-service-mtls.sh" "$PROJECT_SERVICE_MTLS_DIR"
}

ensure_chatos_mtls_material() {
  "$ROOT_DIR/scripts/generate-chatos-mtls.sh" "$CHATOS_MTLS_DIR"
}

ensure_local_connector_mtls_material() {
  "$ROOT_DIR/scripts/generate-local-connector-mtls.sh" "$LOCAL_CONNECTOR_MTLS_DIR"
}

ensure_user_service_mtls_material() {
  "$ROOT_DIR/scripts/generate-user-service-mtls.sh" "$USER_SERVICE_MTLS_DIR"
}

ensure_memory_engine_mtls_material() {
  "$ROOT_DIR/scripts/generate-memory-engine-mtls.sh" "$MEMORY_ENGINE_MTLS_DIR"
}

ensure_plugin_management_mtls_material() {
  "$ROOT_DIR/scripts/generate-plugin-management-mtls.sh" "$PLUGIN_MANAGEMENT_MTLS_DIR"
}

ensure_local_dev_managed_runtime_config() {
  local config_center_base_url="http://127.0.0.1:${CONFIG_CENTER_PORT}"
  local environment="${CHATOS_ENV:-local}"
  local catalog_file effective_file draft_file desired_file merge_file validation_file
  local login_payload login_response token change_count

  LOCAL_DEV_CONFIG_CHANGED=false

  catalog_file="$(mktemp)"
  effective_file="$(mktemp)"
  draft_file="$(mktemp)"
  desired_file="$(mktemp)"
  merge_file="$(mktemp)"
  validation_file="$(mktemp)"

  login_payload="$(python3 - <<'PY'
import json
import os
print(json.dumps({
    "username": os.environ["CHATOS_ADMIN_USERNAME"],
    "password": os.environ["CHATOS_ADMIN_PASSWORD"],
}, separators=(",", ":")))
PY
  )"
  if ! login_response="$(
    curl -fsS \
      -H "content-type: application/json" \
      --data "$login_payload" \
      "${config_center_base_url}/api/auth/login"
  )"; then
    echo "[ERROR] configuration center admin login failed during local-dev config synchronization" >&2
    rm -f "$catalog_file" "$effective_file" "$draft_file" "$desired_file" "$merge_file" "$validation_file"
    return 1
  fi
  token="$(printf '%s' "$login_response" | python3 -c 'import json, sys; print(json.load(sys.stdin)["token"])')"

  curl -fsS \
    -H "authorization: Bearer $token" \
    "${config_center_base_url}/api/config/v1/catalog" \
    >"$catalog_file"
  curl -fsS \
    -H "authorization: Bearer $token" \
    "${config_center_base_url}/api/config/v1/environments/${environment}/effective" \
    >"$effective_file"

  python3 - "$catalog_file" "$effective_file" "$desired_file" <<'PY'
import json
import os
import sys

catalog_path, effective_path, desired_path = sys.argv[1:]
with open(catalog_path, encoding="utf-8") as handle:
    catalog = json.load(handle)
with open(effective_path, encoding="utf-8") as handle:
    effective = json.load(handle).get("values") or {}

def parse_value(raw, value_type):
    if value_type == "boolean":
        normalized = raw.strip().lower()
        if normalized not in {"true", "false"}:
            raise ValueError(f"invalid boolean value: {raw}")
        return normalized == "true"
    if value_type in {"integer", "duration_ms", "bytes"}:
        return int(raw)
    if value_type in {"number", "float"}:
        return float(raw)
    if value_type in {"array", "object", "json"}:
        return json.loads(raw)
    return raw

desired = {}
for definition in catalog:
    aliases = definition.get("env_aliases") or []
    raw = next((os.environ.get(alias) for alias in aliases if os.environ.get(alias)), None)
    if raw is None:
        continue
    desired[definition["key"]] = parse_value(raw, definition.get("value_type") or "string")

rabbitmq_user = os.environ.get("RABBITMQ_DEFAULT_USER", "chatos")
rabbitmq_password = os.environ.get("RABBITMQ_DEFAULT_PASS", "change_me_rabbitmq_password")
rabbitmq_port = os.environ.get("RABBITMQ_PORT", "5672")
rabbitmq_url = f"amqp://{rabbitmq_user}:{rabbitmq_password}@127.0.0.1:{rabbitmq_port}/%2f"
valkey_password = os.environ.get("VALKEY_PASSWORD", "change_me_valkey_password")
valkey_port = os.environ.get("VALKEY_PORT", "6379")
valkey_url = f"redis://:{valkey_password}@127.0.0.1:{valkey_port}/0"
desired.update({
    "task_runner.queue.callback_delivery_mode": "rabbitmq",
    "task_runner.queue.rabbitmq_url": rabbitmq_url,
    "task_runner.observability.otlp_endpoint": "http://127.0.0.1:4317",
    "project_service.observability.otlp_endpoint": "http://127.0.0.1:4317",
    "user_service.observability.otlp_endpoint": "http://127.0.0.1:4317",
    "mcp_management.observability.otlp_endpoint": "http://127.0.0.1:4317",
    "mcp_management.async_tool.dispatch_mode": "rabbitmq",
    "mcp_management.async_tool.rabbitmq_url": rabbitmq_url,
    "mcp_management.security.allowed_internal_callers": "chatos,task-runner,project-service,configuration-center",
    "local_connector.coordination.valkey_url": valkey_url,
    "chatos.observability.otlp_endpoint": "http://127.0.0.1:4317",
})

legacy_user_service_secret = os.environ.get("USER_SERVICE_JWT_SECRET", "").strip()
if legacy_user_service_secret:
    previous_key = "user_service.security.previous_secret_keys"
    existing = str(effective.get(previous_key) or "")
    materials = []
    for item in [*existing.replace(";", ",").split(","), legacy_user_service_secret]:
        normalized = item.strip()
        if normalized and normalized not in materials:
            materials.append(normalized)
    desired[previous_key] = ",".join(materials)

changes = {key: value for key, value in desired.items() if effective.get(key) != value}
with open(desired_path, "w", encoding="utf-8") as handle:
    json.dump(changes, handle, separators=(",", ":"))
PY
  change_count="$(wc -c <"$desired_file" | tr -d ' ')"
  if [[ "$change_count" == "2" ]]; then
    echo "[INFO] configuration center already matches local-dev runtime settings"
    rm -f "$catalog_file" "$effective_file" "$draft_file" "$desired_file" "$merge_file" "$validation_file"
    return 0
  fi

  curl -fsS \
    -H "authorization: Bearer $token" \
    "${config_center_base_url}/api/config/v1/environments/${environment}/draft" \
    >"$draft_file"
  python3 - "$draft_file" "$desired_file" >"$merge_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    response = json.load(handle)
with open(sys.argv[2], encoding="utf-8") as handle:
    desired = json.load(handle)
changes = {}
draft = response.get("draft")
if isinstance(draft, dict) and isinstance(draft.get("changes"), dict):
    changes.update(draft["changes"])
changes.update(desired)
print(json.dumps({"changes": changes}, separators=(",", ":")))
PY
  curl -fsS \
    -X PUT \
    -H "authorization: Bearer $token" \
    -H "content-type: application/json" \
    --data-binary "@$merge_file" \
    "${config_center_base_url}/api/config/v1/environments/${environment}/draft" \
    >/dev/null
  curl -fsS \
    -X POST \
    -H "authorization: Bearer $token" \
    -H "content-type: application/json" \
    --data '{}' \
    "${config_center_base_url}/api/config/v1/environments/${environment}/draft/validate" \
    >"$validation_file"
  if ! python3 - "$validation_file" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    result = json.load(handle)
if not result.get("valid"):
    print("; ".join(result.get("errors") or ["configuration validation failed"]), file=sys.stderr)
    raise SystemExit(1)
PY
  then
    rm -f "$catalog_file" "$effective_file" "$draft_file" "$desired_file" "$merge_file" "$validation_file"
    return 1
  fi
  curl -fsS \
    -X POST \
    -H "authorization: Bearer $token" \
    -H "content-type: application/json" \
    --data '{"message":"Synchronize local-dev host runtime configuration"}' \
    "${config_center_base_url}/api/config/v1/environments/${environment}/draft/publish" \
    >/dev/null
  LOCAL_DEV_CONFIG_CHANGED=true
  rm -f "$catalog_file" "$effective_file" "$draft_file" "$desired_file" "$merge_file" "$validation_file"
  echo "[INFO] published local-dev host runtime settings to configuration center"
}

issue_config_center_token() {
  local caller="$1"
  local scope="$2"
  local secret
  secret="$(config_center_caller_signing_secret "$caller")"
  CONFIG_CENTER_TOKEN_CALLER="$caller" \
    CONFIG_CENTER_TOKEN_SCOPE="$scope" \
    CONFIG_CENTER_TOKEN_SECRET="$secret" \
    python3 <<'PY'
import base64
import hashlib
import hmac
import json
import os
import time
import uuid

def encode(value):
    payload = json.dumps(value, separators=(",", ":")).encode()
    return base64.urlsafe_b64encode(payload).rstrip(b"=").decode()

caller = os.environ["CONFIG_CENTER_TOKEN_CALLER"]
now = int(time.time())
header = encode({"alg": "HS256", "typ": "JWT"})
payload = encode({
    "iss": caller,
    "sub": caller,
    "caller": caller,
    "aud": "configuration-center",
    "scope": os.environ["CONFIG_CENTER_TOKEN_SCOPE"],
    "trace_id": str(uuid.uuid4()),
    "iat": now,
    "exp": now + 60,
})
signature = hmac.new(
    os.environ["CONFIG_CENTER_TOKEN_SECRET"].encode(),
    f"{header}.{payload}".encode(),
    hashlib.sha256,
).digest()
print(f"{header}.{payload}.{base64.urlsafe_b64encode(signature).rstrip(b'=').decode()}")
PY
}

wait_for_config_center_mtls() {
  local token deadline
  deadline=$((SECONDS + ${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}))
  while (( SECONDS < deadline )); do
    token="$(issue_config_center_token user-service config.snapshot.read)"
    if curl -fsS \
      --max-time 3 \
      --cacert "$CONFIG_CENTER_MTLS_CA_CERT_PATH" \
      --cert "$(config_center_client_identity_path user-service)" \
      -H "x-config-center-caller: user-service" \
      -H "x-config-center-internal-token: $token" \
      "${CONFIG_CENTER_BASE_URL%/}/internal/config/v1/snapshots/user-service?environment=${CHATOS_ENV}" \
      >/dev/null
    then
      echo "[INFO] Configuration Center internal mTLS endpoint is ready"
      return 0
    fi
    sleep 1
  done
  echo "[ERROR] Configuration Center internal mTLS endpoint did not become ready" >&2
  return 1
}

ensure_local_connector_control_plane_config() {
  local config_center_internal_base_url="$CONFIG_CENTER_BASE_URL"
  local config_center_public_base_url="http://127.0.0.1:${CONFIG_CENTER_PORT}"
  local environment="${CHATOS_ENV:-local}"
  local key_dir="$STATE_DIR/local-connector"
  local key_path="$key_dir/relay-signing-key.pk8"
  local key_id="${CHATOS_LOCAL_DEV_RELAY_SIGNING_KEY_ID:-relay-key-local-dev}"
  local public_key snapshot_file draft_file merge_file token desired_json snapshot_auth_token
  local login_payload login_response

  mkdir -p "$key_dir"
  if [[ ! -f "$key_path" ]]; then
    echo "[INFO] generating local connector relay signing key"
  fi
  if ! public_key="$(
    cargo run --quiet \
      --manifest-path "$ROOT_DIR/local_connector_service/backend/Cargo.toml" \
      --bin local_connector_dev_relay_keygen \
      -- "$key_path"
  )"; then
    echo "[ERROR] generate local connector relay signing key failed" >&2
    return 1
  fi

  desired_json="$(
    python3 - "$key_path" "$key_id" "$public_key" <<'PY'
import json
import sys

key_path, key_id, public_key = sys.argv[1:]
print(json.dumps({
    "local_connector.security.relay_signing.key_path": key_path,
    "local_connector.security.relay_signing.key_id": key_id,
    "local_connector.remote_control.require_signed_messages": True,
    "local_connector.remote_control.signature_max_skew_seconds": 300,
    "local_connector.remote_control.trusted_relay_public_keys": {
        key_id: public_key,
    },
}, separators=(",", ":")))
PY
  )"

  snapshot_file="$(mktemp)"
  snapshot_auth_token="$(issue_config_center_token local-connector-service config.snapshot.read)"
  if curl -fsS \
    --cacert "$CONFIG_CENTER_MTLS_CA_CERT_PATH" \
    --cert "$(config_center_client_identity_path local-connector-service)" \
    -H "x-config-center-caller: local-connector-service" \
    -H "x-config-center-internal-token: $snapshot_auth_token" \
    "${config_center_internal_base_url}/internal/config/v1/snapshots/local-connector-service?environment=${environment}" \
    >"$snapshot_file"; then
    if python3 - "$snapshot_file" "$desired_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    snapshot = json.load(handle)
desired = json.loads(sys.argv[2])
values = snapshot.get("values") or {}
matches = all(values.get(key) == value for key, value in desired.items())
raise SystemExit(0 if matches else 1)
PY
    then
      echo "[INFO] local connector managed config already matches local-dev relay settings"
      rm -f "$snapshot_file"
      return 0
    fi
  fi
  rm -f "$snapshot_file"

  login_payload="$(python3 - <<'PY'
import json
import os
print(json.dumps({
    "username": os.environ["CHATOS_ADMIN_USERNAME"],
    "password": os.environ["CHATOS_ADMIN_PASSWORD"],
}, separators=(",", ":")))
PY
  )"
  if ! login_response="$(
    curl -fsS \
      -H "content-type: application/json" \
      --data "$login_payload" \
      "${config_center_public_base_url}/api/auth/login"
  )"; then
    echo "[ERROR] configuration center admin login failed during Local Connector relay trust bootstrap" >&2
    return 1
  fi
  if ! token="$(printf '%s' "$login_response" | python3 -c 'import json, sys; print(json.load(sys.stdin)["token"])')"; then
    echo "[ERROR] configuration center admin login returned an invalid response during Local Connector relay trust bootstrap" >&2
    return 1
  fi

  draft_file="$(mktemp)"
  merge_file="$(mktemp)"
  curl -fsS \
    -H "authorization: Bearer $token" \
    "${config_center_public_base_url}/api/config/v1/environments/${environment}/draft" \
    >"$draft_file"
  python3 - "$draft_file" "$desired_json" >"$merge_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    response = json.load(handle)
changes = {}
draft = response.get("draft")
if isinstance(draft, dict) and isinstance(draft.get("changes"), dict):
    changes.update(draft["changes"])
changes.update(json.loads(sys.argv[2]))
print(json.dumps({"changes": changes}, separators=(",", ":")))
PY
  curl -fsS \
    -X PUT \
    -H "authorization: Bearer $token" \
    -H "content-type: application/json" \
    --data-binary "@$merge_file" \
    "${config_center_public_base_url}/api/config/v1/environments/${environment}/draft" \
    >/dev/null
  curl -fsS \
    -X POST \
    -H "authorization: Bearer $token" \
    -H "content-type: application/json" \
    --data '{"message":"Local dev bootstrap Local Connector relay trust"}' \
    "${config_center_public_base_url}/api/config/v1/environments/${environment}/draft/publish" \
    >/dev/null
  rm -f "$draft_file" "$merge_file"
  echo "[INFO] published local connector managed relay trust settings to configuration center"
}

ensure_managed_queue_consumers() {
  local timeout_seconds="${CHATOS_LOCAL_DEV_QUEUE_TIMEOUT_SECONDS:-60}"
  local elapsed=0
  local queue_table
  local rabbitmq_container="${COMPOSE_PROJECT_NAME}-rabbitmq-1"
  local -a required_queues=(
    "mcp_management.async.dispatch"
    "cloud_agent.task_runner.runtime"
    "task_runner.run.post_process"
    "task_runner.callback.delivery"
  )

  while (( elapsed < timeout_seconds )); do
    queue_table="$(
      docker exec "$rabbitmq_container" \
        rabbitmqctl -q list_queues name consumers 2>/dev/null || true
    )"
    if python3 - "$queue_table" "${required_queues[@]}" <<'PY'
import sys

rows = {}
for line in sys.argv[1].splitlines():
    parts = line.split("\t")
    if len(parts) == 2:
        try:
            rows[parts[0]] = int(parts[1])
        except ValueError:
            pass
missing = [name for name in sys.argv[2:] if rows.get(name, 0) < 1]
raise SystemExit(0 if not missing else 1)
PY
    then
      echo "[OK] RabbitMQ managed queues have active consumers"
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  echo "[ERROR] RabbitMQ managed queues did not acquire consumers within ${timeout_seconds}s" >&2
  printf '%s\n' "$queue_table" >&2
  return 1
}

ensure_frontend_dependencies() {
  local app_dir="$1"
  local app_path="$ROOT_DIR/$app_dir"
  local installed_lock="$app_path/node_modules/.package-lock.json"

  if [[ ! -d "$app_path/node_modules" ]] \
    || [[ ! -f "$installed_lock" ]] \
    || [[ "$app_path/package.json" -nt "$installed_lock" ]] \
    || [[ -f "$app_path/package-lock.json" && "$app_path/package-lock.json" -nt "$installed_lock" ]]; then
    echo "[INFO] refreshing frontend dependencies: $app_dir"
    (
      cd "$app_path"
      if [[ -f package-lock.json ]]; then
        npm ci --legacy-peer-deps
      else
        npm install --legacy-peer-deps
      fi
    )
  fi
}

start_frontend() {
  local name="$1"
  local app_dir="$2"
  local port="$3"
  local log_file pid_file
  ensure_frontend_dependencies "$app_dir"
  log_file="$(log_file_for "$name")"
  pid_file="$(pid_file_for "$name")"
  stop_service_pid "$name"
  stop_port_if_needed "$port" "$name"
  echo "[INFO] starting $name on 0.0.0.0:$port"
  : >"$log_file"
  local spawned_pid
  spawned_pid="$(
    if [[ "$name" == "chatos-frontend" ]]; then
      export VITE_API_BASE_URL="http://127.0.0.1:${APISIX_GATEWAY_PORT:-9080}/api/chatos"
    fi
    spawn_detached "$ROOT_DIR/$app_dir" "$log_file" npm run dev -- --host 0.0.0.0 --port "$port" --strictPort
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_port "$name" "$port" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}"
}

cleanup_legacy_local_connector_client_state() {
  # Older local-dev versions owned these processes. Stop only PIDs recorded by
  # that old stack; never kill ports now owned by the standalone client target.
  stop_service_pid "local-connector-client-frontend"
  stop_service_pid "local-connector-client-core"
}

start_all() {
  need_cmd cargo
  need_cmd npm
  need_cmd curl
  need_cmd python3
  load_env_file "$ENV_FILE"
  load_env_file "${CHATOS_LOCAL_DEV_OBJECT_STORAGE_ENV_FILE:-$STATE_DIR/object-storage.env}"
  export_local_env
  ensure_dirs
  ensure_config_center_mtls_material
  ensure_mcp_management_mtls_material
  ensure_task_runner_mtls_material
  ensure_project_service_mtls_material
  ensure_chatos_mtls_material
  ensure_local_connector_mtls_material
  ensure_user_service_mtls_material
  ensure_memory_engine_mtls_material
  ensure_plugin_management_mtls_material
  prepare_local_dev_apisix_config
  cleanup_legacy_local_connector_client_state
  start_infra
  wait_for_consul
  deregister_local_dev_services
  stop_docker_app_services
  cleanup_local_dev_processes
  deregister_local_dev_services
  register_local_dev_harness_service

  local item name service_name package health_path port bin env_overrides app_dir
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name service_name package health_path port bin env_overrides <<<"$item"
    if [[ "$name" == "local-connector-service-backend" ]]; then
      ensure_local_connector_control_plane_config
    fi
    start_backend "$name" "$service_name" "$package" "$health_path" "$port" "$bin" "$env_overrides"
    if [[ "$name" == "configuration-center-backend" ]]; then
      wait_for_config_center_mtls
    fi
    if [[ "$name" == "user-service-backend" ]]; then
      ensure_local_dev_managed_runtime_config
      if [[ "$LOCAL_DEV_CONFIG_CHANGED" == "true" ]]; then
        echo "[INFO] restarting user-service-backend after managed configuration publication"
        start_backend "$name" "$service_name" "$package" "$health_path" "$port" "$bin" "$env_overrides"
      fi
    fi
    if [[ "$name" == "task-runner-scheduler" ]]; then
      ensure_managed_queue_consumers
    fi
  done
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name app_dir port <<<"$item"
    start_frontend "$name" "$app_dir" "$port"
  done
  print_urls
}

stop_all() {
  load_env_file "$ENV_FILE"
  load_env_file "${CHATOS_LOCAL_DEV_OBJECT_STORAGE_ENV_FILE:-$STATE_DIR/object-storage.env}"
  export_local_env
  ensure_dirs
  stop_service_pid "sandbox-runtime-proxy"
  cleanup_legacy_local_connector_client_state
  deregister_local_dev_services
  local item name unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused port <<<"$item"
    stop_service_pid "$name"
    stop_port_if_needed "$port" "$name"
  done
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused unused <<<"$item"
    stop_service_pid "$name"
    if [[ -n "$port" && "$port" != "-" ]]; then
      stop_port_if_needed "$port" "$name"
      if [[ "$name" == "memory-engine-backend" ]]; then
        stop_port_if_needed "$MEMORY_ENGINE_INTERNAL_MTLS_PORT" "$name internal mTLS"
      fi
      if [[ "$name" == "project-management-backend" ]]; then
        stop_port_if_needed "$PROJECT_SERVICE_INTERNAL_MTLS_PORT" "$name internal mTLS"
      fi
      if [[ "$name" == "user-service-backend" ]]; then
        stop_port_if_needed "$USER_SERVICE_INTERNAL_MTLS_PORT" "$name internal mTLS"
      fi
    fi
  done
  cleanup_local_dev_processes
  deregister_local_dev_services
}

status_all() {
  ensure_dirs
  local item name port pid unused container_status
  echo "[INFO] local dev stack status"
  echo
  echo "Docker infrastructure (compose project: $COMPOSE_PROJECT_NAME)"
  for name in "${INFRA_SERVICES[@]}"; do
    container_status="$(
      docker ps -a \
        --filter "label=com.docker.compose.project=$COMPOSE_PROJECT_NAME" \
        --filter "label=com.docker.compose.service=$name" \
        --format '{{.Status}}' \
        | head -n 1
    )"
    port="$(infra_service_host_port "$name" 2>/dev/null || printf '%s' '-')"
    if [[ -n "$container_status" ]]; then
      printf '  %-36s port=%-5s %s\n' "$name" "$port" "$container_status"
    else
      printf '  %-36s port=%-5s not created\n' "$name" "$port"
    fi
  done
  echo
  echo "Host-side services"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused unused <<<"$item"
    if [[ -n "$port" && "$port" != "-" ]]; then
      pid="$(pid_for_port "$port")"
    else
      pid=""
      if [[ -f "$(pid_file_for "$name")" ]]; then
        pid="$(cat "$(pid_file_for "$name")")"
        if ! kill -0 "$pid" 2>/dev/null; then
          pid=""
        fi
      fi
    fi
    if [[ -n "$pid" ]]; then
      printf '  %-36s port=%-5s running pid=%s\n' "$name" "$port" "$pid"
    else
      printf '  %-36s port=%-5s not listening\n' "$name" "$port"
    fi
  done
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name _ port <<<"$item"
    pid="$(pid_for_port "$port")"
    if [[ -n "$pid" ]]; then
      printf '  %-36s port=%-5s running pid=%s\n' "$name" "$port" "$pid"
    else
      printf '  %-36s port=%-5s not listening\n' "$name" "$port"
    fi
  done
  echo
  echo "Logs: $LOG_DIR"
}

logs_for() {
  local name="${1:-}"
  if [[ -z "$name" ]]; then
    ls -1 "$LOG_DIR" 2>/dev/null || true
    echo
    echo "Usage: $0 logs <service-name>"
    return 0
  fi
  tail -f "$(log_file_for "$name")"
}

print_urls() {
  cat <<EOF

[OK] Local dev stack startup requested.

Main app:                 http://localhost:8088
Unified gateway:          http://localhost:${APISIX_GATEWAY_PORT:-9080}
Prometheus:               http://127.0.0.1:${PROMETHEUS_PORT:-9090}
Alertmanager:             http://127.0.0.1:${ALERTMANAGER_PORT:-9093}
Grafana:                  http://127.0.0.1:${GRAFANA_PORT:-3001}
Main backend:             http://localhost:3997
Configuration Center:     http://localhost:39271
Harness:                  http://localhost:3000
User Service:             http://localhost:39191
Memory Engine:            http://localhost:4178
Task Runner:              http://localhost:39091
Project Management:       http://localhost:39211
Plugin Management:        http://localhost:39261
Local Connector Service:  http://localhost:39230
MCP Management Service:   http://localhost:39280

Status:  $0 status
Logs:    $0 logs <service-name>
Stop:    $0 down

The Local Connector client is managed separately:
  make local-connector-client
EOF
}
