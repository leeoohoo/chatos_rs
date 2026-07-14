#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

DESTINATION_DIR="${1:?usage: prepare-browser-runtime-macos.sh DESTINATION_DIR PLATFORM}"
PLATFORM="${2:?usage: prepare-browser-runtime-macos.sh DESTINATION_DIR PLATFORM}"
AGENT_BROWSER_VERSION="${CHATOS_AGENT_BROWSER_VERSION:-0.31.2}"
CHROME_VERSION="${CHATOS_CHROME_FOR_TESTING_VERSION:-150.0.7871.115}"
CACHE_ROOT="${CHATOS_BROWSER_RUNTIME_CACHE:-$HOME/Library/Caches/chatos-local-connector/browser-runtime}"

case "$PLATFORM" in
  macos-arm64)
    AGENT_BROWSER_ARCHIVE_BINARY="agent-browser-darwin-arm64"
    CHROME_PLATFORM="mac-arm64"
    ;;
  macos-x64)
    AGENT_BROWSER_ARCHIVE_BINARY="agent-browser-darwin-x64"
    CHROME_PLATFORM="mac-x64"
    ;;
  *)
    echo "Unsupported browser runtime platform: $PLATFORM" >&2
    exit 1
    ;;
esac

for command_name in curl ditto npm node tar; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required browser runtime command not found: $command_name" >&2
    exit 1
  fi
done

mkdir -p "$CACHE_ROOT" "$DESTINATION_DIR"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chatos-browser-runtime.XXXXXX")"
trap 'rm -rf "$WORK_DIR"' EXIT

AGENT_BROWSER_TARBALL="$CACHE_ROOT/agent-browser-$AGENT_BROWSER_VERSION.tgz"
if [[ ! -f "$AGENT_BROWSER_TARBALL" ]]; then
  echo "[INFO] Downloading agent-browser $AGENT_BROWSER_VERSION"
  npm pack "agent-browser@$AGENT_BROWSER_VERSION" \
    --pack-destination "$CACHE_ROOT" \
    --silent >/dev/null
fi

PACKAGE_VERSION="$(tar -xOf "$AGENT_BROWSER_TARBALL" package/package.json | node -e '
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", chunk => input += chunk);
process.stdin.on("end", () => process.stdout.write(JSON.parse(input).version));
')"
if [[ "$PACKAGE_VERSION" != "$AGENT_BROWSER_VERSION" ]]; then
  echo "agent-browser package version mismatch: expected $AGENT_BROWSER_VERSION, got $PACKAGE_VERSION" >&2
  exit 1
fi

tar -xzf "$AGENT_BROWSER_TARBALL" -C "$WORK_DIR" \
  "package/bin/$AGENT_BROWSER_ARCHIVE_BINARY" \
  package/LICENSE
cp "$WORK_DIR/package/bin/$AGENT_BROWSER_ARCHIVE_BINARY" "$DESTINATION_DIR/agent-browser"
cp "$WORK_DIR/package/LICENSE" "$DESTINATION_DIR/agent-browser.LICENSE"
chmod 755 "$DESTINATION_DIR/agent-browser"

CHROME_CACHE_DIR="$CACHE_ROOT/chrome-$CHROME_VERSION-$CHROME_PLATFORM"
CHROME_CACHE_APP="$CHROME_CACHE_DIR/Google Chrome for Testing.app"
AGENT_BROWSER_CACHE_APP="$HOME/.agent-browser/browsers/chrome-$CHROME_VERSION/Google Chrome for Testing.app"
if [[ ! -x "$CHROME_CACHE_APP/Contents/MacOS/Google Chrome for Testing" ]]; then
  rm -rf "$CHROME_CACHE_DIR"
  mkdir -p "$CHROME_CACHE_DIR"
  if [[ -x "$AGENT_BROWSER_CACHE_APP/Contents/MacOS/Google Chrome for Testing" ]]; then
    echo "[INFO] Reusing Chrome for Testing $CHROME_VERSION from agent-browser cache"
    ditto "$AGENT_BROWSER_CACHE_APP" "$CHROME_CACHE_APP"
  else
    CHROME_ARCHIVE="$CACHE_ROOT/chrome-$CHROME_VERSION-$CHROME_PLATFORM.zip"
    CHROME_URL="https://storage.googleapis.com/chrome-for-testing-public/$CHROME_VERSION/$CHROME_PLATFORM/chrome-$CHROME_PLATFORM.zip"
    if [[ ! -f "$CHROME_ARCHIVE" ]]; then
      echo "[INFO] Downloading Chrome for Testing $CHROME_VERSION ($CHROME_PLATFORM)"
      curl --fail --location --retry 3 --output "$CHROME_ARCHIVE.partial" "$CHROME_URL"
      mv "$CHROME_ARCHIVE.partial" "$CHROME_ARCHIVE"
    fi
    CHROME_EXTRACT_DIR="$WORK_DIR/chrome"
    mkdir -p "$CHROME_EXTRACT_DIR"
    ditto -x -k "$CHROME_ARCHIVE" "$CHROME_EXTRACT_DIR"
    EXTRACTED_APP="$CHROME_EXTRACT_DIR/chrome-$CHROME_PLATFORM/Google Chrome for Testing.app"
    if [[ ! -x "$EXTRACTED_APP/Contents/MacOS/Google Chrome for Testing" ]]; then
      echo "Chrome for Testing archive is incomplete: $CHROME_ARCHIVE" >&2
      exit 1
    fi
    ditto "$EXTRACTED_APP" "$CHROME_CACHE_APP"
  fi
