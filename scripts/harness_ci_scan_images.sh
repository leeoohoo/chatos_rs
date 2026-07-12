#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
IMAGE_LIST_FILE="${CHATOS_CI_IMAGE_LIST_FILE:-$ROOT_DIR/.chatos-ci-images}"

if [ ! -s "$IMAGE_LIST_FILE" ]; then
  echo "[ERROR] Harness CI image list is missing or empty: $IMAGE_LIST_FILE" >&2
  exit 2
fi

while IFS= read -r image; do
  [ -n "$image" ] || continue
  echo "[INFO] scanning image: $image"
  trivy image \
    --scanners vuln \
    --severity HIGH,CRITICAL \
    --ignore-unfixed \
    --exit-code 1 \
    --no-progress \
    "$image"
done <"$IMAGE_LIST_FILE"
