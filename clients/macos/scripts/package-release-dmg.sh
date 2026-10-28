#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
INFO_PLIST="$PROJECT_DIR/Support/ChatOSSwift-Info.plist"
ARCH=$(uname -m)
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}
NOTARY_PROFILE=${CHATOS_NOTARYTOOL_PROFILE:-}

if [[ "$ARCH" != "arm64" ]]; then
  echo "The bundled ripgrep tool currently supports Apple Silicon only; refusing to label an $ARCH build as distributable." >&2
  exit 2
fi
if [[ "$SIGNING_IDENTITY" != "Developer ID Application:"* ]]; then
  echo "CHATOS_CODESIGN_IDENTITY must name a valid Developer ID Application identity." >&2
  exit 2
fi
if [[ -z "$NOTARY_PROFILE" ]]; then
  echo "CHATOS_NOTARYTOOL_PROFILE must name a Keychain profile created by notarytool store-credentials." >&2
  exit 2
fi
IDENTITY_DIAGNOSTICS=$(security find-identity -v -p codesigning | grep -F "\"$SIGNING_IDENTITY\"" || true)
if [[ -z "$IDENTITY_DIAGNOSTICS" ]]; then
  echo "Developer ID signing identity is not available in the current Keychain: $SIGNING_IDENTITY" >&2
  exit 2
fi
if ! print -r -- "$IDENTITY_DIAGNOSTICS" | grep -Fv 'CSSMERR_' >/dev/null; then
  echo "Developer ID signing identity is revoked or otherwise untrusted: $SIGNING_IDENTITY" >&2
  print -r -- "$IDENTITY_DIAGNOSTICS" >&2
  exit 2
fi
xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null

VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INFO_PLIST")
OUTPUT_DIR="$PROJECT_DIR/BundleArtifacts"
DMG_FILENAME="ChatOS-$VERSION-macOS-$ARCH.dmg"
DMG_PATH="$OUTPUT_DIR/$DMG_FILENAME"
CHECKSUM_PATH="$DMG_PATH.sha256"
APP_NOTARY_RESULT="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.app-notary.json"
DMG_NOTARY_RESULT="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.dmg-notary.json"
WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chatos-macos-release.XXXXXX")
DMG_STAGING_DIR="$WORK_DIR/dmg"
APP_ARCHIVE="$WORK_DIR/ChatOS.zip"
DMG_WORK_PATH="$WORK_DIR/$DMG_FILENAME"

cleanup() {
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

APP_PATH=$("$SCRIPT_DIR/package-release-app.sh" | tail -n 1)
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
if ! codesign -d --verbose=4 "$APP_PATH" 2>&1 | grep -q 'flags=.*runtime'; then
  echo "Release App is missing the hardened-runtime code-signing flag." >&2
  exit 1
fi
if ! codesign -d --verbose=4 "$APP_PATH" 2>&1 | grep -Fq "Authority=$SIGNING_IDENTITY"; then
  echo "Release App is not signed by the requested Developer ID identity." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
ditto -c -k --keepParent "$APP_PATH" "$APP_ARCHIVE"
xcrun notarytool submit "$APP_ARCHIVE" \
  --keychain-profile "$NOTARY_PROFILE" \
  --wait \
  --output-format json > "$APP_NOTARY_RESULT"
if [[ $(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("status", ""))' "$APP_NOTARY_RESULT") != "Accepted" ]]; then
  echo "Apple rejected the notarized App archive; see $APP_NOTARY_RESULT" >&2
  exit 1
fi
xcrun stapler staple "$APP_PATH"
xcrun stapler validate "$APP_PATH"
spctl --assess --type execute --verbose=4 "$APP_PATH"

mkdir -p "$DMG_STAGING_DIR"
ditto "$APP_PATH" "$DMG_STAGING_DIR/ChatOS.app"
ln -s /Applications "$DMG_STAGING_DIR/Applications"
cat > "$DMG_STAGING_DIR/安装说明.txt" <<'EOF'
安装：把 ChatOS.app 拖入 Applications，然后从 Applications 打开。
本安装包已使用 Apple Developer ID 签名、公证并附加离线验证票据。
请只从 ChatOS 官方发布页下载，并核对同目录提供的 SHA-256。
EOF

hdiutil create \
  -volname "ChatOS $VERSION" \
  -srcfolder "$DMG_STAGING_DIR" \
  -format UDZO \
  -ov \
  "$DMG_WORK_PATH"
codesign --force --sign "$SIGNING_IDENTITY" --timestamp "$DMG_WORK_PATH"
codesign --verify --strict --verbose=2 "$DMG_WORK_PATH"
xcrun notarytool submit "$DMG_WORK_PATH" \
  --keychain-profile "$NOTARY_PROFILE" \
  --wait \
  --output-format json > "$DMG_NOTARY_RESULT"
if [[ $(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("status", ""))' "$DMG_NOTARY_RESULT") != "Accepted" ]]; then
  echo "Apple rejected the notarized DMG; see $DMG_NOTARY_RESULT" >&2
  exit 1
fi
xcrun stapler staple "$DMG_WORK_PATH"
xcrun stapler validate "$DMG_WORK_PATH"
spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG_WORK_PATH"
mv -f "$DMG_WORK_PATH" "$DMG_PATH"
(
  cd "$OUTPUT_DIR"
  shasum -a 256 "$DMG_FILENAME" > "$DMG_FILENAME.sha256"
)

print -r -- "$DMG_PATH"
print -r -- "$CHECKSUM_PATH"
print -r -- "$APP_NOTARY_RESULT"
print -r -- "$DMG_NOTARY_RESULT"
