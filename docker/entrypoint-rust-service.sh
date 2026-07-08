#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

mkdir -p /data /workspace /tmp/chatos

if [[ $# -gt 0 ]]; then
  exec "$@"
fi

exec /usr/local/bin/chatos-service
