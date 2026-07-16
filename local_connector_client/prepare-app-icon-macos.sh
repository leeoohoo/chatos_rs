#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <source-png> <output-icns>" >&2
  exit 1
fi

SOURCE_PNG="$1"
OUTPUT_ICNS="$2"

for command_name in sips iconutil; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required icon command not found: $command_name" >&2
    exit 1
  fi
done

if [[ ! -f "$SOURCE_PNG" ]]; then
  echo "Application icon source is missing: $SOURCE_PNG" >&2
  exit 1
fi

TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chatos-app-icon.XXXXXX")"
ICONSET_DIR="$TEMP_DIR/ChatOS.iconset"
trap 'rm -rf "$TEMP_DIR"' EXIT
mkdir -p "$ICONSET_DIR" "$(dirname "$OUTPUT_ICNS")"

render_icon() {
  local size="$1"
  local file_name="$2"
  sips -s format png -z "$size" "$size" "$SOURCE_PNG" \
    --out "$ICONSET_DIR/$file_name" >/dev/null
}

render_icon 16 icon_16x16.png
render_icon 32 icon_16x16@2x.png
render_icon 32 icon_32x32.png
render_icon 64 icon_32x32@2x.png
render_icon 128 icon_128x128.png
render_icon 256 icon_128x128@2x.png
render_icon 256 icon_256x256.png
render_icon 512 icon_256x256@2x.png
render_icon 512 icon_512x512.png
render_icon 1024 icon_512x512@2x.png

iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_ICNS"
echo "[OK] macOS application icon: $OUTPUT_ICNS"
