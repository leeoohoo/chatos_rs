#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOOTSTRAP_FILE="$SCRIPT_DIR/docker/bootstrap.conf"
BOOTSTRAP_EXAMPLE="$SCRIPT_DIR/docker/bootstrap.conf.example"

fail() {
  echo "[ERROR] $*" >&2
  exit 1
}

command -v docker >/dev/null 2>&1 \
  || fail "Docker is not installed. Install Docker Engine and the Compose v2 plugin first."
docker compose version >/dev/null 2>&1 \
  || fail "Docker Compose v2 is unavailable. The required command is: docker compose"
command -v openssl >/dev/null 2>&1 \
  || fail "OpenSSL is not installed. It is required to generate local mTLS certificates."
docker info >/dev/null 2>&1 \
  || fail "Docker is not running, or the current user cannot access the Docker daemon."

if [[ ! -f "$BOOTSTRAP_FILE" ]]; then
  install -m 600 "$BOOTSTRAP_EXAMPLE" "$BOOTSTRAP_FILE"
  echo "[INFO] Created docker/bootstrap.conf from the local deployment template."
  echo "[WARN] The generated configuration is for local/private evaluation only."
  echo "[WARN] Before exposing ChatOS publicly, replace its passwords and follow LINUX_DOCKER_DEPLOY.zh-CN.md."
fi

echo "[INFO] Building the uploaded source and starting ChatOS with Docker Compose."
exec "$SCRIPT_DIR/docker/deploy.sh" dev "$@"
