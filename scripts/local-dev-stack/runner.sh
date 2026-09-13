#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  echo "[ERROR] runner.sh must be sourced by a stack entrypoint" >&2
  exit 2
fi

STACK_RUNNER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_DIR="$(cd "$STACK_RUNNER_DIR/.." && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
STACK_PROFILE_PATH="$STACK_RUNNER_DIR/profiles/${STACK_PROFILE:?STACK_PROFILE is required}.sh"

if [[ ! -f "$STACK_PROFILE_PATH" ]]; then
  echo "[ERROR] unknown local stack profile: $STACK_PROFILE" >&2
  return 2
fi

# shellcheck disable=SC1090
source "$STACK_PROFILE_PATH"

# shellcheck source=local-dev-stack/service-catalog.sh
source "$STACK_RUNNER_DIR/service-catalog.sh"
resolve_stack_service_profile

# macOS still ships Bash 3.2, where an explicitly declared empty array raises
# an unbound-variable error under `set -u`. Probe element zero before expanding
# an optional profile array.
stack_has_frontends() {
  [[ "${FRONTEND_SERVICES[0]+present}" == "present" ]]
}

STATE_DIR="${CHATOS_LOCAL_DEV_STATE_DIR:-$ROOT_DIR/.chatos-local-dev}"
LOG_DIR="$STATE_DIR/logs"
PID_DIR="$STATE_DIR/pids"
ENV_FILE="${CHATOS_LOCAL_DEV_BOOTSTRAP_FILE:-$ROOT_DIR/docker/bootstrap.conf}"
PLATFORM_COMPOSE_FILE="$ROOT_DIR/docker/compose.platform.yml"
LOCAL_DEV_COMPOSE_FILE="$ROOT_DIR/docker/compose.local-dev.yml"
COMPOSE_FILES=("$ROOT_DIR/docker/compose.yml")
if [[ -n "${CHATOS_LOCAL_DEV_EXTRA_COMPOSE_FILES:-}" ]]; then
  IFS=':' read -r -a extra_compose_files <<< "${CHATOS_LOCAL_DEV_EXTRA_COMPOSE_FILES}"
  COMPOSE_FILES+=("${extra_compose_files[@]}")
elif [[ -n "${CHATOS_DOCKER_EXTRA_COMPOSE_FILES:-}" ]]; then
  IFS=':' read -r -a extra_compose_files <<< "${CHATOS_DOCKER_EXTRA_COMPOSE_FILES}"
  COMPOSE_FILES+=("${extra_compose_files[@]}")
elif [[ -f "$PLATFORM_COMPOSE_FILE" ]]; then
  COMPOSE_FILES+=("$PLATFORM_COMPOSE_FILE")
fi
if [[ -f "$LOCAL_DEV_COMPOSE_FILE" ]]; then
  COMPOSE_FILES+=("$LOCAL_DEV_COMPOSE_FILE")
fi
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-chatos-rs}"
export CHATOS_LOCAL_DEV_APISIX_CONFIG_PATH="${CHATOS_LOCAL_DEV_APISIX_CONFIG_PATH:-$STATE_DIR/apisix.yaml}"

# shellcheck source=local-dev-stack/support.sh
source "$STACK_RUNNER_DIR/support.sh"
# shellcheck source=local-dev-stack/environment.sh
source "$STACK_RUNNER_DIR/environment.sh"
# shellcheck source=local-dev-stack/services.sh
source "$STACK_RUNNER_DIR/services.sh"

stack_usage() {
  cat <<EOF
Usage: $STACK_COMMAND [up|restart|down|status|logs <service-name>|services]

Profile: $STACK_DISPLAY_NAME
EOF
}

print_stack_services() {
  local item name service_name unused port
  echo "$STACK_DISPLAY_NAME"
  echo
  echo "Infrastructure:"
  printf '  %s\n' "${INFRA_SERVICES[@]}"
  echo
  echo "Host-side services:"
  for item in "${BACKEND_SERVICES[@]}"; do
    IFS='|' read -r name service_name unused unused port unused unused <<<"$item"
    printf '  %-36s service=%-28s port=%s\n' "$name" "$service_name" "$port"
  done
  if stack_has_frontends; then
    echo
    echo "Frontends:"
    for item in "${FRONTEND_SERVICES[@]}"; do
      IFS='|' read -r name unused port <<<"$item"
      printf '  %-36s port=%s\n' "$name" "$port"
    done
  fi
}

run_local_stack() {
  local action="${1:-up}"
  case "$action" in
    up|start)
      start_all
      ;;
    restart)
      stop_all
      start_all
      ;;
    down|stop)
      stop_all
      ;;
    status|ps)
      status_all
      ;;
    logs)
      shift || true
      logs_for "${1:-}"
      ;;
    services)
      print_stack_services
      ;;
    -h|--help|help)
      stack_usage
      ;;
    *)
      stack_usage >&2
      return 2
      ;;
  esac
}
