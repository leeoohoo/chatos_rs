#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MODEL_DIR="$ROOT_DIR/public/assets/models/polyhaven"

download_asset() {
  local asset_id="$1"
  local asset_dir="$MODEL_DIR/$asset_id"
  local metadata_file
  metadata_file="$(mktemp)"

  mkdir -p "$asset_dir"
  curl -L --fail --silent --show-error "https://api.polyhaven.com/files/$asset_id" -o "$metadata_file"

  local gltf_url
  gltf_url="$(jq -r '.gltf["1k"].gltf.url' "$metadata_file")"
  curl -L --fail --silent --show-error "$gltf_url" -o "$asset_dir/$asset_id.gltf"

  while IFS=$'\t' read -r relative_path asset_url; do
    mkdir -p "$asset_dir/$(dirname "$relative_path")"
    curl -L --fail --silent --show-error "$asset_url" -o "$asset_dir/$relative_path"
  done < <(
    jq -r '.gltf["1k"].gltf.include | to_entries[] | [.key, .value.url] | @tsv' "$metadata_file"
  )
}

mkdir -p "$MODEL_DIR"

if [[ "$#" -gt 0 ]]; then
  ASSETS=("$@")
else
  ASSETS=(
    WoodenTable_01
    steel_frame_shelves_01
    desk_lamp_arm_01
    office_notepads
    stationery_supplies
  )
fi

for asset_id in "${ASSETS[@]}"; do
  download_asset "$asset_id"
done
