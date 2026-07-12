#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIPELINE_DIR="${HARNESS_CI_IMAGE_PIPELINE_DIR:-.harness/pipelines/images}"
OUT_DIR="$ROOT_DIR/$PIPELINE_DIR"

write_pipeline() {
  local service="$1"
  local identifier="image-$service"
  local file="$OUT_DIR/$identifier.yml"
  local deploy_command

  if [[ "$service" == "sandbox-agent-image" ]]; then
    deploy_command='bash docker/deploy-harness-ci.sh check-images "$CHATOS_CI_IMAGE_SERVICES"'
  else
    deploy_command='bash docker/deploy-harness-ci.sh "$CHATOS_CI_IMAGE_SERVICES"'
  fi

  cat >"$file" <<EOF
---
kind: pipeline
type: docker
name: $identifier

clone:
  git:
    image: drone/git:local
    pull: if-not-exists

steps:
  - name: build-and-deploy
    image: docker:27-cli
    environment:
      CHATOS_CI_IMAGE_SERVICES: "$service"
      CHATOS_DOCKER_BUILD_PARALLEL_LIMIT: "1"
      CHATOS_DOCKER_MODE: build
      CHATOS_DOCKER_PRUNE_DANGLING_IMAGES: "true"
      CHATOS_IMAGE_NAMESPACE: ghcr.io/leeoohoo
      CHATOS_IMAGE_TAG: harness-ci
    commands:
      - sed -i 's|https://dl-cdn.alpinelinux.org|https://mirrors.aliyun.com|g; s|http://dl-cdn.alpinelinux.org|http://mirrors.aliyun.com|g' /etc/apk/repositories
      - apk add --no-cache bash docker-cli-compose
      - docker info
      - docker compose version
      - bash scripts/harness_ci_build_images.sh
      - $deploy_command
    volumes:
      - name: docker-sock
        path: /var/run/docker.sock

volumes:
  - name: docker-sock
    host:
      path: /var/run/docker.sock
EOF
}

main() {
  cd "$ROOT_DIR"
  mkdir -p "$OUT_DIR"

  local service
  if [[ $# -gt 0 ]]; then
    for service in "$@"; do
      write_pipeline "$service"
    done
  else
    while IFS= read -r service; do
      if [[ -n "$service" ]]; then
        write_pipeline "$service"
      fi
    done < <(bash docker/deploy.sh build-services)
  fi

  echo "[OK] generated Harness image pipelines in $PIPELINE_DIR"
}

main "$@"
