#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "ChatOS macOS supports arm64 only. Run from a native Apple Silicon shell." >&2
  exit 2
fi
native_python() {
  /usr/bin/arch -arm64 /usr/bin/python3 -B "$@"
}
PROJECT_DIR=${SCRIPT_DIR:h}
REPOSITORY_DIR=${PROJECT_DIR:h:h}
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

# Keep diagnostic Debug artifacts physically separate from distributable Release
# artifacts. A later Debug build must never overwrite the app selected for upload
# or installation by a release workflow.
APP_DIR="$PROJECT_DIR/.build/$BUILD_CONFIGURATION/ChatOS.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
TOOLS_DIR="$RESOURCES_DIR/Tools/darwin-arm64"
RIPGREP_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/ripgrep"
SWIFTTERM_NOTICE_DIR="$RESOURCES_DIR/ThirdPartyNotices/SwiftTerm"
PET_DIR="$RESOURCES_DIR/Pets/fengtuan"
EN_LOCALIZATION_DIR="$RESOURCES_DIR/en.lproj"
ZH_HANS_LOCALIZATION_DIR="$RESOURCES_DIR/zh-Hans.lproj"
APP_ICON_SOURCE="$PROJECT_DIR/../windows-react/src-tauri/icons/icon.icns"

if [[ "$BUILD_CONFIGURATION" == "release" && -n "$(git -C "$PROJECT_DIR" status --porcelain)" && "${CHATOS_ALLOW_DIRTY_RELEASE:-0}" != "1" ]]; then
  echo "Refusing to package a Release app from a dirty worktree." >&2
  echo "Commit and push the intended source, or build it from a clean worktree." >&2
  exit 2
fi

BUILD_GIT_COMMIT=${CHATOS_BUILD_GIT_COMMIT:-$(git -C "$PROJECT_DIR" rev-parse HEAD)}
BUILD_DATE_UTC=${CHATOS_BUILD_DATE_UTC:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}

# Evidence is kept outside the signed bundle (hashing a manifest into itself or
# modifying a bundle after signing would invalidate its provenance/signature).
mkdir -p "$PROJECT_DIR/.build"
EVIDENCE_DIR=$(mktemp -d "$PROJECT_DIR/.build/build-evidence.XXXXXXXX")
EVIDENCE_MANIFEST="$APP_DIR.build-manifest.json"
SWIFT_DEBUG_ARGUMENTS=()
SYMBOL_ARGUMENTS=()
if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  SWIFT_DEBUG_ARGUMENTS=(-Xswiftc -g)
  export CARGO_PROFILE_RELEASE_DEBUG=1
fi
native_python "$SCRIPT_DIR/build_evidence.py" capture \
  --repository "$REPOSITORY_DIR" --configuration "$BUILD_CONFIGURATION" \
  --swift-build-system "$SWIFT_BUILD_SYSTEM" --output "$EVIDENCE_DIR/inputs.json"

record_command() {
  native_python "$SCRIPT_DIR/build_evidence.py" record-command \
    --output "$EVIDENCE_DIR/commands.json" -- "$@"
}

