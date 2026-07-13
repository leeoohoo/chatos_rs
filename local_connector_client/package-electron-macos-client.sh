#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$CLIENT_DIR/.." && pwd)"
FRONTEND_DIR="$CLIENT_DIR/frontend"
STAGING_DIR="$CLIENT_DIR/.package/macos"
BUILDER_CONFIG="$CLIENT_DIR/electron-builder-macos.yml"
SKILL_CATALOG="$CLIENT_DIR/skill_bundles/catalog/internal-skill-catalog.json"

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

for command_name in cargo node npm ditto hdiutil shasum; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command not found: $command_name" >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must run on macOS." >&2
  exit 1
fi

node -e '
const fs = require("fs");
const path = require("path");
const catalogPath = process.argv[1];
const clientDir = process.argv[2];
const catalog = JSON.parse(fs.readFileSync(catalogPath, "utf8"));
if (catalog.schema_version !== 1 || !Array.isArray(catalog.skills) || catalog.skills.length !== 27) {
  throw new Error("Local Connector internal Skill catalog must contain exactly 27 schema-v1 entries");
}
for (const skill of catalog.skills) {
  const bundleDir = path.join("skill_bundles", "internal", skill.name, skill.version);
  for (const fileName of ["skill.json", "instructions.md"]) {
    const relativePath = path.join(bundleDir, fileName);
    if (!fs.existsSync(path.join(clientDir, relativePath))) {
      throw new Error(`Missing internal Skill bundle resource: ${relativePath}`);
    }
  }
}
' "$SKILL_CATALOG" "$CLIENT_DIR"

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
mkdir -p "$STAGING_DIR/bundled-tools" "$STAGING_DIR/skill-bundles"
cp "$CORE_BIN" "$STAGING_DIR/local_connector_client_core"
cp -R "$TOOLS_DIR" "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM"
cp -R "$CLIENT_DIR/skill_bundles/." "$STAGING_DIR/skill-bundles/"
chmod +x "$STAGING_DIR/local_connector_client_core"

ELECTRON_VERSION="$(node -p "require('$FRONTEND_DIR/node_modules/electron/package.json').version")"
ELECTRON_DIST_DIR="$STAGING_DIR/electron-dist"
ELECTRON_DIST_SOURCE=""

if [[ -n "${CHATOS_ELECTRON_DIST:-}" ]]; then
  if [[ ! -d "$CHATOS_ELECTRON_DIST/Electron.app" ]]; then
    echo "CHATOS_ELECTRON_DIST must contain Electron.app: $CHATOS_ELECTRON_DIST" >&2
    exit 1
  fi
  ELECTRON_DIST_SOURCE="$CHATOS_ELECTRON_DIST"
elif [[ -d "$FRONTEND_DIR/node_modules/electron/dist/Electron.app" ]]; then
  ELECTRON_DIST_SOURCE="$FRONTEND_DIR/node_modules/electron/dist"
else
  ELECTRON_ARCHIVE_NAME="electron-v$ELECTRON_VERSION-darwin-$ELECTRON_ARCH.zip"
  ELECTRON_CACHE_ROOTS=()
  if [[ -n "${ELECTRON_CACHE:-}" ]]; then
    ELECTRON_CACHE_ROOTS+=("$ELECTRON_CACHE")
  fi
  ELECTRON_CACHE_ROOTS+=("$HOME/Library/Caches/electron")

  for cache_root in "${ELECTRON_CACHE_ROOTS[@]}"; do
    [[ -d "$cache_root" ]] || continue
    while IFS= read -r -d '' cached_archive; do
      rm -rf "$ELECTRON_DIST_DIR"
      mkdir -p "$ELECTRON_DIST_DIR"
      if ditto -x -k "$cached_archive" "$ELECTRON_DIST_DIR" \
        && [[ -d "$ELECTRON_DIST_DIR/Electron.app" ]]; then
        ELECTRON_DIST_SOURCE="$ELECTRON_DIST_DIR"
        echo "[INFO] Reusing cached Electron $ELECTRON_VERSION: $cached_archive"
        break 2
      fi
    done < <(find "$cache_root" -type f -name "$ELECTRON_ARCHIVE_NAME" -print0 2>/dev/null)
  done
fi

BUILD_ARGS=(
  --mac
  dmg
  "--$ELECTRON_ARCH"
  --config
  "$BUILDER_CONFIG"
)

if [[ -n "$ELECTRON_DIST_SOURCE" ]]; then
  BUILD_ARGS+=("--config.electronDist=$ELECTRON_DIST_SOURCE")
else
  echo "[INFO] No local Electron $ELECTRON_VERSION cache was found; electron-builder will download it."
  echo "[INFO] If downloading is unavailable, set CHATOS_ELECTRON_DIST to a directory containing Electron.app."
fi

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
