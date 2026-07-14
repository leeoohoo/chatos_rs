#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
IMAGE_LIST_FILE="${CHATOS_CI_IMAGE_LIST_FILE:-$ROOT_DIR/.chatos-ci-images}"
if [ -n "${TRIVY_DB_REPOSITORY:-}" ]; then
  TRIVY_DB_REPOSITORIES="$TRIVY_DB_REPOSITORY"
else
  TRIVY_DB_REPOSITORIES="${TRIVY_DB_REPOSITORIES:-public.ecr.aws/aquasecurity/trivy-db:2 ghcr.io/aquasecurity/trivy-db:2}"
fi
if [ -n "${TRIVY_JAVA_DB_REPOSITORY:-}" ]; then
  TRIVY_JAVA_DB_REPOSITORIES="$TRIVY_JAVA_DB_REPOSITORY"
else
  TRIVY_JAVA_DB_REPOSITORIES="${TRIVY_JAVA_DB_REPOSITORIES:-docker.m.daocloud.io/aquasec/trivy-java-db:1 public.ecr.aws/aquasecurity/trivy-java-db:1 ghcr.io/aquasecurity/trivy-java-db:1}"
fi
TRIVY_TIMEOUT="${TRIVY_TIMEOUT:-20m}"
TRIVY_JAVA_DB_TIMEOUT="${TRIVY_JAVA_DB_TIMEOUT:-20m}"

if [ ! -s "$IMAGE_LIST_FILE" ]; then
  echo "[ERROR] Harness CI image list is missing or empty: $IMAGE_LIST_FILE" >&2
  exit 2
fi

java_db_downloaded=false
for repository in $TRIVY_JAVA_DB_REPOSITORIES; do
  echo "[INFO] downloading Java DB from: $repository"
  if trivy image \
    --download-java-db-only \
    --java-db-repository "$repository" \
    --timeout "$TRIVY_JAVA_DB_TIMEOUT" \
    --no-progress; then
    java_db_downloaded=true
    break
  fi
  echo "[WARN] Java DB download failed, trying the next repository: $repository" >&2
  trivy clean --java-db >/dev/null 2>&1 || true
done

if [ "$java_db_downloaded" != true ]; then
  echo "[ERROR] unable to download the Trivy Java DB from any configured repository" >&2
  exit 3
fi

set --
for repository in $TRIVY_DB_REPOSITORIES; do
  set -- "$@" --db-repository "$repository"
done

while IFS= read -r image; do
  [ -n "$image" ] || continue
  echo "[INFO] scanning image: $image"
  trivy image \
    "$@" \
    --timeout "$TRIVY_TIMEOUT" \
    --scanners vuln \
    --skip-java-db-update \
    --severity HIGH,CRITICAL \
    --ignore-unfixed \
    --exit-code 1 \
    --no-progress \
    "$image"
done <"$IMAGE_LIST_FILE"