# The native build path is intentionally retained because SwiftBuild currently links
# this AppKit executable with an obsolete SDK load command. SwiftPM also prints a
# deprecation notice for that required workaround; filter only that known notice.
swiftpm() {
  record_command swift "$@"
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
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  "${SWIFT_DEBUG_ARGUMENTS[@]}" \
  --product ChatOSSwift
swiftpm build \
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  "${SWIFT_DEBUG_ARGUMENTS[@]}" \
  --product chatos_plugin_ps
swiftpm build \
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  "${SWIFT_DEBUG_ARGUMENTS[@]}" \
  --product ChatOSMediaWorker
swiftpm build \
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  "${SWIFT_DEBUG_ARGUMENTS[@]}" \
  --product ChatOSCodeReadWorker
swiftpm build \
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  "${SWIFT_DEBUG_ARGUMENTS[@]}" \
  --product ChatOSAgentWorker
BIN_DIR=$(swiftpm build \
  --arch arm64 \
  --force-resolved-versions \
  --build-system "$SWIFT_BUILD_SYSTEM" \
  --scratch-path "$SWIFT_SCRATCH_PATH" \
  --configuration "$BUILD_CONFIGURATION" \
  --show-bin-path)
EXECUTABLE="$BIN_DIR/ChatOSSwift"
PLUGIN_PROCESS_LIST="$BIN_DIR/chatos_plugin_ps"
MEDIA_WORKER="$BIN_DIR/ChatOSMediaWorker"
CODE_READ_WORKER="$BIN_DIR/ChatOSCodeReadWorker"
AGENT_WORKER="$BIN_DIR/ChatOSAgentWorker"
if [[ ! -x "$EXECUTABLE" ]]; then
  echo "ChatOSSwift executable not found at $EXECUTABLE" >&2
  exit 1
fi
if [[ ! -x "$PLUGIN_PROCESS_LIST" ]]; then
  echo "Plugin process-list helper not found at $PLUGIN_PROCESS_LIST" >&2
  exit 1
fi
if [[ ! -x "$MEDIA_WORKER" ]]; then
  echo "Media Worker executable not found at $MEDIA_WORKER" >&2
  exit 1
fi
if [[ ! -x "$CODE_READ_WORKER" ]]; then
  echo "Code Read Worker executable not found at $CODE_READ_WORKER" >&2
  exit 1
fi
if [[ ! -x "$AGENT_WORKER" ]]; then
  echo "Agent Worker executable not found at $AGENT_WORKER" >&2
  exit 1
fi

CARGO_PROFILE_ARGUMENTS=()
RUST_PROFILE=debug
if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  CARGO_PROFILE_ARGUMENTS=(--release)
  RUST_PROFILE=release
fi
record_command cargo build --locked \
  --manifest-path "$REPOSITORY_DIR/Cargo.toml" \
  -p chatos_local_agent_host "${CARGO_PROFILE_ARGUMENTS[@]}"
cargo build --locked \
  --manifest-path "$REPOSITORY_DIR/Cargo.toml" \
  -p chatos_local_agent_host \
  "${CARGO_PROFILE_ARGUMENTS[@]}"
CARGO_TARGET_ROOT=${CARGO_TARGET_DIR:-"$REPOSITORY_DIR/target-shared"}
if [[ "$CARGO_TARGET_ROOT" != /* ]]; then
  CARGO_TARGET_ROOT="$REPOSITORY_DIR/$CARGO_TARGET_ROOT"
fi
LOCAL_AGENT_HOST="$CARGO_TARGET_ROOT/$RUST_PROFILE/chatos_local_agent_host"
if [[ ! -x "$LOCAL_AGENT_HOST" ]]; then
  echo "Local Agent Host executable not found at $LOCAL_AGENT_HOST" >&2
  exit 1
fi

if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  for SYMBOL_BINARY in "$EXECUTABLE" "$LOCAL_AGENT_HOST" "$PLUGIN_PROCESS_LIST" "$MEDIA_WORKER" "$CODE_READ_WORKER" "$AGENT_WORKER"; do
    SYMBOL_PATH="$EVIDENCE_DIR/${SYMBOL_BINARY:t}.dSYM"
    record_command dsymutil "$SYMBOL_BINARY" -o "$SYMBOL_PATH"
    dsymutil "$SYMBOL_BINARY" -o "$SYMBOL_PATH"
    SYMBOL_ARGUMENTS+=(--symbols "$SYMBOL_PATH")
  done
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
cp "$MEDIA_WORKER" "$MACOS_DIR/ChatOSMediaWorker"
cp "$CODE_READ_WORKER" "$MACOS_DIR/ChatOSCodeReadWorker"
cp "$AGENT_WORKER" "$MACOS_DIR/ChatOSAgentWorker"
copy_swift_resource_bundle() {
  local bundle_name=$1
  local source_bundle="$BIN_DIR/$bundle_name"
  if [[ ! -d "$source_bundle" ]]; then
    echo "Swift resource bundle not found at $source_bundle" >&2
    exit 1
  fi

  # Resource bundles belong under Contents/Resources in a signed macOS app.
  # Product code resolves this location explicitly because SwiftPM's native
  # accessor otherwise checks the sealed app root and its build-machine path.
  cp -R "$source_bundle" "$RESOURCES_DIR/"
}

copy_swift_resource_bundle "ChatOSSwift_ChatOSCore.bundle"
copy_swift_resource_bundle "ChatOSSwift_ChatOSModelCapabilities.bundle"
copy_swift_resource_bundle "SwiftTerm_SwiftTerm.bundle"
cp "$PROJECT_DIR/Support/ChatOSSwift-Info.plist" "$CONTENTS_DIR/Info.plist"
if [[ ! -f "$APP_ICON_SOURCE" ]]; then
  echo "ChatOS application icon not found at $APP_ICON_SOURCE" >&2
  exit 1
fi
cp "$APP_ICON_SOURCE" "$RESOURCES_DIR/ChatOS.icns"
set_plist_string "ChatOSBuildConfiguration" "$BUILD_CONFIGURATION"
set_plist_string "ChatOSBuildGitCommit" "$BUILD_GIT_COMMIT"
set_plist_string "ChatOSBuildDateUTC" "$BUILD_DATE_UTC"
cp "$PROJECT_DIR/Support/Tools/darwin-arm64/rg" "$TOOLS_DIR/rg"
cp "$PLUGIN_PROCESS_LIST" "$TOOLS_DIR/ps"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/LICENSE-MIT" "$RIPGREP_NOTICE_DIR/LICENSE-MIT"
cp "$PROJECT_DIR/Support/ThirdParty/ripgrep/UNLICENSE" "$RIPGREP_NOTICE_DIR/UNLICENSE"
cp "$PROJECT_DIR/Support/ThirdParty/SwiftTerm/LICENSE" "$SWIFTTERM_NOTICE_DIR/LICENSE"
cp "$PROJECT_DIR/Support/Pets/fengtuan/pet.json" "$PET_DIR/pet.json"
cp "$PROJECT_DIR/Support/Pets/fengtuan/spritesheet.webp" "$PET_DIR/spritesheet.webp"
cp "$PROJECT_DIR/Support/Localization/en.lproj/Localizable.strings" "$EN_LOCALIZATION_DIR/Localizable.strings"
cp "$PROJECT_DIR/Support/Localization/zh-Hans.lproj/Localizable.strings" "$ZH_HANS_LOCALIZATION_DIR/Localizable.strings"
chmod 755 "$TOOLS_DIR/rg"
chmod 755 "$TOOLS_DIR/ps"
chmod 755 "$MACOS_DIR/chatos_local_agent_host"
chmod 755 "$MACOS_DIR/ChatOSMediaWorker"
chmod 755 "$MACOS_DIR/ChatOSCodeReadWorker"
chmod 755 "$MACOS_DIR/ChatOSAgentWorker"

if [[ -n "$SIGNING_IDENTITY" ]]; then
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$TOOLS_DIR/rg"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$TOOLS_DIR/ps"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$MACOS_DIR/chatos_local_agent_host"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$MACOS_DIR/ChatOSMediaWorker"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$MACOS_DIR/ChatOSCodeReadWorker"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$MACOS_DIR/ChatOSAgentWorker"
  codesign --force --sign "$SIGNING_IDENTITY" --timestamp=none "$APP_DIR"
else
  # Keep a stable designated requirement across local debug rebuilds. A plain
  # ad-hoc signature falls back to its changing cdhash and makes Keychain treat
  # every build as a different application.
  codesign --force --sign - "$TOOLS_DIR/rg"
  codesign --force --sign - "$TOOLS_DIR/ps"
  codesign --force --sign - "$MACOS_DIR/chatos_local_agent_host"
  codesign --force --sign - "$MACOS_DIR/ChatOSMediaWorker"
  codesign --force --sign - "$MACOS_DIR/ChatOSCodeReadWorker"
  codesign --force --sign - "$MACOS_DIR/ChatOSAgentWorker"
  codesign \
    --force \
    --sign - \
    --requirements '=designated => identifier "com.chatos.swift-client"' \
    "$APP_DIR"
fi

native_python "$SCRIPT_DIR/build_evidence.py" finish \
  --repository "$REPOSITORY_DIR" --app "$APP_DIR" \
  --capture "$EVIDENCE_DIR/inputs.json" --commands "$EVIDENCE_DIR/commands.json" \
  "${SYMBOL_ARGUMENTS[@]}" --output "$EVIDENCE_MANIFEST"

"$SCRIPT_DIR/verify-app-build.sh" \
  "$APP_DIR" \
  "$BUILD_CONFIGURATION" \
  "$BUILD_GIT_COMMIT" \
  "${CHATOS_VERIFY_LAUNCH_SMOKE:-0}"

echo "$APP_DIR"
