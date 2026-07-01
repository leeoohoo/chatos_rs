#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="$ROOT_DIR/.github/api-surface-baseline.txt"
GEN_SCRIPT="$ROOT_DIR/scripts/generate_api_surface_snapshot.sh"

bash "$GEN_SCRIPT" > "$BASELINE_FILE"
echo "[OK] Updated API surface baseline: $BASELINE_FILE"
