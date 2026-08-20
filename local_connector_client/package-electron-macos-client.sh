#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

CLIENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$CLIENT_DIR/.." && pwd)"
FRONTEND_DIR="$CLIENT_DIR/frontend"
STAGING_DIR="$CLIENT_DIR/.package/macos"
DIST_DIR="$CLIENT_DIR/dist/electron-macos"
BUILDER_CONFIG="$CLIENT_DIR/electron-builder-macos.yml"
SKILL_CATALOG="$CLIENT_DIR/skill_bundles/catalog/internal-skill-catalog.json"
PLUGIN_CATALOG="$CLIENT_DIR/plugin_bundles/catalog/bundled-plugin-catalog.json"
PLUGIN_BUNDLE_TOOL="$CLIENT_DIR/prepare-plugin-bundles.mjs"
INSTALLED_PACKAGE_VERIFIER="$CLIENT_DIR/verify-installed-package.mjs"
APP_ICON_SOURCE="${CHATOS_APP_ICON_SOURCE:-$ROOT_DIR/official_website_service/frontend/public/brand/okra-logo-mark.png}"
PACKAGE_TARGET_OWNED=0
if [[ -n "${CHATOS_CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="$CHATOS_CARGO_TARGET_DIR"
elif [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="/tmp/chatos-local-connector-package-target-$$"
  PACKAGE_TARGET_OWNED=1
fi

cleanup_package_target() {
  if [[ "$PACKAGE_TARGET_OWNED" != "1" ]]; then
    return
  fi
  case "$CARGO_TARGET_DIR" in
    /tmp/chatos-local-connector-package-target-*) ;;
    *)
      echo "[WARN] Refusing to clean unexpected Cargo target path: $CARGO_TARGET_DIR" >&2
      return
      ;;
  esac
  if [[ -e "$CARGO_TARGET_DIR" ]]; then
    echo "[INFO] Cleaning temporary Cargo target:"
    /usr/bin/du -sh "$CARGO_TARGET_DIR" || true
    /usr/bin/find "$CARGO_TARGET_DIR" -depth -delete || true
  fi
  /bin/df -h /tmp || true
}
trap cleanup_package_target EXIT

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

if [[ "${CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV:-0}" == "1" && "${CHATOS_MAC_SIGN:-0}" == "1" ]]; then
  echo "CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV=1 is only for unsigned local development packages; do not combine it with CHATOS_MAC_SIGN=1." >&2
  exit 1
fi

verify_local_dev_codesign_identifier() {
  local executable_path="${1:?executable path is required}"
  local expected_identifier="${2:?expected identifier is required}"
  local details
  details="$(/usr/bin/codesign -d --verbose=4 "$executable_path" 2>&1)"
  if [[ "$details" != *"Identifier=$expected_identifier"* ]]; then
    echo "Local development code signature identifier mismatch for $executable_path" >&2
    echo "Expected: $expected_identifier" >&2
    echo "$details" >&2
    exit 1
  fi
}

verify_local_dev_app_codesign_identifier() {
  local app_path="${1:?app path is required}"
  local expected_identifier="${2:?expected identifier is required}"
  local details
  /usr/bin/codesign --verify --deep --strict "$app_path"
  details="$(/usr/bin/codesign -d --verbose=4 "$app_path" 2>&1)"
  if [[ "$details" != *"Identifier=$expected_identifier"* ]]; then
    echo "Local development app code signature identifier mismatch for $app_path" >&2
    echo "Expected: $expected_identifier" >&2
    echo "$details" >&2
    exit 1
  fi
}

if [[ -e "$DIST_DIR" ]]; then
  echo "[INFO] Removing stale macOS package output: $DIST_DIR"
  /usr/bin/find "$DIST_DIR" -depth -delete
fi

document_runtime_source_has_tools() {
  local source_root="${1:?source root is required}"
  [[ -d "$source_root" && ! -L "$source_root" ]] || return 1

  local has_libreoffice=0
  if [[ -x "$source_root/libreoffice-headless/libreoffice/LibreOffice.app/Contents/MacOS/soffice" \
    || -x "$source_root/libreoffice-headless/libreoffice/LibreOfficeDev.app/Contents/MacOS/soffice" \
    || -x "$source_root/libreoffice/LibreOffice.app/Contents/MacOS/soffice" \
    || -x "$source_root/libreoffice/LibreOfficeDev.app/Contents/MacOS/soffice" ]]; then
    has_libreoffice=1
  fi

  local has_poppler=0
  if [[ -x "$source_root/poppler/poppler/bin/pdftoppm" \
    || -x "$source_root/poppler/bin/pdftoppm" ]]; then
    has_poppler=1
  fi

  [[ "$has_libreoffice" == "1" && "$has_poppler" == "1" ]]
}

copy_document_runtime_source() {
  local source_root="${1:?source root is required}"
  local destination_root="${2:?destination root is required}"
  case "$destination_root" in
    "$HOME/Library/Caches/chatos-local-connector/document-runtime-source/"*) ;;
    *)
      echo "Refusing to replace unexpected document runtime cache path: $destination_root" >&2
      exit 1
      ;;
  esac

  local temporary_root="$destination_root.partial.$$"
  if [[ -e "$temporary_root" ]]; then
    /usr/bin/find "$temporary_root" -depth -delete
  fi
  if [[ -e "$destination_root" ]]; then
    /usr/bin/find "$destination_root" -depth -delete
  fi

  mkdir -p "$temporary_root"
  if [[ -d "$source_root/libreoffice-headless" ]]; then
    ditto "$source_root/libreoffice-headless" "$temporary_root/libreoffice-headless"
  else
    mkdir -p "$temporary_root/libreoffice"
    ditto "$source_root/libreoffice" "$temporary_root/libreoffice"
  fi
  if [[ -d "$source_root/poppler/poppler" ]]; then
    mkdir -p "$temporary_root/poppler"
    ditto "$source_root/poppler" "$temporary_root/poppler"
  else
    ditto "$source_root/poppler" "$temporary_root/poppler"
  fi

  if ! document_runtime_source_has_tools "$temporary_root"; then
    echo "Imported document runtime source is incomplete: $temporary_root" >&2
    /usr/bin/find "$temporary_root" -depth -delete
    exit 1
  fi
  mv "$temporary_root" "$destination_root"
}

