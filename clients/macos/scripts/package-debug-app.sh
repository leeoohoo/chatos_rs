#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
REPOSITORY_DIR=${PROJECT_DIR:h:h}
APP_DIR="$PROJECT_DIR/.build/ChatOS.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
TOOLS_DIR="$RESOURCES_DIR/Tools/darwin-arm64"
THIRD_PARTY_DIR="$RESOURCES_DIR/ThirdPartyNotices/ripgrep"
PET_DIR="$RESOURCES_DIR/Pets/fengtuan"
EN_LOCALIZATION_DIR="$RESOURCES_DIR/en.lproj"
ZH_HANS_LOCALIZATION_DIR="$RESOURCES_DIR/zh-Hans.lproj"
EXECUTABLE="$PROJECT_DIR/.build/arm64-apple-macosx/debug/ChatOSSwift"
KEYCHAIN_BROKER_EXECUTABLE="$PROJECT_DIR/.build/arm64-apple-macosx/debug/ChatOSKeychainBroker"
LOCAL_AGENT_EXECUTABLE="$REPOSITORY_DIR/target-shared/debug/chatos_local_agent_host"
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}
LOCAL_SIGNING_DIRECTORY=${CHATOS_LOCAL_SIGNING_DIRECTORY:-"/Users/$(id -un)/Library/Application Support/ChatOSSwift/DevelopmentSigning"}
LOCAL_SIGNING_KEYCHAIN="$LOCAL_SIGNING_DIRECTORY/signing.keychain-db"
LOCAL_SIGNING_PASSWORD_FILE="$LOCAL_SIGNING_DIRECTORY/keychain-password"
LOCAL_AGENT_IDENTIFIER=com.chatos.swift-client.local-agent-host
KEYCHAIN_BROKER_IDENTIFIER=com.chatos.swift-client.keychain-broker
typeset -a CODESIGN_KEYCHAIN_ARGUMENTS
USES_LOCAL_SIGNING_KEYCHAIN=0
local_signing_password=""

prepare_local_signing_keychain() {
  [[ "$USES_LOCAL_SIGNING_KEYCHAIN" == "1" ]] || return 0

  security unlock-keychain \
    -p "$local_signing_password" \
    "$LOCAL_SIGNING_KEYCHAIN"
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$local_signing_password" \
    "$LOCAL_SIGNING_KEYCHAIN" \
    >/dev/null
}

lock_local_signing_keychain() {
  [[ "$USES_LOCAL_SIGNING_KEYCHAIN" == "1" ]] || return 0
  security lock-keychain "$LOCAL_SIGNING_KEYCHAIN" >/dev/null 2>&1 || true
}

