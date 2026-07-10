#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${CHATOS_DOCKER_ENV_FILE:-$SCRIPT_DIR/.env}"
ENV_EXAMPLE_FILE="$SCRIPT_DIR/.env.example"
IMAGE_NAMESPACE="${CHATOS_IMAGE_NAMESPACE:-ghcr.io/leeoohoo}"
IMAGE_TAG="${CHATOS_IMAGE_TAG:-harness-ci}"
PUBLIC_HOST="${CHATOS_PUBLIC_HOST:-8.155.171.124}"

ensure_env_file() {
  if [[ -f "$ENV_FILE" ]]; then
    return 0
  fi
  cp "$ENV_EXAMPLE_FILE" "$ENV_FILE"
}

set_env() {
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
  ' "$ENV_FILE" >"$tmp"
  mv "$tmp" "$ENV_FILE"
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

ensure_env_file
set_env CHATOS_IMAGE_NAMESPACE "$IMAGE_NAMESPACE"
set_env CHATOS_IMAGE_TAG "$IMAGE_TAG"
set_env SANDBOX_MANAGER_DOCKER_IMAGE "$IMAGE_NAMESPACE/chatos-rs-sandbox-agent:$IMAGE_TAG"
set_env CHATOS_DOCKER_EXTRA_COMPOSE_FILES ""
set_env HARNESS_PORT "3000"
set_env HARNESS_SSH_PORT "3022"
set_env HARNESS_PUBLIC_BASE_URL "http://$PUBLIC_HOST:3000"
set_env HARNESS_GIT_BASE_URL "http://$PUBLIC_HOST:3000/git"
set_env HARNESS_SSH_PUBLIC_HOST "$PUBLIC_HOST"
set_env HARNESS_BASE_URL "http://harness:3000"

require_local_image "$IMAGE_NAMESPACE/chatos-rs-chatos-backend:$IMAGE_TAG"
require_local_image "$IMAGE_NAMESPACE/chatos-rs-sandbox-agent:$IMAGE_TAG"
require_local_image "harness/harness:latest"

echo "[INFO] using local Harness CI images: $IMAGE_NAMESPACE/*:$IMAGE_TAG"
echo "[INFO] starting the Chatos Docker stack, including the business Harness on port 3000"
exec "$SCRIPT_DIR/deploy.sh" fast "$@"
