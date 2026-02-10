#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="${ROOT_DIR}/target/release/sub-agent-router-mcp-server-rs"

if [[ ! -x "${BIN_PATH}" ]]; then
  echo "[run-admin] building release binary..."
  (cd "${ROOT_DIR}" && cargo build --release)
fi

ADMIN_PORT="${ADMIN_PORT:-7001}"
ADMIN_HOST="${ADMIN_HOST:-127.0.0.1}"

exec "${BIN_PATH}" --admin-port "${ADMIN_PORT}" --admin-host "${ADMIN_HOST}"
