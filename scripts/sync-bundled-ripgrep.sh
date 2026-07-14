#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RIPGREP_VERSION="${RIPGREP_VERSION:-15.1.0}"
GITHUB_RELEASE_BASE="https://github.com/BurntSushi/ripgrep/releases/download"

usage() {
  cat <<'USAGE'
Usage:
  scripts/sync-bundled-ripgrep.sh
  scripts/sync-bundled-ripgrep.sh --all
  scripts/sync-bundled-ripgrep.sh --platform linux-x64

Options:
  --all                 Download and install all supported platform binaries.
  --platform PLATFORM   Download and install one platform binary.
  --version VERSION     Override ripgrep version. Defaults to RIPGREP_VERSION or 15.1.0.
  -h, --help            Show this help.

Environment:
  RG_SOURCE             Copy this local rg binary for the current platform.
  RIPGREP_VERSION       Version used for downloads when --version is not passed.
USAGE
}

platform_dir() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os:$arch" in
    Darwin:arm64) printf '%s\n' "macos-arm64" ;;
    Darwin:x86_64) printf '%s\n' "macos-x64" ;;
    Linux:aarch64 | Linux:arm64) printf '%s\n' "linux-arm64" ;;
    Linux:x86_64 | Linux:amd64) printf '%s\n' "linux-x64" ;;
    MINGW*:x86_64 | MSYS*:x86_64 | CYGWIN*:x86_64) printf '%s\n' "windows-x64" ;;
    MINGW*:aarch64 | MSYS*:aarch64 | CYGWIN*:aarch64) printf '%s\n' "windows-arm64" ;;
    *)
      echo "[ERROR] unsupported platform: $os $arch" >&2
      return 1
      ;;
  esac
}

resolve_rg_source() {
  if [[ -n "${RG_SOURCE:-}" ]]; then
    printf '%s\n' "$RG_SOURCE"
    return
  fi

  if [[ -x "/Applications/Codex.app/Contents/Resources/rg" ]]; then
    printf '%s\n' "/Applications/Codex.app/Contents/Resources/rg"
    return
  fi

  if command -v rg >/dev/null 2>&1; then
    command -v rg
    return
  fi

  echo "[ERROR] rg not found. Set RG_SOURCE=/path/to/rg and retry." >&2
  return 1
}

archive_candidates_for_platform() {
  local platform="$1"

  case "$platform" in
    macos-arm64) printf '%s\n' "aarch64-apple-darwin.tar.gz" ;;
    macos-x64) printf '%s\n' "x86_64-apple-darwin.tar.gz" ;;
    linux-arm64) printf '%s\n' "aarch64-unknown-linux-gnu.tar.gz" ;;
    linux-x64)
      printf '%s\n' "x86_64-unknown-linux-musl.tar.gz"
      printf '%s\n' "x86_64-unknown-linux-gnu.tar.gz"
      ;;
    windows-arm64) printf '%s\n' "aarch64-pc-windows-msvc.zip" ;;
    windows-x64) printf '%s\n' "x86_64-pc-windows-msvc.zip" ;;
    *)
      echo "[ERROR] unsupported bundled platform: $platform" >&2
      return 1
      ;;
  esac
}

platform_bin_name() {
  case "$1" in
    windows-*) printf '%s\n' "rg.exe" ;;
    *) printf '%s\n' "rg" ;;
  esac
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print tolower($1)}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print tolower($1)}'
  else
    echo "[ERROR] sha256sum or shasum is required" >&2
    return 1
  fi
}

refresh_sha256_manifest() {
  local manifest="$ROOT_DIR/bundled-tools/SHA256SUMS"
  local tmp_file
  tmp_file="$(mktemp)"
  (
    cd "$ROOT_DIR"
    while IFS= read -r tool; do
      printf '%s  %s\n' "$(sha256_file "$tool")" "$tool" >> "$tmp_file"
    done < <(find bundled-tools -mindepth 2 -maxdepth 2 -type f \( -name rg -o -name rg.exe \) | sort)
  )
  mv "$tmp_file" "$manifest"
  echo "[INFO] updated $manifest"
}

