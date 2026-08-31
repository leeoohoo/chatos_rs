#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
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
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}

cd "$PROJECT_DIR"
"$PROJECT_DIR/scripts/audit-interface-localization.sh"
swift build

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$TOOLS_DIR" "$THIRD_PARTY_DIR" "$PET_DIR" "$EN_LOCALIZATION_DIR" "$ZH_HANS_LOCALIZATION_DIR"
cp "$EXECUTABLE" "$MACOS_DIR/ChatOSSwift"
cp "$PROJECT_DIR/Support/ChatOSSwift-Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$PROJECT_DIR/Support/Tools/darwin-arm64/rg" "$TOOLS_DIR/rg"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/LICENSE-MIT" "$THIRD_PARTY_DIR/LICENSE-MIT"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/UNLICENSE" "$THIRD_PARTY_DIR/UNLICENSE"
cp "$PROJECT_DIR/Support/Pets/fengtuan/pet.json" "$PET_DIR/pet.json"
cp "$PROJECT_DIR/Support/Pets/fengtuan/spritesheet.webp" "$PET_DIR/spritesheet.webp"
cp "$PROJECT_DIR/Support/Localization/en.lproj/Localizable.strings" "$EN_LOCALIZATION_DIR/Localizable.strings"
cp "$PROJECT_DIR/Support/Localization/zh-Hans.lproj/Localizable.strings" "$ZH_HANS_LOCALIZATION_DIR/Localizable.strings"
chmod 755 "$TOOLS_DIR/rg"

if [[ -n "$SIGNING_IDENTITY" ]]; then
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$TOOLS_DIR/rg"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$APP_DIR"
else
  # Keep a stable designated requirement across local debug rebuilds. A plain
  # ad-hoc signature falls back to its changing cdhash and makes Keychain treat
  # every build as a different application.
  codesign --force --sign - "$TOOLS_DIR/rg"
  codesign \
    --force \
    --sign - \
    --requirements '=designated => identifier "com.chatos.swift-client"' \
    "$APP_DIR"
fi

echo "$APP_DIR"
