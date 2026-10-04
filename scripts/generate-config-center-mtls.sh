#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/generate-service-mtls.sh"

generate_service_mtls "${1:-}" \
  "Configuration Center" \
  "ChatOS Configuration Center Internal CA" \
  "configuration-center-backend" \
  "DNS:configuration-center-backend,DNS:configuration-center,DNS:localhost,IP:127.0.0.1" \
  local-connector-service memory-engine official-website plugin-management-service user-service
