#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

default_extension_id=jooaepjckiofmpldinopgdgddcoaofil
if [ -z "${CHATOS_BROWSER_EXTENSION_ID:-}" ]; then
  CHATOS_BROWSER_EXTENSION_ID=$default_extension_id
  export CHATOS_BROWSER_EXTENSION_ID
fi

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

cargo_target_dir=$(cargo metadata --format-version 1 --no-deps | jq -er '.target_directory')
source_binary="$cargo_target_dir/$cargo_profile_dir/chatos-browser-cdp"
if [ ! -f "$source_binary" ]; then
  echo "Built Browser CDP binary was not found: $source_binary" >&2
  exit 1
fi

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
cp "$source_binary" "$destination"
chmod +x "$destination"

case "$(uname -s)" in
  Darwin)
    codesign --force --sign - "$destination"
    codesign --verify --strict --verbose=2 "$destination"
    ;;
esac

echo "Staged $destination ($build_profile, local development)"