CHATOS_DOCUMENT_RUNTIME_REPO_SOURCE="$CLIENT_DIR/runtime_assets/document-runtime-source/$TOOLS_PLATFORM"
CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE="${CHATOS_DOCUMENT_RUNTIME_SOURCE_CACHE:-$HOME/Library/Caches/chatos-local-connector/document-runtime-source/$TOOLS_PLATFORM}"
if [[ -z "${CHATOS_DOCUMENT_RUNTIME_SOURCE:-}" ]]; then
  if document_runtime_source_has_tools "$CHATOS_DOCUMENT_RUNTIME_REPO_SOURCE"; then
    export CHATOS_DOCUMENT_RUNTIME_SOURCE="$CHATOS_DOCUMENT_RUNTIME_REPO_SOURCE"
    echo "[INFO] Using bundled ChatOS document runtime source: $CHATOS_DOCUMENT_RUNTIME_SOURCE"
  elif document_runtime_source_has_tools "$CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE"; then
    export CHATOS_DOCUMENT_RUNTIME_SOURCE="$CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE"
    echo "[INFO] Using cached ChatOS document runtime source: $CHATOS_DOCUMENT_RUNTIME_SOURCE"
  elif [[ "${CHATOS_USE_CODEX_DOCUMENT_RUNTIME_SOURCE:-0}" == "1" ]]; then
    CODEX_DOCUMENT_RUNTIME_SOURCE="$HOME/.cache/codex-runtimes/codex-primary-runtime/dependencies/native"
    if ! document_runtime_source_has_tools "$CODEX_DOCUMENT_RUNTIME_SOURCE"; then
      cat >&2 <<EOF
CHATOS_USE_CODEX_DOCUMENT_RUNTIME_SOURCE=1 was set, but the Codex document runtime source is not available or incomplete:

  $CODEX_DOCUMENT_RUNTIME_SOURCE

Expected source layout:

  <root>/libreoffice-headless/libreoffice/LibreOffice*.app/Contents/MacOS/soffice
  <root>/poppler/bin/pdftoppm

No new DMG or app was produced. The stale output directory has already been removed:

  $DIST_DIR
EOF
      exit 1
    fi
    echo "[WARN] Importing Codex document runtime source into the ChatOS local cache for temporary local testing only."
    echo "[WARN] Source: $CODEX_DOCUMENT_RUNTIME_SOURCE"
    echo "[WARN] Cache:  $CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE"
    copy_document_runtime_source "$CODEX_DOCUMENT_RUNTIME_SOURCE" "$CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE"
    export CHATOS_DOCUMENT_RUNTIME_SOURCE="$CHATOS_DOCUMENT_RUNTIME_CACHE_SOURCE"
  else
    cat >&2 <<'EOF'
