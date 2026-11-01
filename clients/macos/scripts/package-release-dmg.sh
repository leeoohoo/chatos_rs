#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
INFO_PLIST="$PROJECT_DIR/Support/ChatOSSwift-Info.plist"
ARCH=$(uname -m)
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}
NOTARY_PROFILE=${CHATOS_NOTARYTOOL_PROFILE:-}
EXPECTED_TEAM_ID=AHVN42RQS3

if [[ "$ARCH" != "arm64" ]]; then
  echo "The bundled ripgrep tool requires native arm64 Apple Silicon; refusing to label an $ARCH build as distributable." >&2
  exit 2
fi
if [[ "$SIGNING_IDENTITY" != "Developer ID Application:"* ]]; then
  echo "CHATOS_CODESIGN_IDENTITY must name a valid Developer ID Application identity." >&2
  exit 2
fi
if [[ "$SIGNING_IDENTITY" != *"($EXPECTED_TEAM_ID)" ]]; then
  echo "CHATOS_CODESIGN_IDENTITY must belong to the official ChatOS Team ID: $EXPECTED_TEAM_ID" >&2
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
APP_NOTARY_ARTIFACT="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.app-notary.json"
DMG_NOTARY_ARTIFACT="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.dmg-notary.json"
DISTRIBUTION_MANIFEST="$OUTPUT_DIR/ChatOS-$VERSION-macOS-$ARCH.distribution.json"
WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chatos-macos-release.XXXXXX")
DMG_STAGING_DIR="$WORK_DIR/dmg"
APP_ARCHIVE="$WORK_DIR/ChatOS.zip"
DMG_WORK_PATH="$WORK_DIR/$DMG_FILENAME"
CHECKSUM_WORK_PATH="$WORK_DIR/$DMG_FILENAME.sha256"
APP_NOTARY_RESULT="$WORK_DIR/${APP_NOTARY_ARTIFACT:t}"
DMG_NOTARY_RESULT="$WORK_DIR/${DMG_NOTARY_ARTIFACT:t}"
DISTRIBUTION_MANIFEST_WORK="$WORK_DIR/${DISTRIBUTION_MANIFEST:t}"

cleanup() {
  local exit_status=$?
  if (( exit_status != 0 )); then
    local rejected_dir="$OUTPUT_DIR/rejected"
    local failure_stamp
    failure_stamp=$(date -u '+%Y%m%dT%H%M%SZ')-$$
    for result in "$APP_NOTARY_RESULT" "$DMG_NOTARY_RESULT"; do
      if [[ -s "$result" ]]; then
        mkdir -p "$rejected_dir" || true
        cp "$result" "$rejected_dir/${result:t:r}-$failure_stamp.json" || true
      fi
    done
  fi
  rm -rf -- "$WORK_DIR" || true
  return "$exit_status"
}
trap cleanup EXIT

APP_PATH=$("$SCRIPT_DIR/package-release-app.sh" | tail -n 1)
BUILD_MANIFEST="$APP_PATH.build-manifest.json"
/usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/build_evidence.py" verify \
  --app "$APP_PATH" \
  --manifest "$BUILD_MANIFEST" \
  --require-symbols
SIGNED_EXECUTABLES=(
  "$APP_PATH/Contents/MacOS/ChatOSSwift"
  "$APP_PATH/Contents/MacOS/chatos_local_agent_host"
  "$APP_PATH/Contents/MacOS/ChatOSMediaWorker"
  "$APP_PATH/Contents/MacOS/ChatOSCodeReadWorker"
  "$APP_PATH/Contents/MacOS/ChatOSAgentWorker"
  "$APP_PATH/Contents/Resources/Tools/darwin-arm64/rg"
  "$APP_PATH/Contents/Resources/Tools/darwin-arm64/ps"
)
while IFS= read -r -d '' bundled_file; do
  if [[ $(file -b "$bundled_file") == *Mach-O* ]] && (( ${SIGNED_EXECUTABLES[(Ie)$bundled_file]} == 0 )); then
    echo "Release App contains an unmanaged Mach-O file; add it to the explicit inner-signing manifest: $bundled_file" >&2
    exit 1
  fi
done < <(find "$APP_PATH/Contents" -type f -print0)
for executable in "${SIGNED_EXECUTABLES[@]}"; do
  if [[ ! -f "$executable" ]]; then
    echo "Release App is missing a required signed executable: $executable" >&2
    exit 1
  fi
  if [[ $(lipo -archs "$executable") != "arm64" ]]; then
    echo "Release executable is not arm64-only: $executable" >&2
    exit 1
  fi
  codesign --verify --strict --verbose=2 "$executable"
  EXECUTABLE_SIGNATURE=$(codesign -d --verbose=4 "$executable" 2>&1)
  if [[ "$EXECUTABLE_SIGNATURE" != *"flags="*"runtime"* ]]; then
    echo "Release executable is missing the hardened-runtime flag: $executable" >&2
    exit 1
  fi
  if [[ "$EXECUTABLE_SIGNATURE" != *"Authority=$SIGNING_IDENTITY"* ]]; then
    echo "Release executable is not signed by the requested Developer ID identity: $executable" >&2
    exit 1
  fi
done
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
xcrun stapler validate "$DMG_STAGING_DIR/ChatOS.app"
codesign --verify --deep --strict --verbose=2 "$DMG_STAGING_DIR/ChatOS.app"
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
/usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/build_evidence.py" distribution \
  --app "$APP_PATH" \
  --build-manifest "$BUILD_MANIFEST" \
  --dmg "$DMG_WORK_PATH" \
  --app-notary "$APP_NOTARY_RESULT" \
  --dmg-notary "$DMG_NOTARY_RESULT" \
  --signing-identity "$SIGNING_IDENTITY" \
  --output "$DISTRIBUTION_MANIFEST_WORK"
(
  cd "$WORK_DIR"
  shasum -a 256 "$DMG_FILENAME" > "$DMG_FILENAME.sha256"
)
mv -f "$DMG_WORK_PATH" "$DMG_PATH"
mv -f "$CHECKSUM_WORK_PATH" "$CHECKSUM_PATH"
mv -f "$APP_NOTARY_RESULT" "$APP_NOTARY_ARTIFACT"
mv -f "$DMG_NOTARY_RESULT" "$DMG_NOTARY_ARTIFACT"
# The manifest is the publication marker and moves last. If the process is
# interrupted during earlier moves, the previous manifest cannot validate the
# partial artifact set as a new release.
mv -f "$DISTRIBUTION_MANIFEST_WORK" "$DISTRIBUTION_MANIFEST"

print -r -- "$DMG_PATH"
print -r -- "$CHECKSUM_PATH"
print -r -- "$APP_NOTARY_ARTIFACT"
print -r -- "$DMG_NOTARY_ARTIFACT"
print -r -- "$DISTRIBUTION_MANIFEST"
