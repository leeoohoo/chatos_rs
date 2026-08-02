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
  stop_port_if_needed "$port" "$name"
  echo "[INFO] starting $name on 127.0.0.1:$port"
  : >"$log_file"
  (
    cd "$ROOT_DIR"
    cargo "${cargo_args[@]}"
  ) >>"$log_file" 2>&1
  local spawned_pid
  spawned_pid="$(
    export CHATOS_SERVICE_NAME="$service_name"
    export CHATOS_SERVICE_ID="${service_name}-local"
    export CHATOS_SERVICE_PORT="$port"
    export CHATOS_SERVICE_HEALTH_PATH="$health_path"
    spawn_detached "$ROOT_DIR" "$log_file" "$binary"
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_http "$name" "http://127.0.0.1:${port}${health_path}" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
}

ensure_task_runner_sandbox_base_image() {
  local base_url="http://127.0.0.1:${SANDBOX_MANAGER_PORT}"
  local image_id="${TASK_RUNNER_SANDBOX_BASE_IMAGE_ID:-default}"
  local feature_list="${CHATOS_LOCAL_DEV_SANDBOX_BASE_IMAGE_FEATURES:-}"
  local timeout_seconds="${CHATOS_LOCAL_DEV_SANDBOX_IMAGE_TIMEOUT_SECONDS:-900}"
  local catalog_file job_file jobs_file job_id built_image_id status error elapsed
  catalog_file="$(mktemp)"
  job_file="$(mktemp)"
  jobs_file="$(mktemp)"

  if ! curl -fsS \
    -H "x-sandbox-operator-token: $SANDBOX_MANAGER_OPERATOR_TOKEN" \
    "$base_url/api/sandbox-images" >"$catalog_file"; then
    rm -f "$catalog_file" "$job_file" "$jobs_file"
    echo "[ERROR] failed to inspect Sandbox Manager image catalog" >&2
    return 1
  fi
  if python3 - "$catalog_file" "$image_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    catalog = json.load(handle)
image_id = sys.argv[2]
ready = any(
    image.get("id") == image_id and image.get("initialized") is True
    for image in catalog.get("images", [])
)
raise SystemExit(0 if ready else 1)
PY
  then
    echo "[INFO] Task Runner sandbox base image is ready: $image_id"
    rm -f "$catalog_file" "$job_file" "$jobs_file"
    return 0
  fi

  if [[ -z "$feature_list" ]]; then
    rm -f "$catalog_file" "$job_file" "$jobs_file"
    echo "[ERROR] sandbox base image $image_id is missing and CHATOS_LOCAL_DEV_SANDBOX_BASE_IMAGE_FEATURES is empty" >&2
    return 1
  fi

  echo "[INFO] initializing Task Runner sandbox base image: $image_id"
  if ! python3 - "$feature_list" <<'PY' | curl -fsS \
    -H "content-type: application/json" \
    -H "x-sandbox-operator-token: $SANDBOX_MANAGER_OPERATOR_TOKEN" \
    --data-binary @- \
    "$base_url/api/sandbox-images/initialize" >"$job_file"
import json
import sys

features = [item.strip() for item in sys.argv[1].split(",") if item.strip()]
print(json.dumps({"features": features}, separators=(",", ":")))
PY
  then
    rm -f "$catalog_file" "$job_file" "$jobs_file"
    echo "[ERROR] failed to initialize Sandbox Manager image $image_id" >&2
    return 1
  fi

  read -r job_id built_image_id < <(python3 - "$job_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    job = json.load(handle)
print(job.get("id", ""), job.get("image_id", ""))
PY
  )
  if [[ -z "$job_id" || "$built_image_id" != "$image_id" ]]; then
    rm -f "$catalog_file" "$job_file" "$jobs_file"
    echo "[ERROR] sandbox feature selection produced $built_image_id instead of configured image $image_id" >&2
    return 1
  fi

  elapsed=0
  while (( elapsed < timeout_seconds )); do
    if ! curl -fsS \
      -H "x-sandbox-operator-token: $SANDBOX_MANAGER_OPERATOR_TOKEN" \
      "$base_url/api/sandbox-image-jobs" >"$jobs_file"; then
      rm -f "$catalog_file" "$job_file" "$jobs_file"
      echo "[ERROR] failed to inspect Sandbox Manager image job $job_id" >&2
      return 1
    fi
    read -r status error < <(python3 - "$jobs_file" "$job_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    jobs = json.load(handle)
job = next((item for item in jobs if item.get("id") == sys.argv[2]), {})
error = str(job.get("error") or "").replace("\n", " ")
print(job.get("status", "missing"), error)
PY
    )
    case "$status" in
      succeeded)
        echo "[INFO] Task Runner sandbox base image initialized: $image_id"
        rm -f "$catalog_file" "$job_file" "$jobs_file"
        return 0
        ;;
      failed)
        rm -f "$catalog_file" "$job_file" "$jobs_file"
        echo "[ERROR] Sandbox Manager image build failed for $image_id: $error" >&2
        return 1
        ;;
    esac
    sleep 2
    elapsed=$((elapsed + 2))
  done

  rm -f "$catalog_file" "$job_file" "$jobs_file"
  echo "[ERROR] timed out waiting for Sandbox Manager image $image_id" >&2
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
    spawn_detached "$ROOT_DIR/$app_dir" "$log_file" npm run dev -- --host 0.0.0.0 --port "$port" --strictPort
  )"
  echo "$spawned_pid" >"$pid_file"
  wait_for_port "$name" "$port" "${CHATOS_LOCAL_DEV_HEALTH_TIMEOUT_SECONDS:-120}" || true
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
  cleanup_legacy_local_connector_client_state
  start_infra
  wait_for_consul
  deregister_local_dev_services
  stop_docker_app_services
  cleanup_local_dev_processes
  deregister_local_dev_services
  register_local_dev_harness_service

  local item name service_name package health_path port bin app_dir
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name service_name package health_path port bin <<<"$item"
    start_backend "$name" "$service_name" "$package" "$health_path" "$port" "$bin"
    if [[ "$name" == "sandbox-manager-backend" ]]; then
      ensure_task_runner_sandbox_base_image
    fi
  done
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name app_dir port <<<"$item"
    start_frontend "$name" "$app_dir" "$port"
  done
  print_urls
}

stop_all() {
  ensure_dirs
  cleanup_legacy_local_connector_client_state
  deregister_local_dev_services
  local item name unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused port <<<"$item"
    stop_service_pid "$name"
    stop_port_if_needed "$port" "$name"
  done
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    stop_service_pid "$name"
    stop_port_if_needed "$port" "$name"
  done
  cleanup_local_dev_processes
  deregister_local_dev_services
}

status_all() {
  ensure_dirs
  local item name port pid unused
  echo "[INFO] local dev stack status"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    pid="$(pid_for_port "$port")"
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
Main backend:             http://localhost:3997
Configuration Center:     http://localhost:39271
Harness:                  http://localhost:3000
User Service:             http://localhost:39191
Memory Engine:            http://localhost:4178
Task Runner:              http://localhost:39091
Project Management:       http://localhost:39211
Plugin Management:        http://localhost:39261
Sandbox Manager:          http://localhost:8096
Local Connector Service:  http://localhost:39230
MCP Management Service:   http://localhost:39280
Official Website:         http://localhost:39251

Status:  $0 status
Logs:    $0 logs <service-name>
Stop:    $0 down

The Local Connector client is managed separately:
  make local-connector-client
EOF
}
