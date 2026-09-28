#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
INFO_PLIST="$PROJECT_DIR/Support/ChatOSSwift-Info.plist"
ARCH=$(uname -m)

if [[ "$ARCH" != "arm64" ]]; then
  echo "The bundled ripgrep tool currently supports Apple Silicon only; refusing to label an $ARCH build as distributable." >&2
  exit 2
fi

VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INFO_PLIST")
OUTPUT_DIR="$PROJECT_DIR/BundleArtifacts"
DMG_PATH="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.dmg"
CHECKSUM_PATH="$DMG_PATH.sha256"
STAGING_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chatos-macos-release.XXXXXX")

cleanup() {
  rm -rf -- "$STAGING_DIR"
}
trap cleanup EXIT

APP_PATH=$("$SCRIPT_DIR/package-release-app.sh" | tail -n 1)
codesign --verify --deep --strict "$APP_PATH"

mkdir -p "$OUTPUT_DIR"
cp -R "$APP_PATH" "$STAGING_DIR/ChatOS.app"
ln -s /Applications "$STAGING_DIR/Applications"
cat > "$STAGING_DIR/首次运行说明.txt" <<'EOF'
ChatOS 尚未使用 Apple Developer ID 公证。

安装：把 ChatOS.app 拖入 Applications。
首次运行：在 Finder 中按住 Control 点击 ChatOS，选择“打开”；如果系统仍阻止运行，
请前往“系统设置 → 隐私与安全性”，确认“仍要打开”。

请只从 ChatOS 官方发布页下载，并核对同目录提供的 SHA-256。
EOF

hdiutil create \
  -volname "ChatOS $VERSION" \
  -srcfolder "$STAGING_DIR" \
  -format UDZO \
  -ov \
  "$DMG_PATH"
shasum -a 256 "$DMG_PATH" > "$CHECKSUM_PATH"

print -r -- "$DMG_PATH"
print -r -- "$CHECKSUM_PATH"
