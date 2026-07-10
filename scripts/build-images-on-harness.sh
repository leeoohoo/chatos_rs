#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export HARNESS_BASE_URL="${HARNESS_BASE_URL:-http://8.155.171.124:3001}"
export HARNESS_GIT_BASE_URL="${HARNESS_GIT_BASE_URL:-http://8.155.171.124:3001/git}"
export HARNESS_ADMIN_LOGIN="${HARNESS_ADMIN_LOGIN:-admin}"
export HARNESS_CI_BRANCH="${HARNESS_CI_BRANCH:-$(git -C "$ROOT_DIR" branch --show-current)}"
export HARNESS_CI_PIPELINE="${HARNESS_CI_PIPELINE:-chatos-rs-images}"
export HARNESS_CI_CONFIG_PATH="${HARNESS_CI_CONFIG_PATH:-.drone.images.yml}"
export HARNESS_CI_SNAPSHOT_SCOPE="${HARNESS_CI_SNAPSHOT_SCOPE:-all}"
export HARNESS_CI_RUN="${HARNESS_CI_RUN:-true}"

if [[ -z "${HARNESS_ADMIN_PASSWORD:-}" ]]; then
  printf 'Harness admin password for %s: ' "$HARNESS_BASE_URL" >&2
  IFS= read -r -s HARNESS_ADMIN_PASSWORD
  export HARNESS_ADMIN_PASSWORD
  printf '\n' >&2
fi

echo "[INFO] sending current worktree snapshot to Harness CI"
echo "[INFO] branch: $HARNESS_CI_BRANCH"
echo "[INFO] pipeline: $HARNESS_CI_PIPELINE"

exec bash "$ROOT_DIR/scripts/bootstrap_harness_ci.sh"
