#!/bin/zsh
set -euo pipefail

APP_PATH=${1:-}
EXPECTED_CONFIGURATION=${2:-}
EXPECTED_GIT_COMMIT=${3:-}

if [[ -z "$APP_PATH" || ! -d "$APP_PATH" ]]; then
  echo "Usage: verify-app-build.sh APP_PATH [debug|release] [GIT_COMMIT]" >&2
  exit 2
fi

INFO_PLIST="$APP_PATH/Contents/Info.plist"
EXECUTABLE="$APP_PATH/Contents/MacOS/ChatOSSwift"
LOCAL_AGENT_HOST="$APP_PATH/Contents/MacOS/chatos_local_agent_host"
if [[ ! -f "$INFO_PLIST" || ! -x "$EXECUTABLE" || ! -x "$LOCAL_AGENT_HOST" ]]; then
  echo "Invalid ChatOS app bundle: $APP_PATH" >&2
  exit 1
fi

CONFIGURATION=$(plutil -extract ChatOSBuildConfiguration raw "$INFO_PLIST")
GIT_COMMIT=$(plutil -extract ChatOSBuildGitCommit raw "$INFO_PLIST")
BUILD_DATE=$(plutil -extract ChatOSBuildDateUTC raw "$INFO_PLIST")

if [[ "$CONFIGURATION" != "debug" && "$CONFIGURATION" != "release" ]]; then
  echo "Invalid ChatOS build configuration: $CONFIGURATION" >&2
  exit 1
fi
if [[ -n "$EXPECTED_CONFIGURATION" && "$CONFIGURATION" != "$EXPECTED_CONFIGURATION" ]]; then
  echo "ChatOS build configuration mismatch: actual=$CONFIGURATION expected=$EXPECTED_CONFIGURATION" >&2
  exit 1
fi
if [[ -n "$EXPECTED_GIT_COMMIT" && "$GIT_COMMIT" != "$EXPECTED_GIT_COMMIT" ]]; then
  echo "ChatOS Git commit mismatch: actual=$GIT_COMMIT expected=$EXPECTED_GIT_COMMIT" >&2
  exit 1
fi
if ! print -r -- "$GIT_COMMIT" | grep -Eq '^[0-9a-f]{40}$'; then
  echo "Invalid ChatOS Git commit metadata: $GIT_COMMIT" >&2
  exit 1
fi
if [[ -z "$BUILD_DATE" ]]; then
  echo "Missing ChatOS build date metadata" >&2
  exit 1
fi

if [[ "$CONFIGURATION" == "release" ]] && strings "$EXECUTABLE" | grep -F '/debug/' >/dev/null; then
  echo "Release ChatOS binary contains a Debug build path" >&2
  exit 1
fi
if [[ "$CONFIGURATION" == "release" ]] && strings "$LOCAL_AGENT_HOST" | grep -F '/debug/' >/dev/null; then
  echo "Release Local Agent Host contains a Debug build path" >&2
  exit 1
fi

codesign --verify --strict "$LOCAL_AGENT_HOST"
codesign --verify --deep --strict "$APP_PATH"
echo "Verified ChatOS $CONFIGURATION build $GIT_COMMIT ($BUILD_DATE)"
