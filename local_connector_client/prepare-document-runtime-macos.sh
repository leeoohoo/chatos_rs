#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

DESTINATION_DIR="${1:?usage: prepare-document-runtime-macos.sh DESTINATION_DIR PLATFORM}"
PLATFORM="${2:?usage: prepare-document-runtime-macos.sh DESTINATION_DIR PLATFORM}"
SOURCE_ROOT="${CHATOS_DOCUMENT_RUNTIME_SOURCE:-}"
FONT_URL="${CHATOS_DOCUMENT_FONT_URL:-https://fonts.gstatic.com/s/notosanssc/v40/k3kCo84MPvpLmixcA63oeAL7Iqp5IZJF9bmaG9_FnYw.ttf}"
FONT_SHA256="${CHATOS_DOCUMENT_FONT_SHA256:-450625c8d46ab3df97b7904ded955ec2746d17ec76740cb1e91d1ba63a0f89af}"
FONT_CACHE_ROOT="${CHATOS_DOCUMENT_RUNTIME_CACHE:-$HOME/Library/Caches/chatos-local-connector/document-runtime}"
FONT_LICENSE="$({ cd "$(dirname "${BASH_SOURCE[0]}")" && pwd; })/runtime_assets/fonts/NotoSansSC-OFL.txt"

case "$PLATFORM" in
  macos-arm64|macos-x64) ;;
  *)
    echo "Unsupported document runtime platform: $PLATFORM" >&2
    exit 1
    ;;
esac

if [[ -z "$SOURCE_ROOT" ]]; then
  echo "CHATOS_DOCUMENT_RUNTIME_SOURCE must point to a verified LibreOffice and Poppler runtime root" >&2
  exit 1
fi
if [[ ! -d "$SOURCE_ROOT" || -L "$SOURCE_ROOT" ]]; then
  echo "Document runtime source must be a regular non-symlink directory: $SOURCE_ROOT" >&2
  exit 1
fi

for command_name in curl ditto node shasum; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required document runtime command not found: $command_name" >&2
    exit 1
  fi
done

if [[ -d "$SOURCE_ROOT/libreoffice-headless/libreoffice" ]]; then
  LIBREOFFICE_SOURCE="$SOURCE_ROOT/libreoffice-headless/libreoffice"
  LIBREOFFICE_SOURCE_MANIFEST="$SOURCE_ROOT/libreoffice-headless/manifest.json"
elif [[ -d "$SOURCE_ROOT/libreoffice" ]]; then
  LIBREOFFICE_SOURCE="$SOURCE_ROOT/libreoffice"
  LIBREOFFICE_SOURCE_MANIFEST="$SOURCE_ROOT/libreoffice/manifest.json"
else
  echo "Document runtime source is missing LibreOffice: $SOURCE_ROOT" >&2
  exit 1
fi

if [[ -x "$LIBREOFFICE_SOURCE/LibreOffice.app/Contents/MacOS/soffice" ]]; then
  LIBREOFFICE_APP_NAME="LibreOffice.app"
elif [[ -x "$LIBREOFFICE_SOURCE/LibreOfficeDev.app/Contents/MacOS/soffice" ]]; then
  LIBREOFFICE_APP_NAME="LibreOfficeDev.app"
else
  echo "Document runtime source is missing a supported soffice executable" >&2
  exit 1
fi

if [[ -x "$SOURCE_ROOT/poppler/poppler/bin/pdftoppm" ]]; then
  POPPLER_SOURCE="$SOURCE_ROOT/poppler/poppler"
  POPPLER_SOURCE_MANIFEST="$SOURCE_ROOT/poppler/manifest.json"
elif [[ -x "$SOURCE_ROOT/poppler/bin/pdftoppm" ]]; then
  POPPLER_SOURCE="$SOURCE_ROOT/poppler"
  POPPLER_SOURCE_MANIFEST="$SOURCE_ROOT/poppler/manifest.json"
else
  echo "Document runtime source is missing Poppler pdftoppm" >&2
  exit 1
fi

if [[ -e "$DESTINATION_DIR" ]]; then
  /usr/bin/find "$DESTINATION_DIR" -depth -delete
fi
mkdir -p "$DESTINATION_DIR/libreoffice" "$DESTINATION_DIR/poppler" "$DESTINATION_DIR/fonts" "$FONT_CACHE_ROOT"
ditto "$LIBREOFFICE_SOURCE/$LIBREOFFICE_APP_NAME" "$DESTINATION_DIR/libreoffice/$LIBREOFFICE_APP_NAME"
ditto "$POPPLER_SOURCE" "$DESTINATION_DIR/poppler"

