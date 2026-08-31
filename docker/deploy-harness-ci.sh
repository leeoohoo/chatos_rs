#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOOTSTRAP_FILE="${CHATOS_DOCKER_BOOTSTRAP_FILE:-$SCRIPT_DIR/bootstrap.conf}"
BOOTSTRAP_EXAMPLE_FILE="$SCRIPT_DIR/bootstrap.conf.example"
IMAGE_NAMESPACE="${CHATOS_IMAGE_NAMESPACE:-ghcr.io/leeoohoo}"
IMAGE_TAG="${CHATOS_IMAGE_TAG:-harness-ci}"
PUBLIC_HOST="${CHATOS_PUBLIC_HOST:-8.155.171.124}"

ensure_bootstrap_file() {
  if [[ -f "$BOOTSTRAP_FILE" ]]; then
    return 0
  fi
  cp "$BOOTSTRAP_EXAMPLE_FILE" "$BOOTSTRAP_FILE"
}

set_bootstrap_value() {
  local key="$1"
  local value="$2"
  local tmp
  tmp="$(mktemp)"
  awk -v key="$key" -v value="$value" '
    BEGIN { written = 0 }
    $0 ~ "^" key "=" {
      print key "=" value
      written = 1
      next
    }
    { print }
    END {
      if (!written) {
        print key "=" value
      }
    }
  ' "$BOOTSTRAP_FILE" >"$tmp"
  mv "$tmp" "$BOOTSTRAP_FILE"
}

require_local_image() {
  local image="$1"
  if docker image inspect "$image" >/dev/null 2>&1; then
    return 0
  fi
  echo "[ERROR] local image not found: $image" >&2
  echo "        Run the Harness CI image pipeline on this server first, or push/pull the image tag." >&2
  exit 1
}

ensure_runtime_image() {
  local image="$1"
  if docker image inspect "$image" >/dev/null 2>&1; then
    return 0
  fi
  echo "[INFO] pulling required runtime image: $image"
  docker pull "$image"
}

image_for_service() {
  case "$1" in
    user-service-backend)
      printf '%s/chatos-rs-user-service-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    configuration-center-backend)
      printf '%s/chatos-rs-configuration-center-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    memory-engine-backend)
      printf '%s/chatos-rs-memory-engine-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    project-management-backend)
      printf '%s/chatos-rs-project-management-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    plugin-management-backend)
      printf '%s/chatos-rs-plugin-management-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    local-connector-service-backend)
      printf '%s/chatos-rs-local-connector-service-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    task-runner-backend)
      printf '%s/chatos-rs-task-runner-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    chatos-backend)
      printf '%s/chatos-rs-chatos-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    official-website-backend)
      printf '%s/chatos-rs-official-website-backend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    admin-console-frontend)
      printf '%s/chatos-rs-admin-console-frontend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    official-website-frontend)
      printf '%s/chatos-rs-official-website-frontend:%s\n' "$IMAGE_NAMESPACE" "$IMAGE_TAG"
      ;;
    *)
      echo "[ERROR] unsupported Harness CI image service: $1" >&2
      exit 2
      ;;
  esac
}

require_harness_ci_images_for_services() {
  local service
  local image
  if [[ $# -eq 0 ]]; then
    require_harness_ci_images
    return 0
  fi
  for service in "$@"; do
    image="$(image_for_service "$service")"
    require_local_image "$image"
  done
}

require_harness_ci_images() {
  local required_images=(
    "$IMAGE_NAMESPACE/chatos-rs-configuration-center-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-user-service-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-memory-engine-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-project-management-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-plugin-management-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-local-connector-service-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-task-runner-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-chatos-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-official-website-backend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-admin-console-frontend:$IMAGE_TAG"
    "$IMAGE_NAMESPACE/chatos-rs-official-website-frontend:$IMAGE_TAG"
    "harness/harness:latest"
  )
  local image
  for image in "${required_images[@]}"; do
    require_local_image "$image"
  done
}

ensure_bootstrap_file
set_bootstrap_value CHATOS_IMAGE_NAMESPACE "$IMAGE_NAMESPACE"
set_bootstrap_value CHATOS_IMAGE_TAG "$IMAGE_TAG"
set_bootstrap_value CHATOS_DOCKER_EXTRA_COMPOSE_FILES ""
set_bootstrap_value HARNESS_PORT "3000"
set_bootstrap_value HARNESS_SSH_PORT "3022"
set_bootstrap_value HARNESS_PUBLIC_BASE_URL "http://$PUBLIC_HOST:3000"
set_bootstrap_value HARNESS_GIT_BASE_URL "http://$PUBLIC_HOST:3000/git"
set_bootstrap_value HARNESS_SSH_PUBLIC_HOST "$PUBLIC_HOST"
set_bootstrap_value HARNESS_BASE_URL "http://harness:3000"

case "${1:-}" in
  check-images|verify-images)
    shift || true
    require_harness_ci_images_for_services "$@"
    if [[ $# -eq 0 ]]; then
      echo "[OK] all Harness CI images are available locally: $IMAGE_NAMESPACE/*:$IMAGE_TAG"
    else
      echo "[OK] selected Harness CI images are available locally: $*"
    fi
    exit 0
    ;;
esac

require_harness_ci_images_for_services "$@"

if [[ $# -eq 0 ]]; then
  echo "[INFO] using local Harness CI images: $IMAGE_NAMESPACE/*:$IMAGE_TAG"
else
  echo "[INFO] using selected local Harness CI images: $*"
fi
echo "[INFO] starting the Chat OS Docker stack, including the business Harness on port 3000"
if [[ $# -eq 0 ]]; then
  exec "$SCRIPT_DIR/deploy.sh" fast
fi
exec "$SCRIPT_DIR/deploy.sh" restart-fast "$@"