ChatOS document runtime source was not found.

The macOS package needs a ChatOS-owned LibreOffice + Poppler runtime source. The packaging script checks these locations automatically:

  local_connector_client/runtime_assets/document-runtime-source/<platform>
  ~/Library/Caches/chatos-local-connector/document-runtime-source/<platform>

You can still override the source explicitly:

  CHATOS_DOCUMENT_RUNTIME_SOURCE=/path/to/document-runtime-source ./package-electron-macos-client.sh

Expected source layout:

  <root>/libreoffice-headless/libreoffice/LibreOffice*.app/Contents/MacOS/soffice
  <root>/poppler/bin/pdftoppm

For a temporary local-only verification build on this Mac, you may explicitly import the Codex-bundled runtime source into the ChatOS cache:

  CHATOS_USE_CODEX_DOCUMENT_RUNTIME_SOURCE=1 ./package-electron-macos-client.sh

Do not use the Codex import option for official builds.
EOF
    cat >&2 <<EOF

No new DMG or app was produced. The stale output directory has already been removed:

  $DIST_DIR
EOF
    exit 1
  fi
fi

node -e '
const fs = require("fs");
const path = require("path");
const catalogPath = process.argv[1];
const clientDir = process.argv[2];
const catalog = JSON.parse(fs.readFileSync(catalogPath, "utf8"));
if (catalog.schema_version !== 1 || !Array.isArray(catalog.skills) || catalog.skills.length !== 28) {
  throw new Error("Local Connector internal Skill catalog must contain exactly 28 schema-v1 entries");
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
CHROME_NATIVE_HOST_BIN="$TARGET_DIR/release/chatos_chrome_native_host"
COMPUTER_USE_HELPER_BIN="$TARGET_DIR/release/chatos_computer_use_helper"
TOOLS_DIR="$ROOT_DIR/bundled-tools/$TOOLS_PLATFORM"

if [[ ! -x "$CORE_BIN" ]]; then
  echo "Local Connector Core was not built: $CORE_BIN" >&2
  exit 1
fi

if [[ ! -x "$CHROME_NATIVE_HOST_BIN" ]]; then
  echo "ChatOS Chrome Native Host was not built: $CHROME_NATIVE_HOST_BIN" >&2
  exit 1
fi

if [[ ! -x "$COMPUTER_USE_HELPER_BIN" ]]; then
  echo "ChatOS Computer Use helper was not built: $COMPUTER_USE_HELPER_BIN" >&2
  exit 1
fi

if [[ ! -d "$TOOLS_DIR" ]]; then
  echo "Bundled tools directory is missing: $TOOLS_DIR" >&2
  exit 1
fi

if [[ -e "$STAGING_DIR" ]]; then
  /usr/bin/find "$STAGING_DIR" -depth -delete
fi
mkdir -p \
  "$STAGING_DIR/bundled-tools" \
  "$STAGING_DIR/chrome-extension" \
  "$STAGING_DIR/plugin-bundles" \
  "$STAGING_DIR/skill-bundles" \
  "$STAGING_DIR/sqlite-migrations"
cp "$CORE_BIN" "$STAGING_DIR/local_connector_client_core"
cp "$CHROME_NATIVE_HOST_BIN" "$STAGING_DIR/chatos_chrome_native_host"
cp "$COMPUTER_USE_HELPER_BIN" "$STAGING_DIR/chatos_computer_use_helper"
cp -R "$CLIENT_DIR/chrome_extension/." "$STAGING_DIR/chrome-extension/"
cp -R "$TOOLS_DIR" "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM"
cp -R "$CLIENT_DIR/skill_bundles/." "$STAGING_DIR/skill-bundles/"
node "$PLUGIN_BUNDLE_TOOL" \
  --plugin-catalog "$PLUGIN_CATALOG" \
  --skill-catalog "$SKILL_CATALOG" \
  --skill-root "$CLIENT_DIR/skill_bundles/internal" \
  --output "$STAGING_DIR/plugin-bundles" \
  --platform "$TOOLS_PLATFORM"
node "$PLUGIN_BUNDLE_TOOL" \
  --verify-only \
  --plugin-catalog "$PLUGIN_CATALOG" \
  --skill-catalog "$SKILL_CATALOG" \
  --skill-root "$CLIENT_DIR/skill_bundles/internal" \
  --output "$STAGING_DIR/plugin-bundles" \
  --platform "$TOOLS_PLATFORM"
cp -R "$CLIENT_DIR/core/migrations/." "$STAGING_DIR/sqlite-migrations/"
if [[ "${CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV:-0}" == "1" ]]; then
  cat > "$STAGING_DIR/computer-use-unsigned-local-dev.json" <<'JSON'
{
  "allowUnsignedComputerUseLocalDev": true,
  "warning": "Local development only. Do not ship this marker in signed or production packages."
}
JSON
else
  cat > "$STAGING_DIR/computer-use-unsigned-local-dev.json" <<'JSON'
{
  "allowUnsignedComputerUseLocalDev": false
}
JSON
fi
chmod +x \
  "$STAGING_DIR/local_connector_client_core" \
  "$STAGING_DIR/chatos_chrome_native_host" \
  "$STAGING_DIR/chatos_computer_use_helper"
bash "$CLIENT_DIR/prepare-app-icon-macos.sh" \
  "$APP_ICON_SOURCE" \
  "$STAGING_DIR/ChatOS.icns"
"$CLIENT_DIR/prepare-browser-runtime-macos.sh" \
  "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM" \
  "$TOOLS_PLATFORM"
"$CLIENT_DIR/prepare-document-runtime-macos.sh" \
  "$STAGING_DIR/bundled-tools/$TOOLS_PLATFORM/documents-runtime" \
  "$TOOLS_PLATFORM"

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
      if [[ -e "$ELECTRON_DIST_DIR" ]]; then
        /usr/bin/find "$ELECTRON_DIST_DIR" -depth -delete
      fi
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
if [[ "$ELECTRON_ARCH" == "arm64" ]]; then
  APP_PATH="$CLIENT_DIR/dist/electron-macos/mac-arm64/Chat OS Local Connector.app"
else
  APP_PATH="$CLIENT_DIR/dist/electron-macos/mac/Chat OS Local Connector.app"
fi
RESOURCES_PATH="$APP_PATH/Contents/Resources"
VERIFICATION_REPORT="$DMG_PATH.verification.json"

if [[ ! -f "$DMG_PATH" ]]; then
  echo "DMG output was not created: $DMG_PATH" >&2
  exit 1
fi
if [[ ! -d "$RESOURCES_PATH" || -L "$RESOURCES_PATH" ]]; then
  echo "Packaged macOS app Resources were not created: $RESOURCES_PATH" >&2
  exit 1
fi
if [[ "${CHATOS_MAC_SIGN:-0}" != "1" && "${CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV:-0}" == "1" ]]; then
  verify_local_dev_app_codesign_identifier \
    "$APP_PATH" \
    "com.chatos.local-connector"
  verify_local_dev_codesign_identifier \
    "$RESOURCES_PATH/local_connector_client_core" \
    "com.chatos.local-connector.core"
  verify_local_dev_codesign_identifier \
    "$RESOURCES_PATH/chatos_computer_use_helper" \
    "com.chatos.local-connector.computer-use-helper"
fi

VERIFY_ARGS=(
  --platform
  "$TOOLS_PLATFORM"
  --resources
  "$RESOURCES_PATH"
  --plugin-catalog
  "$PLUGIN_CATALOG"
  --skill-catalog
  "$SKILL_CATALOG"
  --electron-runtime-source
  "$FRONTEND_DIR/electron/core-runtime.cjs"
  --chrome-extension-source
  "$CLIENT_DIR/chrome_extension"
  --report
  "$VERIFICATION_REPORT"
)
if [[ "${CHATOS_MAC_SIGN:-0}" == "1" ]]; then
  VERIFY_ARGS+=(--require-signed)
fi
node "$INSTALLED_PACKAGE_VERIFIER" "${VERIFY_ARGS[@]}" >/dev/null

hdiutil verify "$DMG_PATH" >/dev/null
echo "[OK] macOS desktop installer: $DMG_PATH"
echo "[OK] Installed-package verification: $VERIFICATION_REPORT"
echo "[OK] SHA-256: $(shasum -a 256 "$DMG_PATH" | awk '{print $1}')"

if [[ "${CHATOS_MAC_SIGN:-0}" != "1" ]]; then
  echo "[INFO] Package is unsigned. Set CHATOS_MAC_SIGN=1 after installing a valid Developer ID Application certificate."
  if [[ "${CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV:-0}" == "1" ]]; then
    echo "[WARN] Unsigned local-development Computer Use is enabled for this package."
    echo "[INFO] If macOS still shows stale Accessibility or Screen Recording state from an older local build, run:"
    echo "       $CLIENT_DIR/reset-macos-local-dev-permissions.sh"
  fi
fi
