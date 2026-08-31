#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

build_profile=${CHATOS_BROWSER_BUILD_PROFILE:-release}
case "$build_profile" in
  debug)
    cargo build -p browser-cdp-cli
    cargo_profile_dir=debug
    ;;
  release)
    cargo build --release -p browser-cdp-cli
    cargo_profile_dir=release
    ;;
  *)
    echo "Unsupported CHATOS_BROWSER_BUILD_PROFILE: $build_profile" >&2
    exit 1
    ;;
esac

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) destination="npm/dist/macos/arm64/chatos-browser-cdp" ;;
  Darwin-x86_64) destination="npm/dist/macos/x64/chatos-browser-cdp" ;;
  Linux-aarch64|Linux-arm64) destination="npm/dist/linux/arm64/chatos-browser-cdp" ;;
  Linux-x86_64) destination="npm/dist/linux/x64/chatos-browser-cdp" ;;
  *)
    echo "Unsupported local staging platform: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$destination")"
cp "target/$cargo_profile_dir/chatos-browser-cdp" "$destination"
chmod +x "$destination"

case "$(uname -s)" in
  Darwin)
    codesign --force --sign - "$destination"
    codesign --verify --strict --verbose=2 "$destination"
    ;;
esac

echo "Staged $destination ($build_profile, local development)"
