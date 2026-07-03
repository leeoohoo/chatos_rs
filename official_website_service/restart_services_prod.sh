#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BASE_SCRIPT="$SCRIPT_DIR/restart_services.sh"

if [[ ! -f "$BASE_SCRIPT" ]]; then
  echo "[ERROR] missing base startup script: $BASE_SCRIPT"
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:official-website:prod" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR:official-website:prod" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="officialwebsiteprod"
fi

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if command -v rustup >/dev/null 2>&1; then
  ACTIVE_TOOLCHAIN="$(cd "$ROOT_DIR" && rustup show active-toolchain 2>/dev/null | awk 'NR==1 {print $1}')"
  if [[ -n "$ACTIVE_TOOLCHAIN" ]]; then
    export RUSTUP_TOOLCHAIN="$ACTIVE_TOOLCHAIN"
  fi
fi

export OFFICIAL_WEBSITE_MODE="${OFFICIAL_WEBSITE_MODE:-prod}"
export OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-49250}"
export OFFICIAL_WEBSITE_FRONTEND_PORT="${OFFICIAL_WEBSITE_FRONTEND_PORT:-49251}"
export OFFICIAL_WEBSITE_RUNTIME_DIR="${OFFICIAL_WEBSITE_RUNTIME_DIR:-/tmp/chatos_rs_official_website_prod_${ROOT_HASH}}"
export OFFICIAL_WEBSITE_BACKEND_SESSION="${OFFICIAL_WEBSITE_BACKEND_SESSION:-chatos_official_website_prod_backend}"
export OFFICIAL_WEBSITE_FRONTEND_SESSION="${OFFICIAL_WEBSITE_FRONTEND_SESSION:-chatos_official_website_prod_frontend}"

exec "$BASE_SCRIPT" "$@"
