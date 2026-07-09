#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


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

Scan Git-relevant repository files larger than threshold MB.
The default scope is tracked files plus untracked files that are not ignored.
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

is_allowed_large_file() {
  local rel="$1"
  case "$rel" in
    bundled-tools/*/rg|bundled-tools/*/rg.exe)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

scan_git_scope() {
  git -C "$ROOT_DIR" ls-files -z --cached --others --exclude-standard
}

scan_fallback_scope() {
  find "$ROOT_DIR" \
    \( -type d \( -name .git -o -name node_modules -o -name target -o -name target-shared -o -name dist -o -name .local -o -name .cache \) -prune \) \
    -o \( -type f -print0 \)
}

if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  while IFS= read -r -d '' rel; do
    file="$ROOT_DIR/$rel"
    [[ -f "$file" ]] || continue
    if is_allowed_large_file "$rel"; then
      continue
    fi
    bytes="$(file_size_bytes "$file")"
    if (( bytes > THRESHOLD_MB * 1024 * 1024 )); then
      printf '%s\t%s\n' "$bytes" "$rel" >> "$tmp_file"
    fi
  done < <(scan_git_scope)
else
  while IFS= read -r -d '' file; do
    bytes="$(file_size_bytes "$file")"
    rel="${file#$ROOT_DIR/}"
    if is_allowed_large_file "$rel"; then
      continue
    fi
    if (( bytes > THRESHOLD_MB * 1024 * 1024 )); then
      printf '%s\t%s\n' "$bytes" "$rel" >> "$tmp_file"
    fi
  done < <(scan_fallback_scope)
fi

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
