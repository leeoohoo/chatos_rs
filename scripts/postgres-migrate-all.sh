#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifests=(
  "config_center_service/backend/Cargo.toml"
  "user_service/backend/Cargo.toml"
  "plugin_management_service/backend/Cargo.toml"
  "local_connector_service/backend/Cargo.toml"
  "task_runner_service/backend/Cargo.toml"
  "mcp_management_service/backend/Cargo.toml"
  "memory_engine/backend/Cargo.toml"
  "chatos/backend/Cargo.toml"
)

for manifest in "${manifests[@]}"; do
  echo "Running migrations for ${manifest%/Cargo.toml}"
  cargo run --quiet --manifest-path "$root_dir/$manifest" --bin migrate
done

"$root_dir/scripts/postgres-finalize-privileges.sh"
