#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

load_env_file() {
  local file="$1"
  if [[ -f "$file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$file"
    set +a
  fi
}

env_value() {
  local key="$1"
  local default_value="$2"
  if [[ -n "${!key:-}" ]]; then
    printf '%s' "${!key}"
  else
    printf '%s' "$default_value"
  fi
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $cmd" >&2
    exit 1
  fi
}

compose() {
  local args=(-p "$COMPOSE_PROJECT_NAME" -f "$COMPOSE_FILE")
  if [[ -f "$ENV_FILE" ]]; then
    args+=(--env-file "$ENV_FILE")
  fi
  docker compose "${args[@]}" "$@"
}

pid_file_for() {
  printf '%s/%s.pid\n' "$PID_DIR" "$1"
}

log_file_for() {
  printf '%s/%s.log\n' "$LOG_DIR" "$1"
}

spawn_detached() {
  local cwd="$1"
  local log_file="$2"
  shift 2
  python3 - "$cwd" "$log_file" "$@" <<'PY'
import os
import subprocess
import sys

cwd = sys.argv[1]
log_path = sys.argv[2]
command = sys.argv[3:]

with open(log_path, "ab", buffering=0) as log:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=os.environ.copy(),
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

print(process.pid)
PY
}

target_binary_for() {
  local bin="$1"
  local target_dir="${CARGO_TARGET_DIR:-$ROOT_DIR/target-shared}"
  local binary="$target_dir/debug/$bin"
  if [[ ! -x "$binary" && -x "$binary.exe" ]]; then
    binary="$binary.exe"
  fi
  printf '%s\n' "$binary"
}

pid_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n 1 || true
  fi
}

pids_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true
  fi
}

repo_managed_pids() {
  python3 - "$ROOT_DIR" <<'PY'
import os
import subprocess
import sys

root = os.path.realpath(sys.argv[1])
service_bins = {
    "user_service_backend",
    "memory_engine",
    "project_management_service_backend",
    "plugin_management_service_backend",
    "local_connector_service_backend",
    "sandbox_manager_service_backend",
    "task_runner_service_backend",
    "chat_app_server_rs",
    "official_website_service_backend",
}

current = os.getpid()
parent = os.getppid()
rows = []
output = subprocess.check_output(["ps", "-axo", "pid=,ppid=,command="], text=True)
for line in output.splitlines():
    parts = line.strip().split(None, 2)
    if len(parts) < 3:
        continue
    pid, ppid, command = int(parts[0]), int(parts[1]), parts[2]
    if pid in {current, parent}:
        continue
    rows.append((pid, ppid, command))

matched = set()
for pid, _ppid, command in rows:
    if root not in command:
        continue
    if "/local_connector_client/" in command:
        continue
    if any(f"/{name}" in command or command.endswith(name) for name in service_bins):
        matched.add(pid)
        continue
    if "/node_modules/.bin/vite" in command or "/node_modules/@esbuild/" in command:
        matched.add(pid)

matched_ppids = {ppid for pid, ppid, _command in rows if pid in matched}
for pid, _ppid, command in rows:
    if pid in matched_ppids and command.startswith("npm run dev"):
        matched.add(pid)

for pid in sorted(matched, reverse=True):
    print(pid)
PY
}

stop_pid() {
  local pid="$1"
  local name="$2"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  echo "[INFO] stopping $name (pid=$pid)"
  kill "-$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  sleep 1
  if kill -0 "$pid" 2>/dev/null; then
    kill -9 "-$pid" 2>/dev/null || true
    kill -9 "$pid" 2>/dev/null || true
  fi
}

stop_service_pid() {
  local name="$1"
  local file
  file="$(pid_file_for "$name")"
  if [[ -f "$file" ]]; then
    stop_pid "$(cat "$file")" "$name"
    rm -f "$file"
  fi
}

stop_port_if_needed() {
  local port="$1"
  local name="$2"
  local pid
  while IFS= read -r pid; do
    if [[ -n "$pid" ]]; then
      stop_pid "$pid" "$name on port $port"
    fi
  done < <(pids_for_port "$port")
}

stop_repo_managed_processes() {
  local pid
  while IFS= read -r pid; do
    if [[ -n "$pid" ]]; then
      stop_pid "$pid" "stale local dev process"
    fi
  done < <(repo_managed_pids)
}

stop_managed_ports() {
  local item name unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r name unused port <<<"$item"
    stop_port_if_needed "$port" "$name"
  done
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name unused unused unused port unused <<<"$item"
    stop_port_if_needed "$port" "$name"
  done
}

managed_ports_busy() {
  local item _name _unused port
  for item in "${FRONTEND_SERVICES[@]}"; do
    IFS='|' read -r _name _unused port <<<"$item"
    if [[ -n "$(pids_for_port "$port")" ]]; then
      return 0
    fi
  done
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r _name _unused _unused _unused port _unused <<<"$item"
    if [[ -n "$(pids_for_port "$port")" ]]; then
      return 0
    fi
  done
  return 1
}

cleanup_local_dev_processes() {
  local attempt
  for attempt in 1 2 3 4 5; do
    stop_repo_managed_processes
    stop_managed_ports
    if ! managed_ports_busy && [[ -z "$(repo_managed_pids)" ]]; then
      return 0
    fi
    sleep 1
  done
}

wait_for_http() {
  local name="$1"
  local url="$2"
  local timeout="${3:-90}"
  local start
  start="$(date +%s)"
  while true; do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "[OK] $name is ready: $url"
      return 0
    fi
    if (( "$(date +%s)" - start >= timeout )); then
      echo "[WARN] $name did not become healthy within ${timeout}s: $url" >&2
      echo "       log: $(log_file_for "$name")" >&2
      return 1
    fi
    sleep 2
  done
}

wait_for_port() {
  local name="$1"
  local port="$2"
  local timeout="${3:-90}"
  local start
  start="$(date +%s)"
  while true; do
    if [[ -n "$(pid_for_port "$port")" ]]; then
      echo "[OK] $name is listening on port $port"
      return 0
    fi
    if (( "$(date +%s)" - start >= timeout )); then
      echo "[WARN] $name did not listen within ${timeout}s on port $port" >&2
      echo "       log: $(log_file_for "$name")" >&2
      return 1
    fi
    sleep 2
  done
}

wait_for_consul() {
  local consul_addr="${CHATOS_CONSUL_HTTP_ADDR:-http://127.0.0.1:8500}"
  wait_for_http "consul" "${consul_addr%/}/v1/status/leader" "${CHATOS_LOCAL_DEV_INFRA_TIMEOUT_SECONDS:-120}" || true
}
