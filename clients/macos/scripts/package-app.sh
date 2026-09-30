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
RIPGREP_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/ripgrep"
SWIFTTERM_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/SwiftTerm"
PET_DIR="$RESOURCES_DIR/Pets/fengtuan"
EN_LOCALIZATION_DIR="$RESOURCES_DIR/en.lproj"
ZH_HANS_LOCALIZATION_DIR="$RESOURCES_DIR/zh-Hans.lproj"
SIGNING_IDENTITY=${CHATOS_CODESIGN_IDENTITY:-}
SWIFT_BUILD_SYSTEM=${CHATOS_SWIFT_BUILD_SYSTEM:-native}
SWIFT_SCRATCH_PATH=${CHATOS_SWIFT_SCRATCH_PATH:-"$PROJECT_DIR/.build-native"}
BUILD_CONFIGURATION=${CHATOS_BUILD_CONFIGURATION:-}

case "$BUILD_CONFIGURATION" in
  debug|release) ;;
  "")
    echo "CHATOS_BUILD_CONFIGURATION must be set explicitly to debug or release" >&2
    exit 2
    ;;
  *)
    echo "CHATOS_BUILD_CONFIGURATION must be debug or release" >&2
    exit 2
    ;;
esac

if [[ "$BUILD_CONFIGURATION" == "release" && -n "$(git -C "$PROJECT_DIR" status --porcelain)" && "${CHATOS_ALLOW_DIRTY_RELEASE:-0}" != "1" ]]; then
  echo "Refusing to package a Release app from a dirty worktree." >&2
  echo "Commit and push the intended source, or build it from a clean worktree." >&2
  exit 2
fi

BUILD_GIT_COMMIT=${CHATOS_BUILD_GIT_COMMIT:-$(git -C "$PROJECT_DIR" rev-parse HEAD)}
BUILD_DATE_UTC=${CHATOS_BUILD_DATE_UTC:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}

# The native build path is intentionally retained because SwiftBuild currently links
# this AppKit executable with an obsolete SDK load command. SwiftPM also prints a
# deprecation notice for that required workaround; filter only that known notice.
swiftpm() {
  command swift "$@" 2> >(sed '/warning: .--build-system native. has been deprecated and will be removed in a future release/d' >&2)
}

set_plist_string() {
  local key=$1
  local value=$2
  /usr/libexec/PlistBuddy -c "Delete :$key" "$CONTENTS_DIR/Info.plist" >/dev/null 2>&1 || true
  /usr/libexec/PlistBuddy -c "Add :$key string $value" "$CONTENTS_DIR/Info.plist"
}

cd "$PROJECT_DIR"
"$PROJECT_DIR/scripts/audit-interface-localization.sh"
swiftpm build \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  --product ChatOSSwift
BIN_DIR=$(swiftpm build \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  --show-bin-path)
EXECUTABLE="$BIN_DIR/ChatOSSwift"
if [[ ! -x "$EXECUTABLE" ]]; then
  echo "ChatOSSwift executable not found at $EXECUTABLE" >&2
  exit 1
fi

CARGO_PROFILE_ARGUMENTS=()
RUST_PROFILE=debug
if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  CARGO_PROFILE_ARGUMENTS=(--release)
  RUST_PROFILE=release
fi
cargo build \
  --manifest-path "$REPOSITORY_DIR/Cargo.toml" \
  -p chatos_local_agent_host \
  "${CARGO_PROFILE_ARGUMENTS[@]}"
LOCAL_AGENT_HOST="$REPOSITORY_DIR/target-shared/$RUST_PROFILE/chatos_local_agent_host"
if [[ ! -x "$LOCAL_AGENT_HOST" ]]; then
  echo "Local Agent Host executable not found at $LOCAL_AGENT_HOST" >&2
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
cp "$LOCAL_AGENT_HOST" "$MACOS_DIR/chatos_local_agent_host"
CORE_RESOURCE_BUNDLE="$BIN_DIR/ChatOSSwift_ChatOSCore.bundle"
if [[ ! -d "$CORE_RESOURCE_BUNDLE" ]]; then
  echo "ChatOSCore resource bundle not found at $CORE_RESOURCE_BUNDLE" >&2
  exit 1
fi
cp -R "$CORE_RESOURCE_BUNDLE" "$RESOURCES_DIR/"
cp "$PROJECT_DIR/Support/ChatOSSwift-Info.plist" "$CONTENTS_DIR/Info.plist"
set_plist_string "ChatOSBuildConfiguration" "$BUILD_CONFIGURATION"
set_plist_string "ChatOSBuildGitCommit" "$BUILD_GIT_COMMIT"
set_plist_string "ChatOSBuildDateUTC" "$BUILD_DATE_UTC"
cp "$PROJECT_DIR/Support/Tools/darwin-arm64/rg" "$TOOLS_DIR/rg"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/LICENSE-MIT" "$RIPGREP_NOTICE_DIR/LICENSE-MIT"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/UNLICENSE" "$RIPGREP_NOTICE_DIR/UNLICENSE"
cp "$PROJECT_DIR/Support/ThirdParty/SwiftTerm/LICENSE" "$SWIFTTERM_NOTICE_DIR/LICENSE"
cp "$PROJECT_DIR/Support/Pets/fengtuan/pet.json" "$PET_DIR/pet.json"
cp "$PROJECT_DIR/Support/Pets/fengtuan/spritesheet.webp" "$PET_DIR/spritesheet.webp"
cp "$PROJECT_DIR/Support/Localization/en.lproj/Localizable.strings" "$EN_LOCALIZATION_DIR/Localizable.strings"
cp "$PROJECT_DIR/Support/Localization/zh-Hans.lproj/Localizable.strings" "$ZH_HANS_LOCALIZATION_DIR/Localizable.strings"
chmod 755 "$TOOLS_DIR/rg"
chmod 755 "$MACOS_DIR/chatos_local_agent_host"

if [[ -n "$SIGNING_IDENTITY" ]]; then
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$TOOLS_DIR/rg"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$MACOS_DIR/chatos_local_agent_host"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$APP_DIR"
else
  # Keep a stable designated requirement across local debug rebuilds. A plain
  # ad-hoc signature falls back to its changing cdhash and makes Keychain treat
  # every build as a different application.
  codesign --force --sign - "$TOOLS_DIR/rg"
  codesign --force --sign - "$MACOS_DIR/chatos_local_agent_host"
  codesign \
    --force \
    --sign - \
    --requirements '=designated => identifier "com.chatos.swift-client"' \
    "$APP_DIR"
fi

"$SCRIPT_DIR/verify-app-build.sh" \
  "$APP_DIR" \
  "$BUILD_CONFIGURATION" \
  "$BUILD_GIT_COMMIT"

echo "$APP_DIR"
