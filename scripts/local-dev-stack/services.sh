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

start_frontend() {
  local name="$1"
  local app_dir="$2"
  local port="$3"
  local log_file pid_file
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
Harness:                  http://localhost:3000
User Service:             http://localhost:39191
Memory Engine:            http://localhost:4178
Task Runner:              http://localhost:39091
Project Management:       http://localhost:39211
Plugin Management:        http://localhost:39261
Sandbox Manager:          http://localhost:8096
Local Connector Service:  http://localhost:39230
Official Website:         http://localhost:39251

Status:  $0 status
Logs:    $0 logs <service-name>
Stop:    $0 down

The Local Connector client is managed separately:
  make local-connector-client
EOF
}
