#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
THRESHOLD_MB=5
FAIL_ON_HIT=0

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --threshold)
      shift
      THRESHOLD_MB="${1:-}"
      ;;
    --fail)
      FAIL_ON_HIT=1
      ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/check-large-files.sh [--threshold <MB>] [--fail]

Scan repository files larger than threshold MB.
Excluded directories: .git, node_modules, target, dist
EOF
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
  shift
done

if ! [[ "$THRESHOLD_MB" =~ ^[0-9]+$ ]]; then
  echo "Invalid threshold MB: $THRESHOLD_MB" >&2
  exit 1
fi

file_size_bytes() {
  local file="$1"
  if stat -f%z "$file" >/dev/null 2>&1; then
    stat -f%z "$file"
  else
    stat -c%s "$file"
  fi
}

human_bytes() {
  local bytes="$1"
  awk -v b="$bytes" 'BEGIN {
    split("B KB MB GB TB", u, " ");
    i=1;
    while (b >= 1024 && i < 5) {
      b /= 1024;
      i++;
    }
    printf("%.1f %s", b, u[i]);
  }'
}

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

while IFS= read -r -d '' file; do
  bytes="$(file_size_bytes "$file")"
  rel="${file#$ROOT_DIR/}"
  printf '%s\t%s\n' "$bytes" "$rel" >> "$tmp_file"
done < <(
  find "$ROOT_DIR" \
    \( -type d \( -name .git -o -name node_modules -o -name target -o -name dist \) -prune \) \
    -o \( -type f -size +"${THRESHOLD_MB}"M -print0 \)
)

if [[ ! -s "$tmp_file" ]]; then
  echo "No files exceed ${THRESHOLD_MB} MB."
  exit 0
fi

echo "Files larger than ${THRESHOLD_MB} MB:"
while IFS=$'\t' read -r bytes rel; do
  printf "%10s  %s\n" "$(human_bytes "$bytes")" "$rel"
done < <(sort -nr "$tmp_file")

if [[ "$FAIL_ON_HIT" -eq 1 ]]; then
  exit 2
fi
