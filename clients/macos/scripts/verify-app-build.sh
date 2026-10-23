#!/bin/zsh
set -euo pipefail

APP_PATH=${1:-}
EXPECTED_CONFIGURATION=${2:-}
EXPECTED_GIT_COMMIT=${3:-}
RUN_LAUNCH_SMOKE=${4:-0}
SCRIPT_DIR=${0:A:h}
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "ChatOS macOS verification requires a native arm64 shell; Intel is unsupported." >&2
  exit 2
fi

if [[ -z "$APP_PATH" || ! -d "$APP_PATH" ]]; then
  echo "Usage: verify-app-build.sh APP_PATH [debug|release] [GIT_COMMIT]" >&2
  exit 2
fi

INFO_PLIST="$APP_PATH/Contents/Info.plist"
EXECUTABLE="$APP_PATH/Contents/MacOS/ChatOSSwift"
LOCAL_AGENT_HOST="$APP_PATH/Contents/MacOS/chatos_local_agent_host"
MEDIA_WORKER="$APP_PATH/Contents/MacOS/ChatOSMediaWorker"
AGENT_WORKER="$APP_PATH/Contents/MacOS/ChatOSAgentWorker"
PLUGIN_PROCESS_LIST="$APP_PATH/Contents/Resources/Tools/darwin-arm64/ps"
if [[ ! -f "$INFO_PLIST" || ! -x "$EXECUTABLE" || ! -x "$LOCAL_AGENT_HOST" || ! -x "$MEDIA_WORKER" || ! -x "$AGENT_WORKER" || ! -x "$PLUGIN_PROCESS_LIST" ]]; then
  echo "Invalid ChatOS app bundle: $APP_PATH" >&2
  exit 1
fi

for BINARY in "$EXECUTABLE" "$LOCAL_AGENT_HOST" "$MEDIA_WORKER" "$AGENT_WORKER" "$PLUGIN_PROCESS_LIST" \
    "$APP_PATH/Contents/Resources/Tools/darwin-arm64/rg"; do
  if [[ "$(/usr/bin/lipo -archs "$BINARY")" != "arm64" ]]; then
    echo "Expected arm64-only macOS executable: $BINARY" >&2
    exit 1
  fi
done

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

for RESOURCE_BUNDLE in \
  ChatOSSwift_ChatOSCore.bundle \
  ChatOSSwift_ChatOSModelCapabilities.bundle \
  SwiftTerm_SwiftTerm.bundle
do
  if [[ ! -d "$APP_PATH/Contents/Resources/$RESOURCE_BUNDLE" ]]; then
    echo "Missing Swift resource bundle: $APP_PATH/Contents/Resources/$RESOURCE_BUNDLE" >&2
    exit 1
  fi
done

if [[ "$CONFIGURATION" == "release" ]] && strings "$EXECUTABLE" | grep -F '/debug/' >/dev/null; then
  echo "Release ChatOS binary contains a Debug build path" >&2
  exit 1
fi
if [[ "$CONFIGURATION" == "release" ]] && strings "$LOCAL_AGENT_HOST" | grep -F '/debug/' >/dev/null; then
  echo "Release Local Agent Host contains a Debug build path" >&2
  exit 1
fi
if [[ "$CONFIGURATION" == "release" ]] && strings "$MEDIA_WORKER" | grep -F '/debug/' >/dev/null; then
  echo "Release Media Worker contains a Debug build path" >&2
  exit 1
fi
if [[ "$CONFIGURATION" == "release" ]] && strings "$AGENT_WORKER" | grep -F '/debug/' >/dev/null; then
  echo "Release Agent Worker contains a Debug build path" >&2
  exit 1
fi

codesign --verify --strict "$LOCAL_AGENT_HOST"
codesign --verify --strict "$MEDIA_WORKER"
codesign --verify --strict "$AGENT_WORKER"
codesign --verify --strict "$PLUGIN_PROCESS_LIST"
codesign --verify --deep --strict "$APP_PATH"

# A signed binary alone does not establish which source/toolchain produced it.
# This also catches a stale manifest or replacement resource/helper after packaging.
/usr/bin/arch -arm64 /usr/bin/python3 -B "$SCRIPT_DIR/build_evidence.py" verify \
  --app "$APP_PATH" --manifest "$APP_PATH.build-manifest.json"

if [[ "$RUN_LAUNCH_SMOKE" == "1" ]]; then
  SMOKE_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/chatos-app-smoke.XXXXXXXX")
  SMOKE_LOG="$SMOKE_ROOT/ChatOSSwift.log"
  SMOKE_PID=""
  cleanup_smoke() {
    if [[ -n "$SMOKE_PID" ]] && kill -0 "$SMOKE_PID" 2>/dev/null; then
      kill -TERM "$SMOKE_PID" 2>/dev/null || true
      wait "$SMOKE_PID" 2>/dev/null || true
    fi
    rm -rf -- "$SMOKE_ROOT"
  }
  trap cleanup_smoke EXIT INT TERM

  "$LOCAL_AGENT_HOST" --help >"$SMOKE_ROOT/local-agent-host.log" 2>&1
  if "$MEDIA_WORKER" >"$SMOKE_ROOT/media-worker.log" 2>&1; then
    echo "Media Worker accepted a launch without its inherited parent capability" >&2
    exit 1
  fi
  if "$AGENT_WORKER" >"$SMOKE_ROOT/agent-worker.log" 2>&1; then
    echo "Agent Worker accepted a launch without its inherited parent capability" >&2
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
      echo "Packaged ChatOS exited during launch smoke test" >&2
      sed -n '1,200p' "$SMOKE_LOG" >&2
      exit 1
    fi
    sleep 0.1
  done

  if find "$SMOKE_ROOT/Library/Logs/DiagnosticReports" -maxdepth 1 \
      -type f -name 'ChatOSSwift*' -print -quit 2>/dev/null | grep -q .; then
    echo "Packaged ChatOS produced a crash report during launch smoke test" >&2
    exit 1
  fi

  for _ in {1..30}; do
    if ! kill -0 "$SMOKE_PID" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if kill -0 "$SMOKE_PID" 2>/dev/null; then
    echo "Packaged ChatOS did not terminate cleanly after launch smoke test" >&2
    exit 1
  fi
  if ! wait "$SMOKE_PID"; then
    echo "Packaged ChatOS returned a failure status after launch smoke test" >&2
    sed -n '1,200p' "$SMOKE_LOG" >&2
    exit 1
  fi
  SMOKE_PID=""
  trap - EXIT INT TERM
  rm -rf -- "$SMOKE_ROOT"
fi

echo "Verified ChatOS $CONFIGURATION build $GIT_COMMIT ($BUILD_DATE)"
