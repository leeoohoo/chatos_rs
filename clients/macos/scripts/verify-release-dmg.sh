#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
DMG_PATH=${1:-}
DISTRIBUTION_MANIFEST=${2:-}
EXPECTED_TEAM_ID=AHVN42RQS3
EXPECTED_BUNDLE_IDENTIFIER=com.chatos.swift-client

if [[ "$(uname -m)" != "arm64" ]]; then
  echo "ChatOS release verification requires native arm64 Apple Silicon." >&2
  exit 2
fi
if [[ -z "$DMG_PATH" || -z "$DISTRIBUTION_MANIFEST" || ! -f "$DMG_PATH" || ! -f "$DISTRIBUTION_MANIFEST" ]]; then
  echo "Usage: verify-release-dmg.sh DMG_PATH DISTRIBUTION_MANIFEST" >&2
  exit 2
fi
DMG_PATH=${DMG_PATH:A}
DISTRIBUTION_MANIFEST=${DISTRIBUTION_MANIFEST:A}

TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/chatos-release-verification.XXXXXX")
MOUNT_POINT="$TEST_ROOT/mounted"
INSTALL_ROOT="$TEST_ROOT/Applications"
INSTALLED_APP="$INSTALL_ROOT/ChatOS.app"
SMOKE_ROOT="$TEST_ROOT/smoke-home"
SMOKE_LOG="$TEST_ROOT/ChatOSSwift.log"
MOUNTED=0
SMOKE_PID=""

cleanup() {
  local exit_status=$?
  if [[ -n "$SMOKE_PID" ]] && kill -0 "$SMOKE_PID" 2>/dev/null; then
    kill -TERM "$SMOKE_PID" 2>/dev/null || true
    wait "$SMOKE_PID" 2>/dev/null || true
  fi
  if (( MOUNTED == 1 )); then
    hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true
  fi
  rm -rf -- "$TEST_ROOT" || true
  return "$exit_status"
}
trap cleanup EXIT INT TERM

codesign --verify --strict --verbose=2 "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"
spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG_PATH"

mkdir -p "$MOUNT_POINT" "$INSTALL_ROOT" "$SMOKE_ROOT"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT_POINT" "$DMG_PATH" >/dev/null
MOUNTED=1
MOUNTED_APP="$MOUNT_POINT/ChatOS.app"
if [[ ! -d "$MOUNTED_APP" ]]; then
  echo "DMG does not contain ChatOS.app at its root." >&2
  exit 1
fi
xcrun stapler validate "$MOUNTED_APP"
codesign --verify --deep --strict --verbose=2 "$MOUNTED_APP"
spctl --assess --type execute --verbose=4 "$MOUNTED_APP"

ditto "$MOUNTED_APP" "$INSTALLED_APP"
hdiutil detach "$MOUNT_POINT" -quiet
MOUNTED=0

xcrun stapler validate "$INSTALLED_APP"
codesign --verify --deep --strict --verbose=2 "$INSTALLED_APP"
spctl --assess --type execute --verbose=4 "$INSTALLED_APP"
/usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/build_evidence.py" verify-distribution \
  --app "$INSTALLED_APP" \
  --dmg "$DMG_PATH" \
  --manifest "$DISTRIBUTION_MANIFEST" \
  --expected-team-id "$EXPECTED_TEAM_ID" \
  --expected-bundle-identifier "$EXPECTED_BUNDLE_IDENTIFIER"

EXECUTABLE="$INSTALLED_APP/Contents/MacOS/ChatOSSwift"
LOCAL_AGENT_HOST="$INSTALLED_APP/Contents/MacOS/chatos_local_agent_host"
MEDIA_WORKER="$INSTALLED_APP/Contents/MacOS/ChatOSMediaWorker"
CODE_READ_WORKER="$INSTALLED_APP/Contents/MacOS/ChatOSCodeReadWorker"
AGENT_WORKER="$INSTALLED_APP/Contents/MacOS/ChatOSAgentWorker"
SIGNED_EXECUTABLES=(
  "$EXECUTABLE"
  "$LOCAL_AGENT_HOST"
  "$MEDIA_WORKER"
  "$CODE_READ_WORKER"
  "$AGENT_WORKER"
  "$INSTALLED_APP/Contents/Resources/Tools/darwin-arm64/rg"
  "$INSTALLED_APP/Contents/Resources/Tools/darwin-arm64/ps"
)
for executable in "${SIGNED_EXECUTABLES[@]}"; do
  if [[ ! -x "$executable" || $(lipo -archs "$executable") != "arm64" ]]; then
    echo "Installed release contains a missing or non-arm64 executable: $executable" >&2
    exit 1
  fi
  codesign --verify --strict --verbose=2 "$executable"
done

"$LOCAL_AGENT_HOST" --help >"$TEST_ROOT/local-agent-host.log" 2>&1
if "$MEDIA_WORKER" >"$TEST_ROOT/media-worker.log" 2>&1; then
  echo "Media Worker accepted launch without its inherited parent capability." >&2
  exit 1
fi
if "$CODE_READ_WORKER" >"$TEST_ROOT/code-read-worker.log" 2>&1; then
  echo "Code Read Worker accepted launch without its inherited parent capability." >&2
  exit 1
fi
if "$AGENT_WORKER" >"$TEST_ROOT/agent-worker.log" 2>&1; then
  echo "Agent Worker accepted launch without its inherited parent capability." >&2
  exit 1
fi

env \
  CFFIXED_USER_HOME="$SMOKE_ROOT" \
  CHATOS_BUILD_SMOKE_TEST=1 \
  CHATOS_BUILD_SMOKE_TEST_EXIT_AFTER_SECONDS=6 \
  "$EXECUTABLE" >"$SMOKE_LOG" 2>&1 &
SMOKE_PID=$!
for _ in {1..50}; do
  if ! kill -0 "$SMOKE_PID" 2>/dev/null; then
    wait "$SMOKE_PID" 2>/dev/null || true
    echo "Installed ChatOS exited during launch smoke test." >&2
    sed -n '1,200p' "$SMOKE_LOG" >&2
    exit 1
  fi
  sleep 0.1
done
for _ in {1..30}; do
  if ! kill -0 "$SMOKE_PID" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$SMOKE_PID" 2>/dev/null; then
  echo "Installed ChatOS did not terminate after launch smoke test." >&2
  exit 1
fi
if ! wait "$SMOKE_PID"; then
  echo "Installed ChatOS returned a failure status after launch smoke test." >&2
  sed -n '1,200p' "$SMOKE_LOG" >&2
  exit 1
fi
SMOKE_PID=""

if find "$SMOKE_ROOT/Library/Logs/DiagnosticReports" -maxdepth 1 \
    -type f -name 'ChatOSSwift*' -print -quit 2>/dev/null | grep -q .; then
  echo "Installed ChatOS produced a crash report during launch smoke test." >&2
  exit 1
fi

trap - EXIT INT TERM
rm -rf -- "$TEST_ROOT"
print -r -- "Verified signed/notarized ChatOS DMG and isolated installed-App smoke."
