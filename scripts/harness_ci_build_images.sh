#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICES_FILE="$ROOT_DIR/docker/.harness-ci-image-services"

read_selected_services() {
  local services="${CHATOS_CI_IMAGE_SERVICES:-}"
  if [[ -f "$SERVICES_FILE" ]]; then
    services="$(tr '\r\n' '  ' <"$SERVICES_FILE")"
  fi
  # shellcheck disable=SC2086
  printf '%s\n' $services
}

validate_services() {
  local service
  local available_file="$1"
  shift || true
  for service in "$@"; do
    if ! grep -Fx -- "$service" "$available_file" >/dev/null; then
      echo "[ERROR] unsupported image service: $service" >&2
      echo "[INFO] buildable services:" >&2
      sed 's/^/  - /' "$available_file" >&2
      exit 2
    fi
  done
}

main() {
  cd "$ROOT_DIR"

  local available_file
  available_file="$(mktemp)"
  trap 'rm -f "$available_file"' EXIT
  bash docker/deploy.sh build-services >"$available_file"

  local -a selected_services=()
  while IFS= read -r service; do
    if [[ -n "$service" ]]; then
      selected_services+=("$service")
    fi
  done < <(read_selected_services)

  validate_services "$available_file" "${selected_services[@]}"

  if [[ ${#selected_services[@]} -eq 0 ]]; then
    echo "[INFO] Harness image build scope: all services"
    bash docker/deploy.sh build
    bash docker/deploy-harness-ci.sh check-images
  else
    echo "[INFO] Harness image build scope: ${selected_services[*]}"
    bash docker/deploy.sh build "${selected_services[@]}"
    bash docker/deploy-harness-ci.sh check-images "${selected_services[@]}"
  fi

  docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' \
    | grep 'chatos-rs-.*:harness-ci' \
    | sort
}

main "$@"