fi

rm -rf "$DESTINATION_DIR/browser"
mkdir -p "$DESTINATION_DIR/browser"
ditto "$CHROME_CACHE_APP" "$DESTINATION_DIR/browser/Google Chrome for Testing.app"

AGENT_BROWSER_BIN="$DESTINATION_DIR/agent-browser"
CHROME_BIN="$DESTINATION_DIR/browser/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
if [[ ! -x "$AGENT_BROWSER_BIN" || ! -x "$CHROME_BIN" ]]; then
  echo "Packaged browser runtime is incomplete under $DESTINATION_DIR" >&2
  exit 1
fi

AGENT_VERSION_OUTPUT="$("$AGENT_BROWSER_BIN" --version)"
CHROME_VERSION_OUTPUT="$("$CHROME_BIN" --version)"
if [[ "$AGENT_VERSION_OUTPUT" != *"$AGENT_BROWSER_VERSION"* ]]; then
  echo "Unexpected agent-browser version: $AGENT_VERSION_OUTPUT" >&2
  exit 1
fi
if [[ "$CHROME_VERSION_OUTPUT" != *"$CHROME_VERSION"* ]]; then
  echo "Unexpected Chrome for Testing version: $CHROME_VERSION_OUTPUT" >&2
  exit 1
fi

if [[ "${CHATOS_SKIP_BROWSER_RUNTIME_SMOKE_TEST:-0}" != "1" ]]; then
  SMOKE_HOME="$WORK_DIR/smoke-home"
  SMOKE_SOCKET_DIR="/tmp/chatos-ab-package-$$"
  mkdir -p "$SMOKE_HOME" "$SMOKE_SOCKET_DIR"
  SMOKE_SESSION="pkg"
  SMOKE_ENV=(
    "PATH=/usr/bin:/bin"
    "HOME=$SMOKE_HOME"
    "AGENT_BROWSER_BIN=$AGENT_BROWSER_BIN"
    "AGENT_BROWSER_EXECUTABLE_PATH=$CHROME_BIN"
    "AGENT_BROWSER_SOCKET_DIR=$SMOKE_SOCKET_DIR"
  )
  OPEN_OUTPUT="$(env -i "${SMOKE_ENV[@]}" "$AGENT_BROWSER_BIN" --session "$SMOKE_SESSION" --json open about:blank)"
  SNAPSHOT_OUTPUT="$(env -i "${SMOKE_ENV[@]}" "$AGENT_BROWSER_BIN" --session "$SMOKE_SESSION" --json snapshot)"
  CLOSE_OUTPUT="$(env -i "${SMOKE_ENV[@]}" "$AGENT_BROWSER_BIN" --session "$SMOKE_SESSION" --json close)"
  for output in "$OPEN_OUTPUT" "$SNAPSHOT_OUTPUT" "$CLOSE_OUTPUT"; do
    node -e '
const payload = JSON.parse(process.argv[1]);
if (payload.success !== true) {
  throw new Error(payload.error || "agent-browser smoke test failed");
}
' "$output"
  done
  rm -rf "$SMOKE_SOCKET_DIR"
  SMOKE_TEST_RESULT="passed"
else
  SMOKE_TEST_RESULT="skipped"
fi

echo "[OK] Browser runtime: agent-browser $AGENT_BROWSER_VERSION"
echo "[OK] Browser runtime: Chrome for Testing $CHROME_VERSION"
if [[ "$SMOKE_TEST_RESULT" == "passed" ]]; then
  echo "[OK] Browser runtime smoke test: open, snapshot, close"
else
  echo "[INFO] Browser runtime smoke test skipped by CHATOS_SKIP_BROWSER_RUNTIME_SMOKE_TEST=1"
fi
