#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
IMAGE_LIST_FILE="${CHATOS_CI_IMAGE_LIST_FILE:-$ROOT_DIR/.chatos-ci-images}"
TRIVY_DB_REPOSITORY="${TRIVY_DB_REPOSITORY:-ghcr.io/aquasecurity/trivy-db:2}"
TRIVY_TIMEOUT="${TRIVY_TIMEOUT:-20m}"

if [ ! -s "$IMAGE_LIST_FILE" ]; then
  echo "[ERROR] Harness CI image list is missing or empty: $IMAGE_LIST_FILE" >&2
  exit 2
fi

while IFS= read -r image; do
  [ -n "$image" ] || continue
  echo "[INFO] scanning image: $image"
  trivy image \
    --db-repository "$TRIVY_DB_REPOSITORY" \
    --timeout "$TRIVY_TIMEOUT" \
    --scanners vuln \
    --severity HIGH,CRITICAL \
    --ignore-unfixed \
    --exit-code 1 \
    --no-progress \
    "$image"
done <"$IMAGE_LIST_FILE"
