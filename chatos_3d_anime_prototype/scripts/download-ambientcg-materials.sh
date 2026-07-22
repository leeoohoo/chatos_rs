#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MATERIAL_DIR="$ROOT_DIR/public/assets/materials/ambientcg"

download_material() {
  local asset_id="$1"
  local target_dir="$MATERIAL_DIR/$asset_id"
  local archive_file
  archive_file="$(mktemp)"

  mkdir -p "$target_dir"
  curl -L --fail --silent --show-error "https://ambientcg.com/get?file=${asset_id}_1K-JPG.zip" -o "$archive_file"
  unzip -o -j -q "$archive_file" \
    "*_Color.jpg" \
    "*_NormalGL.jpg" \
    "*_Roughness.jpg" \
    -d "$target_dir"
}

mkdir -p "$MATERIAL_DIR"

if [[ "$#" -gt 0 ]]; then
  ASSETS=("$@")
else
  ASSETS=(WoodFloor051 Plaster001 Fabric036)
fi

for asset_id in "${ASSETS[@]}"; do
  download_material "$asset_id"
done
