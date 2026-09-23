#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}
APP_DIR="$PROJECT_DIR/.build/ChatOS.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
TOOLS_DIR="$RESOURCES_DIR/Tools/darwin-arm64"
RIPGREP_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/ripgrep"
SWIFTTERM_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/SwiftTerm"
PET_DIR="$RESOURCES_DIR/Pets/fengtuan"
EN_LOCALIZATION_DIR="$RESOURCES_DIR/en.lproj"
ZH_HANS_LOCALIZATION_DIR="$RESOURCES_DIR/zh-Hans.lproj"
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}
SWIFT_BUILD_SYSTEM=${CHATOS_SWIFT_BUILD_SYSTEM:-native}
SWIFT_SCRATCH_PATH=${CHATOS_SWIFT_SCRATCH_PATH:-"$PROJECT_DIR/.build-native"}

cd "$PROJECT_DIR"
"$PROJECT_DIR/scripts/audit-interface-localization.sh"
swift build \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --product ChatOSSwift
BIN_DIR=$(swift build \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --show-bin-path)
EXECUTABLE="$BIN_DIR/ChatOSSwift"
if [[ ! -x "$EXECUTABLE" ]]; then
  echo "ChatOSSwift executable not found at $EXECUTABLE" >&2
  exit 1
fi

# A binary produced by SwiftPM's alternate build path can compile and launch
# while carrying an older LC_BUILD_VERSION SDK. AppKit then selects legacy
# control rendering (notably square segmented controls), so never package a
# product that was linked against a different SDK than the active Xcode SDK.
EXPECTED_SDK_VERSION=$(xcrun --sdk macosx --show-sdk-version)
LINKED_SDK_VERSION=$(vtool -show-build "$EXECUTABLE" | awk '$1 == "sdk" { print $2; exit }')
if [[ -z "$LINKED_SDK_VERSION" || "$LINKED_SDK_VERSION" != "$EXPECTED_SDK_VERSION" ]]; then
  echo "ChatOSSwift SDK mismatch: linked=${LINKED_SDK_VERSION:-unknown}, expected=$EXPECTED_SDK_VERSION" >&2
  echo "Refusing to package an incompatible UI binary." >&2
  exit 1
fi

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$TOOLS_DIR" "$RIPGREP_NOTICE_DIR" "$SWIFTTERM_NOTICE_DIR" "$PET_DIR" "$EN_LOCALIZATION_DIR" "$ZH_HANS_LOCALIZATION_DIR"
cp "$EXECUTABLE" "$MACOS_DIR/ChatOSSwift"
CORE_RESOURCE_BUNDLE="$BIN_DIR/ChatOSSwift_ChatOSCore.bundle"
if [[ ! -d "$CORE_RESOURCE_BUNDLE" ]]; then
  echo "ChatOSCore resource bundle not found at $CORE_RESOURCE_BUNDLE" >&2
  exit 1
fi
cp -R "$CORE_RESOURCE_BUNDLE" "$RESOURCES_DIR/"
cp "$PROJECT_DIR/Support/ChatOSSwift-Info.plist" "$CONTENTS_DIR/Info.plist"
cp "$PROJECT_DIR/Support/Tools/darwin-arm64/rg" "$TOOLS_DIR/rg"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/LICENSE-MIT" "$RIPGREP_NOTICE_DIR/LICENSE-MIT"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/UNLICENSE" "$RIPGREP_NOTICE_DIR/UNLICENSE"
cp "$PROJECT_DIR/Support/ThirdParty/SwiftTerm/LICENSE" "$SWIFTTERM_NOTICE_DIR/LICENSE"
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
