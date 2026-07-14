#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT_DIR/bundled-tools/SHA256SUMS"

usage() {
  cat <<'EOF'
Usage: scripts/check-bundled-tools-integrity.sh

Verify that tracked bundled tool binaries match bundled-tools/SHA256SUMS and
that every bundled rg binary has a manifest entry.
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    echo "Unknown argument: $1" >&2
    usage >&2
    exit 1
    ;;
esac

if [[ ! -f "$MANIFEST" ]]; then
  echo "Bundled tools manifest not found: $MANIFEST" >&2
  exit 1
fi

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print tolower($1)}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print tolower($1)}'
  else
    echo "sha256sum or shasum is required" >&2
    return 1
  fi
}

tmp_expected="$(mktemp)"
tmp_actual="$(mktemp)"
trap 'rm -f "$tmp_expected" "$tmp_actual"' EXIT

cd "$ROOT_DIR"

find bundled-tools -mindepth 2 -maxdepth 2 -type f \( -name rg -o -name rg.exe \) \
  | sort > "$tmp_actual"

while read -r checksum rel extra; do
  [[ -n "${checksum:-}" && "${checksum:0:1}" != "#" ]] || continue
  if [[ -n "${extra:-}" ]]; then
    echo "Invalid manifest line with extra fields: $checksum $rel $extra" >&2
    exit 1
  fi
  if [[ ! "$checksum" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "Invalid SHA256 checksum for $rel: $checksum" >&2
    exit 1
  fi
  case "$rel" in
    bundled-tools/*/rg|bundled-tools/*/rg.exe)
      ;;
    *)
      echo "Unexpected bundled tool manifest path: $rel" >&2
      exit 1
      ;;
  esac
  if [[ ! -f "$rel" ]]; then
    echo "Manifest references missing bundled tool: $rel" >&2
    exit 1
  fi
  actual="$(sha256_file "$rel")"
  if [[ "$actual" != "${checksum,,}" ]]; then
    echo "Bundled tool checksum mismatch: $rel" >&2
    echo "  expected: ${checksum,,}" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
  printf '%s\n' "$rel" >> "$tmp_expected"
done < "$MANIFEST"

sort -u "$tmp_expected" -o "$tmp_expected"

if ! diff -u "$tmp_expected" "$tmp_actual"; then
  echo "Bundled tools manifest does not match actual rg binaries." >&2
  exit 1
fi

echo "Bundled tools integrity check passed."