install_binary() {
  local platform="$1"
  local src="$2"
  local bin_name dest_dir dest

  bin_name="$(platform_bin_name "$platform")"
  dest_dir="$ROOT_DIR/bundled-tools/$platform"
  dest="$dest_dir/$bin_name"
  mkdir -p "$dest_dir"
  install -m 0755 "$src" "$dest"

  if [[ "$platform" == "$(platform_dir)" ]]; then
    "$dest" --version >"$dest_dir/VERSION"
    echo "[INFO] bundled $("$dest" --version | head -n 1) at $dest"
  else
    printf 'ripgrep %s\n' "$RIPGREP_VERSION" >"$dest_dir/VERSION"
    echo "[INFO] bundled ripgrep $RIPGREP_VERSION for $platform at $dest"
  fi
  refresh_sha256_manifest
}

download_platform() {
  local platform="$1"
  local tmp_dir archive extract_dir candidate archive_name url bin_name found

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir:-}"' RETURN
  extract_dir="$tmp_dir/extract"
  mkdir -p "$extract_dir"
  bin_name="$(platform_bin_name "$platform")"

  while IFS= read -r candidate; do
    [[ -n "$candidate" ]] || continue
    archive_name="ripgrep-$RIPGREP_VERSION-$candidate"
    archive="$tmp_dir/$archive_name"
    url="$GITHUB_RELEASE_BASE/$RIPGREP_VERSION/$archive_name"

    echo "[INFO] downloading $url"
    if ! curl -fL --retry 3 --retry-delay 2 -o "$archive" "$url"; then
      echo "[WARN] failed to download $archive_name" >&2
      continue
    fi

    rm -rf "$extract_dir"
    mkdir -p "$extract_dir"
    case "$archive_name" in
      *.zip)
        unzip -q "$archive" -d "$extract_dir"
        ;;
      *.tar.gz)
        tar -xzf "$archive" -C "$extract_dir"
        ;;
      *)
        echo "[WARN] unsupported archive format: $archive_name" >&2
        continue
        ;;
    esac

    found="$(find "$extract_dir" -type f -name "$bin_name" | head -n 1)"
    if [[ -z "$found" ]]; then
      echo "[WARN] $bin_name not found in $archive_name" >&2
      continue
    fi

    install_binary "$platform" "$found"
    rm -rf "$tmp_dir"
    trap - RETURN
    return
  done < <(archive_candidates_for_platform "$platform")

  rm -rf "$tmp_dir"
  trap - RETURN
  echo "[ERROR] unable to download a ripgrep binary for $platform" >&2
  return 1
}

sync_current_platform() {
  local platform src
  platform="$(platform_dir)"
  src="$(resolve_rg_source)"

  if [[ ! -x "$src" ]]; then
    echo "[ERROR] rg source is not executable: $src" >&2
    exit 1
  fi

  install_binary "$platform" "$src"
}

sync_all_platforms() {
  local platform failures=0

  for platform in \
    macos-arm64 \
    macos-x64 \
    linux-arm64 \
    linux-x64 \
    windows-arm64 \
    windows-x64
  do
    if [[ "$platform" == "$(platform_dir)" ]]; then
      sync_current_platform || failures=$((failures + 1))
    else
      download_platform "$platform" || failures=$((failures + 1))
    fi
  done

  if [[ "$failures" -gt 0 ]]; then
    echo "[ERROR] failed to bundle $failures platform(s)" >&2
    return 1
  fi
}

main() {
  local mode="current" requested_platform=""

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --all)
        mode="all"
        shift
        ;;
      --platform)
        mode="platform"
        requested_platform="${2:-}"
        if [[ -z "$requested_platform" ]]; then
          echo "[ERROR] --platform requires a value" >&2
          exit 1
        fi
        shift 2
        ;;
      --version)
        RIPGREP_VERSION="${2:-}"
        if [[ -z "$RIPGREP_VERSION" ]]; then
          echo "[ERROR] --version requires a value" >&2
          exit 1
        fi
        shift 2
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        echo "[ERROR] unknown argument: $1" >&2
        usage >&2
        exit 1
        ;;
    esac
  done

  case "$mode" in
    current) sync_current_platform ;;
    all) sync_all_platforms ;;
    platform) download_platform "$requested_platform" ;;
  esac
}

main "$@"
