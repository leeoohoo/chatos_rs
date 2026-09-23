#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

matches="$(rg -n \
  'mongodb::|bson::doc|MONGODB_|mongodb://|mongodb\+srv://' \
  --hidden \
  --glob '!.git/**' \
  --glob '!docs/**' \
  --glob '!node_modules/**' \
  --glob '!target/**' \
  --glob '!target-*/**' \
  --glob '!scripts/check-no-runtime-mongodb.sh' \
  --glob '!tools/postgres-user-migration/**' \
  --glob '!plugins/web-design-studio/ui-src/library-runtime/vendor/**' \
  . || true)"

if [[ -n "$matches" ]]; then
  echo "Runtime MongoDB references are forbidden:" >&2
  printf '%s\n' "$matches" >&2
  exit 1
fi

echo "No runtime MongoDB references found."
