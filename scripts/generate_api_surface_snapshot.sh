#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MAIN_API_DIR="chat_app_server_rs/src/api"

normalize_routes() {
  local target_dir="$1"
  local label="$2"

  echo "## ${label}"
  if [[ ! -d "$target_dir" ]]; then
    echo "(directory missing)"
    echo
    return
  fi
  grep -R -n --include='*.rs' '\.route(' "$target_dir" \
    | sed -E 's/:([0-9]+):/:/' \
    | sed -E 's/[[:space:]]+/ /g' \
    | sort
  echo
}

count_routes() {
  local target_dir="$1"
  if [[ ! -d "$target_dir" ]]; then
    echo "0"
    return
  fi
  grep -R --include='*.rs' -o '\.route(' "$target_dir" | wc -l | tr -d ' '
}

pushd "$ROOT_DIR" >/dev/null

main_count="$(count_routes "$MAIN_API_DIR")"

cat <<EOF
# API Surface Baseline

main_backend_route_count=${main_count}
total_route_count=${main_count}

EOF

normalize_routes "$MAIN_API_DIR" "chat_app_server_rs/src/api (.route lines)"

popd >/dev/null
