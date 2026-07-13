#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICES_FILE="$ROOT_DIR/docker/.harness-ci-image-services"
IMAGE_LIST_FILE="${CHATOS_CI_IMAGE_LIST_FILE:-$ROOT_DIR/.chatos-ci-images}"
AVAILABLE_FILE=""

cleanup() {
  if [[ -n "$AVAILABLE_FILE" ]]; then
    rm -f "$AVAILABLE_FILE"
  fi
}

trap cleanup EXIT

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

configure_rust_target_cache() {
  if [[ -n "${CHATOS_CARGO_TARGET_CACHE_ID:-}" ]]; then
    echo "[INFO] Rust target cache: $CHATOS_CARGO_TARGET_CACHE_ID"
    return 0
  fi

  local pipeline_scope="${DRONE_STAGE_NAME:-docker-images}"
  local run_scope="${DRONE_BUILD_NUMBER:-${CI_BUILD_NUMBER:-$(date +%s)}}"
  local cache_scope
  cache_scope="$(printf '%s-%s' "$pipeline_scope" "$run_scope" | tr -c '[:alnum:]_.-' '-')"
  export CHATOS_CARGO_TARGET_CACHE_ID="chatos-rust-target-1.94-j4-${cache_scope}"
  echo "[INFO] Rust target cache: $CHATOS_CARGO_TARGET_CACHE_ID"
}

main() {
  cd "$ROOT_DIR"

  configure_rust_target_cache

  AVAILABLE_FILE="$(mktemp)"
  bash docker/deploy.sh build-services >"$AVAILABLE_FILE"

  local -a selected_services=()
  while IFS= read -r service; do
    if [[ -n "$service" ]]; then
      selected_services+=("$service")
    fi
  done < <(read_selected_services)

  validate_services "$AVAILABLE_FILE" "${selected_services[@]}"

  if [[ ${#selected_services[@]} -eq 0 ]]; then
    echo "[INFO] Harness image build scope: all services"
    bash docker/deploy.sh build
    bash docker/deploy-harness-ci.sh check-images
  else
    echo "[INFO] Harness image build scope: ${selected_services[*]}"
    bash docker/deploy.sh build "${selected_services[@]}"
    bash docker/deploy-harness-ci.sh check-images "${selected_services[@]}"
  fi

  {
    docker images --format '{{.Repository}}:{{.Tag}}' \
      | grep 'chatos-rs-.*:harness-ci' || true
  } | sort -u >"$IMAGE_LIST_FILE"
  if [[ ! -s "$IMAGE_LIST_FILE" ]]; then
    echo "[ERROR] no Harness CI images were recorded for security scanning" >&2
    exit 2
  fi
  while IFS= read -r image; do
    docker image inspect --format '{{.RepoTags}} {{.Size}}' "$image"
  done <"$IMAGE_LIST_FILE"
}

main "$@"
