#!/usr/bin/env bash
# Local validation sourced by deploy-production.sh.

validate_deploy_preflight() {
  if [[ -n "$DEPLOY_WECHAT_DEVELOPMENT_LOGIN_ENABLED" ]] \
    && [[ "$DEPLOY_WECHAT_DEVELOPMENT_LOGIN_ENABLED" != "true" ]] \
    && [[ "$DEPLOY_WECHAT_DEVELOPMENT_LOGIN_ENABLED" != "false" ]]; then
    echo "[ERROR] CHATOS_DEPLOY_WECHAT_DEVELOPMENT_LOGIN_ENABLED must be true or false" >&2
    exit 2
  fi

  need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
      echo "[ERROR] missing command: $1" >&2
      exit 1
    fi
  }

  need_cmd git
  need_cmd ssh
  need_cmd bash
  need_cmd cargo

  cd "$ROOT_DIR"

  if [[ -n "$DEPLOY_SERVICES_CSV" ]]; then
    IFS=',' read -r -a requested_services <<< "$DEPLOY_SERVICES_CSV"
    available_services="$(./docker/deploy.sh build-services)"
    normalized_services=()
    for service in "${requested_services[@]}"; do
      service="$(printf '%s' "$service" | xargs)"
      [[ -n "$service" ]] || continue
      if [[ "$service" == "gateway-config" ]]; then
        normalized_services+=("$service")
        continue
      fi
      if ! grep -Fxq "$service" <<< "$available_services"; then
        echo "[ERROR] service is not independently buildable: $service" >&2
        echo "Available services:" >&2
        printf '%s\n' "$available_services" >&2
        printf '%s\n' "gateway-config" >&2
        exit 2
      fi
      normalized_services+=("$service")
    done
    if [[ ${#normalized_services[@]} -eq 0 ]]; then
      echo "[ERROR] CHATOS_DEPLOY_SERVICES did not contain a valid service" >&2
      exit 2
    fi
    DEPLOY_SERVICES_CSV="$(IFS=,; printf '%s' "${normalized_services[*]}")"
    echo "[INFO] selected production services: $DEPLOY_SERVICES_CSV"
  else
    echo "[INFO] selected production scope: all cloud services"
  fi
}
