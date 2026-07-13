#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$CLIENT_DIR/.." && pwd)"
FRONTEND_DIR="$CLIENT_DIR/frontend"
STAGING_DIR="$CLIENT_DIR/.package/macos"
BUILDER_CONFIG="$CLIENT_DIR/electron-builder-macos.yml"

case "$(uname -m)" in
  arm64|aarch64)
    ELECTRON_ARCH="arm64"
    TOOLS_PLATFORM="macos-arm64"
    ;;
  x86_64|amd64)
    ELECTRON_ARCH="x64"
    TOOLS_PLATFORM="macos-x64"
    ;;
  *)
    echo "Unsupported macOS architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

for command_name in cargo node npm hdiutil shasum; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command not found: $command_name" >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must run on macOS." >&2
  exit 1
fi

if [[ "${CHATOS_SKIP_NPM_CI:-0}" != "1" ]]; then
  (
    cd "$FRONTEND_DIR"
    npm ci
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
  cargo build --release -p local_connector_client_core
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

if [[ ! -x "$CORE_BIN" ]]; then
  echo "Local Connector Core was not built: $CORE_BIN" >&2
  exit 1
fi

if [[ ! -d "$TOOLS_DIR" ]]; then
  echo "Bundled tools directory is missing: $TOOLS_DIR" >&2
  exit 1
fi

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/bundled-tools"
cp "$CORE_BIN" "$STAGING_DIR/local_connector_client_core"
cp -R "$TOOLS_DIR" "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM"
chmod +x "$STAGING_DIR/local_connector_client_core"

BUILD_ARGS=(
  --mac
  dmg
  "--$ELECTRON_ARCH"
  --config
  "$BUILDER_CONFIG"
)

(
  cd "$FRONTEND_DIR"
  if [[ "${CHATOS_MAC_SIGN:-0}" == "1" ]]; then
    ./node_modules/.bin/electron-builder "${BUILD_ARGS[@]}"
  else
    CSC_IDENTITY_AUTO_DISCOVERY=false \
      ./node_modules/.bin/electron-builder "${BUILD_ARGS[@]}"
  fi
)

VERSION="$(node -p "require('$FRONTEND_DIR/package.json').version")"
DMG_PATH="$CLIENT_DIR/dist/electron-macos/Chat-OS-Local-Connector-$VERSION-$ELECTRON_ARCH.dmg"

if [[ ! -f "$DMG_PATH" ]]; then
  echo "DMG output was not created: $DMG_PATH" >&2
  exit 1
fi

hdiutil verify "$DMG_PATH" >/dev/null
echo "[OK] macOS desktop installer: $DMG_PATH"
echo "[OK] SHA-256: $(shasum -a 256 "$DMG_PATH" | awk '{print $1}')"

if [[ "${CHATOS_MAC_SIGN:-0}" != "1" ]]; then
  echo "[INFO] Package is unsigned. Set CHATOS_MAC_SIGN=1 after installing a valid Developer ID Application certificate."
fi