FONT_CACHE_FILE="$FONT_CACHE_ROOT/NotoSansSC-Regular.ttf"
if [[ ! -f "$FONT_CACHE_FILE" || "$(shasum -a 256 "$FONT_CACHE_FILE" | awk '{print $1}')" != "$FONT_SHA256" ]]; then
  rm -f "$FONT_CACHE_FILE" "$FONT_CACHE_FILE.partial"
  echo "[INFO] Downloading Noto Sans SC document fallback font"
  curl --fail --location --retry 3 --output "$FONT_CACHE_FILE.partial" "$FONT_URL"
  if [[ "$(shasum -a 256 "$FONT_CACHE_FILE.partial" | awk '{print $1}')" != "$FONT_SHA256" ]]; then
    echo "Downloaded document fallback font hash does not match the pinned value" >&2
    rm -f "$FONT_CACHE_FILE.partial"
    exit 1
  fi
  mv "$FONT_CACHE_FILE.partial" "$FONT_CACHE_FILE"
fi
cp "$FONT_CACHE_FILE" "$DESTINATION_DIR/fonts/NotoSansSC-Regular.ttf"
cp "$FONT_LICENSE" "$DESTINATION_DIR/fonts/NotoSansSC-OFL.txt"

SOFFICE_RELATIVE="libreoffice/$LIBREOFFICE_APP_NAME/Contents/MacOS/soffice"
PDFTOPPM_RELATIVE="poppler/bin/pdftoppm"
POPPLER_LIBRARY_RELATIVE="poppler/lib"
FONT_DIRECTORY_RELATIVE="fonts"
FONT_RELATIVE="fonts/NotoSansSC-Regular.ttf"
SOFFICE="$DESTINATION_DIR/$SOFFICE_RELATIVE"
PDFTOPPM="$DESTINATION_DIR/$PDFTOPPM_RELATIVE"
POPPLER_LIBRARY="$DESTINATION_DIR/$POPPLER_LIBRARY_RELATIVE"
if [[ ! -x "$SOFFICE" || -L "$SOFFICE" || ! -x "$PDFTOPPM" || -L "$PDFTOPPM" || ! -d "$POPPLER_LIBRARY" || -L "$POPPLER_LIBRARY" ]]; then
  echo "Packaged document runtime is incomplete under $DESTINATION_DIR" >&2
  exit 1
fi

SOFFICE_VERSION="$("$SOFFICE" --version | head -n 1)"
PDFTOPPM_VERSION="$(DYLD_FALLBACK_LIBRARY_PATH="$POPPLER_LIBRARY" "$PDFTOPPM" -v 2>&1 | head -n 1)"
if [[ "$SOFFICE_VERSION" != *"LibreOffice"* || "$PDFTOPPM_VERSION" != *"pdftoppm version"* ]]; then
  echo "Packaged document runtime version probe failed" >&2
  exit 1
fi
SOFFICE_SHA256="$(shasum -a 256 "$SOFFICE" | awk '{print $1}')"
PDFTOPPM_SHA256="$(shasum -a 256 "$PDFTOPPM" | awk '{print $1}')"
RUNTIME_REVISION="${CHATOS_DOCUMENT_RUNTIME_REVISION:-libreoffice-poppler-2026-07-25.1}"

node - "$DESTINATION_DIR/runtime.json" "$PLATFORM" "$RUNTIME_REVISION" \
  "$SOFFICE_RELATIVE" "$SOFFICE_SHA256" "$SOFFICE_VERSION" \
  "$PDFTOPPM_RELATIVE" "$PDFTOPPM_SHA256" "$PDFTOPPM_VERSION" \
  "$POPPLER_LIBRARY_RELATIVE" "$FONT_DIRECTORY_RELATIVE" "$FONT_RELATIVE" "$FONT_SHA256" <<'NODE'
const fs = require('fs');
const [
  manifestPath,
  platform,
  runtimeRevision,
  sofficePath,
  sofficeSha256,
  sofficeVersion,
  pdftoppmPath,
  pdftoppmSha256,
  pdftoppmVersion,
  popplerLibraryDir,
  fontDirectory,
  fontPath,
  fontSha256,
] = process.argv.slice(2);
const manifest = {
  schema_version: 1,
  runtime_revision: runtimeRevision,
  platform,
  soffice: { path: sofficePath, sha256: sofficeSha256, version: sofficeVersion },
  pdftoppm: { path: pdftoppmPath, sha256: pdftoppmSha256, version: pdftoppmVersion },
  poppler_library_dir: popplerLibraryDir,
  font_directory: fontDirectory,
  fonts: [{ path: fontPath, sha256: fontSha256 }],
};
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644 });
NODE

if [[ -f "$LIBREOFFICE_SOURCE_MANIFEST" ]]; then
  cp "$LIBREOFFICE_SOURCE_MANIFEST" "$DESTINATION_DIR/libreoffice-source-manifest.json"
fi
if [[ -f "$POPPLER_SOURCE_MANIFEST" ]]; then
  cp "$POPPLER_SOURCE_MANIFEST" "$DESTINATION_DIR/poppler-source-manifest.json"
fi

echo "[OK] Document runtime: $SOFFICE_VERSION"
echo "[OK] Document runtime: $PDFTOPPM_VERSION"
echo "[OK] Document runtime manifest: $DESTINATION_DIR/runtime.json"
