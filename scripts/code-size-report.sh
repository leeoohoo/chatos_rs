#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOP_N="${TOP_N:-25}"
WARN_LINES="${WARN_LINES:-700}"
WARN_BYTES="${WARN_BYTES:-40960}"

usage() {
  cat <<'EOF'
Usage: scripts/code-size-report.sh [--top <N>] [--warn-lines <N>] [--warn-kb <N>]

Report source-code file size and line-count hotspots.
The scan excludes generated/build/runtime artifacts, lockfiles, docs, and binary assets.

Environment overrides:
  TOP_N       Number of rows per table. Default: 25
  WARN_LINES  Line-count hotspot threshold. Default: 700
  WARN_BYTES  Byte-size hotspot threshold. Default: 40960
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --top)
      shift
      TOP_N="${1:-}"
      ;;
    --warn-lines)
      shift
      WARN_LINES="${1:-}"
      ;;
    --warn-kb)
      shift
      warn_kb="${1:-}"
      if ! [[ "$warn_kb" =~ ^[0-9]+$ ]]; then
        echo "Invalid --warn-kb value: $warn_kb" >&2
        exit 1
      fi
      WARN_BYTES=$((warn_kb * 1024))
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if ! [[ "$TOP_N" =~ ^[0-9]+$ ]] || (( TOP_N < 1 )); then
  echo "Invalid TOP_N: $TOP_N" >&2
  exit 1
fi
if ! [[ "$WARN_LINES" =~ ^[0-9]+$ ]]; then
  echo "Invalid WARN_LINES: $WARN_LINES" >&2
  exit 1
fi
if ! [[ "$WARN_BYTES" =~ ^[0-9]+$ ]]; then
  echo "Invalid WARN_BYTES: $WARN_BYTES" >&2
  exit 1
fi

tmp_file="$(mktemp)"
size_sorted_file="$(mktemp)"
line_sorted_file="$(mktemp)"
trap 'rm -f "$tmp_file" "$size_sorted_file" "$line_sorted_file"' EXIT

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

is_source_file() {
  local rel="$1"
  case "$rel" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.mjs|*.cjs|*.py|*.sh|*.ps1|*.html|*.css|*.scss|*.sql|*.toml|*.yaml|*.yml)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_excluded_path() {
  local rel="$1"
  case "$rel" in
    .git/*|.github/*|.cache/*|.local/*|.vite/*|.task_runner/*|bundled-tools/*)
      return 0
      ;;
    target/*|target-*/*|*/target/*|*/node_modules/*|*/dist/*|*/build/*|*/coverage/*)
      return 0
      ;;
    docs/*|*/docs/*|*.md|*.lock|package-lock.json|pnpm-lock.yaml|yarn.lock)
      return 0
      ;;
    *.png|*.jpg|*.jpeg|*.gif|*.webp|*.ico|*.pdf|*.zip|*.gz|*.tgz|*.exe|*.dll|*.pdb)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

scan_git_scope() {
  git -C "$ROOT_DIR" ls-files -z --cached --others --exclude-standard
}

scan_fallback_scope() {
  find "$ROOT_DIR" \
    \( -type d \( -name .git -o -name node_modules -o -name target -o -name target-shared -o -name dist -o -name build -o -name coverage -o -name .local -o -name .cache -o -name .vite -o -name bundled-tools \) -prune \) \
    -o \( -type f -print0 \)
}

record_file() {
  local rel="$1"
  local file="$ROOT_DIR/$rel"
  [[ -f "$file" ]] || return 0
  is_source_file "$rel" || return 0
  is_excluded_path "$rel" && return 0

  local bytes
  local lines
  bytes="$(file_size_bytes "$file")"
  lines="$(wc -l < "$file" | tr -d ' ')"
  printf '%s\t%s\t%s\n' "$bytes" "$lines" "$rel" >> "$tmp_file"
}

if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  while IFS= read -r -d '' rel; do
    record_file "$rel"
  done < <(scan_git_scope)
else
  while IFS= read -r -d '' file; do
    rel="${file#$ROOT_DIR/}"
    record_file "$rel"
  done < <(scan_fallback_scope)
fi

echo "# Source Code Size Report"
echo
echo "Root: $ROOT_DIR"
echo "Top rows: $TOP_N"
echo "Line hotspot threshold: $WARN_LINES"
echo "Size hotspot threshold: $(human_bytes "$WARN_BYTES")"
echo

if [[ ! -s "$tmp_file" ]]; then
  echo "No source files found."
  exit 0
fi

total_files="$(wc -l < "$tmp_file" | tr -d ' ')"
total_bytes="$(awk -F '\t' '{sum += $1} END {print sum + 0}' "$tmp_file")"
total_lines="$(awk -F '\t' '{sum += $2} END {print sum + 0}' "$tmp_file")"
echo "Summary: $total_files files, $(human_bytes "$total_bytes"), $total_lines lines"

sort -nr -k1,1 "$tmp_file" > "$size_sorted_file"
sort -nr -k2,2 "$tmp_file" > "$line_sorted_file"

echo
echo "## Top By Size"
printf '%10s  %8s  %s\n' "Size" "Lines" "File"
head -n "$TOP_N" "$size_sorted_file" | while IFS=$'\t' read -r bytes lines rel; do
  printf '%10s  %8s  %s\n' "$(human_bytes "$bytes")" "$lines" "$rel"
done

echo
echo "## Top By Lines"
printf '%10s  %8s  %s\n' "Size" "Lines" "File"
head -n "$TOP_N" "$line_sorted_file" | while IFS=$'\t' read -r bytes lines rel; do
  printf '%10s  %8s  %s\n' "$(human_bytes "$bytes")" "$lines" "$rel"
done

echo
echo "## Hotspots Over Threshold"
hotspot_count=0
while IFS=$'\t' read -r bytes lines rel; do
  if (( bytes >= WARN_BYTES || lines >= WARN_LINES )); then
    if (( hotspot_count == 0 )); then
      printf '%10s  %8s  %s\n' "Size" "Lines" "File"
    fi
    hotspot_count=$((hotspot_count + 1))
    printf '%10s  %8s  %s\n' "$(human_bytes "$bytes")" "$lines" "$rel"
  fi
done < "$line_sorted_file"

if (( hotspot_count == 0 )); then
  echo "No source files exceed configured thresholds."
else
  echo
  echo "Hotspot count: $hotspot_count"
fi