find_verified_apple_signing_identity() {
  local identity line common_name

  for certificate_prefix in "Developer ID Application:" "Apple Development:"; do
    line=$(security find-identity -v -p codesigning 2>/dev/null \
      | awk -v prefix="\"$certificate_prefix" 'index($0, prefix) {print; exit}')
    [[ -n "$line" ]] || continue

    identity=$(print -r -- "$line" | awk '{print $2}')
    common_name=${line#*\"}
    common_name=${common_name%%\"*}

    if security verify-cert \
      -c =(security find-certificate -c "$common_name" -p) \
      -p codeSign \
      -R ocsp \
      -R require \
      -q >/dev/null 2>&1; then
      print -r -- "$identity"
      return 0
    fi
  done

  return 1
}

if [[ -z "$SIGNING_IDENTITY" ]]; then
  if [[ -f "$LOCAL_SIGNING_KEYCHAIN" && -f "$LOCAL_SIGNING_PASSWORD_FILE" ]]; then
    local_signing_password=$(<"$LOCAL_SIGNING_PASSWORD_FILE")
    USES_LOCAL_SIGNING_KEYCHAIN=1
    prepare_local_signing_keychain
    SIGNING_IDENTITY=$(security find-identity \
      -v \
      -p codesigning \
      "$LOCAL_SIGNING_KEYCHAIN" 2>/dev/null \
      | awk '/"ChatOS Local Development"/{print $2; exit}')
    CODESIGN_KEYCHAIN_ARGUMENTS=(--keychain "$LOCAL_SIGNING_KEYCHAIN")
  else
    SIGNING_IDENTITY=$(find_verified_apple_signing_identity || true)
  fi
fi

if [[ -z "$SIGNING_IDENTITY" ]]; then
  print -u2 -- "No usable stable code-signing identity is available."
  print -u2 -- "Run clients/macos/scripts/setup-local-signing-identity.sh once,"
  print -u2 -- "install a valid Apple signing certificate, or explicitly set"
  print -u2 -- "CHATOS_CODESIGN_IDENTITY for this build."
  exit 1
fi

cd "$PROJECT_DIR"
"$PROJECT_DIR/scripts/audit-interface-localization.sh"
swift build
swift build --product ChatOSKeychainBroker
cd "$REPOSITORY_DIR"
cargo build -p chatos_local_agent_host

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$TOOLS_DIR" "$THIRD_PARTY_DIR" "$PET_DIR" "$EN_LOCALIZATION_DIR" "$ZH_HANS_LOCALIZATION_DIR"
cp "$EXECUTABLE" "$MACOS_DIR/ChatOSSwift"
cp "$KEYCHAIN_BROKER_EXECUTABLE" "$MACOS_DIR/chatos_keychain_broker"
cp "$LOCAL_AGENT_EXECUTABLE" "$MACOS_DIR/chatos_local_agent_host"
cp "$PROJECT_DIR/Support/ChatOSSwift-Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$PROJECT_DIR/Support/Tools/darwin-arm64/rg" "$TOOLS_DIR/rg"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/LICENSE-MIT" "$THIRD_PARTY_DIR/LICENSE-MIT"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/UNLICENSE" "$THIRD_PARTY_DIR/UNLICENSE"
cp "$PROJECT_DIR/Support/Pets/fengtuan/pet.json" "$PET_DIR/pet.json"
cp "$PROJECT_DIR/Support/Pets/fengtuan/spritesheet.webp" "$PET_DIR/spritesheet.webp"
cp "$PROJECT_DIR/Support/Localization/en.lproj/Localizable.strings" "$EN_LOCALIZATION_DIR/Localizable.strings"
cp "$PROJECT_DIR/Support/Localization/zh-Hans.lproj/Localizable.strings" "$ZH_HANS_LOCALIZATION_DIR/Localizable.strings"
chmod 755 "$TOOLS_DIR/rg"
chmod 755 "$MACOS_DIR/chatos_keychain_broker"
chmod 755 "$MACOS_DIR/chatos_local_agent_host"

# Compilation can outlive the signing Keychain's automatic unlock timeout.
# Re-open it immediately before the first signature so codesign never falls
# back to an interactive macOS password prompt.
prepare_local_signing_keychain
trap lock_local_signing_keychain EXIT

codesign \
  --force \
  --sign "$SIGNING_IDENTITY" \
  "${CODESIGN_KEYCHAIN_ARGUMENTS[@]}" \
  --timestamp=none \
  "$TOOLS_DIR/rg"
codesign \
  --force \
  --sign "$SIGNING_IDENTITY" \
  "${CODESIGN_KEYCHAIN_ARGUMENTS[@]}" \
  --identifier "$KEYCHAIN_BROKER_IDENTIFIER" \
  --timestamp=none \
  "$MACOS_DIR/chatos_keychain_broker"
codesign \
  --force \
  --sign "$SIGNING_IDENTITY" \
  "${CODESIGN_KEYCHAIN_ARGUMENTS[@]}" \
  --identifier "$LOCAL_AGENT_IDENTIFIER" \
  --timestamp=none \
  "$MACOS_DIR/chatos_local_agent_host"
codesign \
  --force \
  --sign "$SIGNING_IDENTITY" \
  "${CODESIGN_KEYCHAIN_ARGUMENTS[@]}" \
  --timestamp=none \
  "$APP_DIR"

lock_local_signing_keychain
trap - EXIT

echo "$APP_DIR"
