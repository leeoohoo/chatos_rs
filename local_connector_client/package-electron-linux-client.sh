#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$CLIENT_DIR/.." && pwd)"
FRONTEND_DIR="$CLIENT_DIR/frontend"
STAGING_DIR="$CLIENT_DIR/.package/linux"
DIST_DIR="$CLIENT_DIR/dist/electron-linux"
BUILDER_CONFIG="$CLIENT_DIR/electron-builder-linux.yml"
INSTALLED_PACKAGE_VERIFIER="$CLIENT_DIR/verify-installed-package.mjs"
PACKAGE_TARGET_OWNED=0

if [[ -n "${CHATOS_CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="$CHATOS_CARGO_TARGET_DIR"
elif [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="/tmp/chatos-local-connector-linux-target-$$"
  PACKAGE_TARGET_OWNED=1
fi

cleanup_package_target() {
  if [[ "$PACKAGE_TARGET_OWNED" != "1" ]]; then
    return
  fi
  case "$CARGO_TARGET_DIR" in
    /tmp/chatos-local-connector-linux-target-*) ;;
    *)
      echo "[WARN] Refusing to clean unexpected Cargo target path: $CARGO_TARGET_DIR" >&2
      return
      ;;
  esac
  if [[ -e "$CARGO_TARGET_DIR" ]]; then
    find "$CARGO_TARGET_DIR" -depth -delete
  fi
}
trap cleanup_package_target EXIT

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This script must run on Linux." >&2
  exit 1
fi

case "$(uname -m)" in
  aarch64|arm64)
    ELECTRON_ARCH="arm64"
    TOOLS_PLATFORM="linux-arm64"
    UNPACKED_DIR_NAME="linux-arm64-unpacked"
    ;;
  x86_64|amd64)
    ELECTRON_ARCH="x64"
    TOOLS_PLATFORM="linux-x64"
    UNPACKED_DIR_NAME="linux-unpacked"
    ;;
  *)
    echo "Unsupported Linux architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

for command_name in cargo node npm sha256sum; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command not found: $command_name" >&2
    exit 1
  fi
done

if [[ -e "$DIST_DIR" ]]; then
  find "$DIST_DIR" -depth -delete
fi
if [[ -e "$STAGING_DIR" ]]; then
  find "$STAGING_DIR" -depth -delete
fi

if [[ "${CHATOS_SKIP_NPM_CI:-0}" != "1" ]]; then
  (
    cd "$FRONTEND_DIR"
    ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm ci
  )
fi

if [[ ! -x "$FRONTEND_DIR/node_modules/.bin/electron-builder" ]]; then
  echo "electron-builder is missing. Run without CHATOS_SKIP_NPM_CI=1 first." >&2
  exit 1
fi

(
  cd "$FRONTEND_DIR"
  npm run build:electron
)

(
  cd "$ROOT_DIR"
  cargo build --release \
    -p local_connector_client_core \
    --bin local_connector_client_core
)

TARGET_DIR="$({
  cd "$ROOT_DIR"
  cargo metadata --no-deps --format-version 1
} | node -e '
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => input += chunk);
process.stdin.on("end", () => process.stdout.write(JSON.parse(input).target_directory));
')"
CORE_BIN="$TARGET_DIR/release/local_connector_client_core"
TOOLS_DIR="$ROOT_DIR/bundled-tools/$TOOLS_PLATFORM"

for executable_path in "$CORE_BIN"; do
  if [[ ! -x "$executable_path" ]]; then
    echo "Required Linux executable was not built: $executable_path" >&2
    exit 1
  fi
done
if [[ ! -d "$TOOLS_DIR" ]]; then
  echo "Bundled tools directory is missing: $TOOLS_DIR" >&2
  exit 1
fi

mkdir -p \
  "$STAGING_DIR/bundled-tools" \
  "$STAGING_DIR/sqlite-migrations"
cp "$CORE_BIN" "$STAGING_DIR/local_connector_client_core"
cp -R "$TOOLS_DIR" "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM"
cp -R "$CLIENT_DIR/core/migrations/." "$STAGING_DIR/sqlite-migrations/"
chmod 755 "$STAGING_DIR/local_connector_client_core"

(
  cd "$FRONTEND_DIR"
  ./node_modules/.bin/electron-builder \
    --linux deb \
    "--$ELECTRON_ARCH" \
    --config "$BUILDER_CONFIG"
)

VERSION="$(node -p "require('$FRONTEND_DIR/package.json').version")"
DEB_PATH="$DIST_DIR/Chat-OS-Local-Connector-$VERSION-$ELECTRON_ARCH.deb"
RESOURCES_PATH="$DIST_DIR/$UNPACKED_DIR_NAME/resources"
VERIFICATION_REPORT="$DEB_PATH.verification.json"

if [[ ! -f "$DEB_PATH" ]]; then
  echo "Linux DEB output was not created: $DEB_PATH" >&2
  exit 1
fi
if [[ ! -d "$RESOURCES_PATH" || -L "$RESOURCES_PATH" ]]; then
  echo "Packaged Linux resources were not created: $RESOURCES_PATH" >&2
  exit 1
fi

node "$INSTALLED_PACKAGE_VERIFIER" \
  --platform "$TOOLS_PLATFORM" \
  --runtime-profile linux-core \
  --resources "$RESOURCES_PATH" \
  --electron-runtime-source "$FRONTEND_DIR/electron/core-runtime.cjs" \
  --report "$VERIFICATION_REPORT" \
  >/dev/null

echo "[OK] Linux desktop installer: $DEB_PATH"
echo "[OK] Installed-package verification: $VERIFICATION_REPORT"
echo "[OK] SHA-256: $(sha256sum "$DEB_PATH" | awk '{print $1}')"
echo "[INFO] Runtime profile: linux-core (browser capabilities are installed separately as Marketplace MCP plugins)."
