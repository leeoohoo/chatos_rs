#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"

load_optional_env() {
  local env_file="$1"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

load_optional_env "$ROOT_DIR/.env"

MEMORY_ENGINE_SCRIPT="$ROOT_DIR/memory_engine/restart_services.sh"
USER_SERVICE_SCRIPT="$ROOT_DIR/user_service/restart_services.sh"
CHATOS_SCRIPT="$ROOT_DIR/restart_services.sh"
TASK_RUNNER_SCRIPT="$ROOT_DIR/restart_task_runner_service.sh"
DB_CONNECTION_HUB_SCRIPT="$ROOT_DIR/db_connection_hub/restart_services.sh"

START_MEMORY_ENGINE="${START_MEMORY_ENGINE:-1}"
START_USER_SERVICE="${START_USER_SERVICE:-1}"
START_CHATOS="${START_CHATOS:-1}"
START_TASK_RUNNER="${START_TASK_RUNNER:-1}"
START_DB_CONNECTION_HUB="${START_DB_CONNECTION_HUB:-1}"

run_enabled() {
  local flag="$1"
  [[ "$flag" == "1" || "$flag" == "true" || "$flag" == "TRUE" || "$flag" == "yes" || "$flag" == "on" ]]
}

invoke_script() {
  local script="$1"
  shift

  if [[ "$script" == "$TASK_RUNNER_SCRIPT" ]]; then
    CHATOS_RS_SHELL_SANITIZED=1 CHATOS_RS_SCRIPT_PATH="$script" bash <(tr -d '\r' < "$script") "$@"
  elif [[ -x "$script" ]]; then
    "$script" "$@"
  else
    bash "$script" "$@"
  fi
}

run_service() {
  local label="$1"
  local script="$2"
  local action="$3"

  echo "[INFO] ${label}: ${action}"
  invoke_script "$script" "$action"
}

run_chatos() {
  local action="$1"
  echo "[INFO] chatos: ${action}"
  START_USER_SERVICE=0 "$CHATOS_SCRIPT" "$action"
}

do_status() {
  if run_enabled "$START_MEMORY_ENGINE"; then
    run_service "memory_engine" "$MEMORY_ENGINE_SCRIPT" status
    echo
  fi
  if run_enabled "$START_USER_SERVICE"; then
    run_service "user_service" "$USER_SERVICE_SCRIPT" status
    echo
  fi
  if run_enabled "$START_DB_CONNECTION_HUB"; then
    run_service "db_connection_hub" "$DB_CONNECTION_HUB_SCRIPT" status
    echo
  fi
  if run_enabled "$START_CHATOS"; then
    run_chatos status
    echo
  fi
  if run_enabled "$START_TASK_RUNNER"; then
    run_service "task_runner" "$TASK_RUNNER_SCRIPT" status
  fi
}

do_stop() {
  local failed=0

  if run_enabled "$START_TASK_RUNNER"; then
    run_service "task_runner" "$TASK_RUNNER_SCRIPT" stop || failed=1
  fi
  if run_enabled "$START_CHATOS"; then
    run_chatos stop || failed=1
  fi
  if run_enabled "$START_DB_CONNECTION_HUB"; then
    run_service "db_connection_hub" "$DB_CONNECTION_HUB_SCRIPT" stop || failed=1
  fi
  if run_enabled "$START_USER_SERVICE"; then
    run_service "user_service" "$USER_SERVICE_SCRIPT" stop || failed=1
  fi
  if run_enabled "$START_MEMORY_ENGINE"; then
    run_service "memory_engine" "$MEMORY_ENGINE_SCRIPT" stop || failed=1
  fi

  return "$failed"
}

do_restart() {
  local started_memory=0
  local started_user=0
  local started_db_hub=0
  local started_chatos=0
  local started_task=0

  do_stop || true

  if run_enabled "$START_MEMORY_ENGINE"; then
    run_service "memory_engine" "$MEMORY_ENGINE_SCRIPT" restart || return 1
    started_memory=1
  fi
  if run_enabled "$START_USER_SERVICE"; then
    run_service "user_service" "$USER_SERVICE_SCRIPT" restart || return 1
    started_user=1
  fi
  if run_enabled "$START_DB_CONNECTION_HUB"; then
    run_service "db_connection_hub" "$DB_CONNECTION_HUB_SCRIPT" restart || return 1
    started_db_hub=1
  fi
  if run_enabled "$START_CHATOS"; then
    run_chatos restart || return 1
    started_chatos=1
  fi
  if run_enabled "$START_TASK_RUNNER"; then
    run_service "task_runner" "$TASK_RUNNER_SCRIPT" restart || return 1
    started_task=1
  fi

  echo "[OK] full stack is running"
  if (( started_memory == 1 )); then
    echo "  memory_engine backend: http://localhost:${MEMORY_ENGINE_PORT:-7081}"
    echo "  memory_engine frontend: http://localhost:${MEMORY_ENGINE_FRONTEND_PORT:-4178}"
  fi
  if (( started_user == 1 )); then
    echo "  user_service backend: http://localhost:${USER_SERVICE_PORT:-39190}"
    echo "  user_service frontend: http://localhost:${USER_SERVICE_FRONTEND_PORT:-39191}"
  fi
  if (( started_db_hub == 1 )); then
    echo "  db_connection_hub backend: http://localhost:${DB_HUB_BACKEND_PORT:-${DB_HUB_PORT:-8099}}"
    echo "  db_connection_hub frontend: http://localhost:${DB_HUB_FRONTEND_PORT:-5174}"
  fi
  if (( started_chatos == 1 )); then
    echo "  chatos backend: http://localhost:${MAIN_BACKEND_PORT:-${BACKEND_PORT:-3997}}"
    echo "  chatos frontend: http://localhost:${FRONTEND_PORT:-8088}"
  fi
  if (( started_task == 1 )); then
    echo "  task_runner backend: http://localhost:${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}"
    echo "  task_runner frontend: http://localhost:${TASK_RUNNER_FRONTEND_PORT:-39091}"
  fi
}

CMD="${1:-restart}"

case "$CMD" in
  restart|start)
    if ! do_restart; then
      echo "[WARN] full stack startup failed, cleaning up..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] full stack stopped"
    ;;
  status)
    do_status
    ;;
  *)
    echo "usage: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
