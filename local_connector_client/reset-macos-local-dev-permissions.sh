#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This helper is only for macOS." >&2
  exit 1
fi

IDENTIFIERS=(
  "com.chatos.local-connector"
  "com.chatos.local-connector.core"
  "com.chatos.local-connector.computer-use-helper"
)
SERVICES=(
  "Accessibility"
  "ScreenCapture"
)

for service in "${SERVICES[@]}"; do
  for identifier in "${IDENTIFIERS[@]}"; do
    echo "[INFO] Resetting macOS TCC $service for $identifier"
    /usr/bin/tccutil reset "$service" "$identifier" >/dev/null 2>&1 || true
  done
done

echo "[OK] Local development macOS Accessibility/Screen Recording permission records were reset."
echo "[OK] Reopen Chat OS Local Connector, then use its permission buttons to grant access again."
