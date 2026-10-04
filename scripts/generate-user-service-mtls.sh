#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/generate-service-mtls.sh"

generate_service_mtls "${1:-}" \
  "User Service" \
  "ChatOS User Service Internal CA" \
  "user-service-backend" \
  "DNS:user-service-backend,DNS:user-service,DNS:localhost,IP:127.0.0.1" \
  memory-engine
