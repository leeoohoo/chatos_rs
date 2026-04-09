#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WARN_LINES=800
MAX_LINES=1200
TOP_N=20
FAIL_ON_HIT=0

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --warn-lines)
      shift
      WARN_LINES="${1:-}"
      ;;
    --max-lines)
      shift
      MAX_LINES="${1:-}"
      ;;
    --top)
      shift
      TOP_N="${1:-}"
      ;;
    --fail)
      FAIL_ON_HIT=1
      ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/check-code-structure.sh [--warn-lines <N>] [--max-lines <N>] [--top <N>] [--fail]

Checks:
1) Oversized source files (.rs/.ts/.tsx/.py)
2) Exact duplicate source files by hash

Default thresholds:
- warn-lines: 800
- max-lines: 1200
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

for value in "$WARN_LINES" "$MAX_LINES" "$TOP_N"; do
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    echo "Invalid numeric argument: $value" >&2
    exit 1
  fi
done

if [[ "$WARN_LINES" -gt "$MAX_LINES" ]]; then
  echo "--warn-lines must be <= --max-lines" >&2
  exit 1
fi

if command -v sha1sum >/dev/null 2>&1; then
  hash_file() {
    sha1sum "$1" | awk '{print $1}'
  }
else
  hash_file() {
    shasum "$1" | awk '{print $1}'
  }
fi

line_tmp="$(mktemp)"
hash_tmp="$(mktemp)"
hash_sorted_tmp="$(mktemp)"
dup_report_tmp="$(mktemp)"
dup_summary_tmp="$(mktemp)"

cleanup() {
  rm -f "$line_tmp" "$hash_tmp" "$hash_sorted_tmp" "$dup_report_tmp" "$dup_summary_tmp"
}
trap cleanup EXIT

while IFS= read -r -d '' file; do
  lines="$(wc -l < "$file" | tr -d ' ')"
  rel="${file#$ROOT_DIR/}"
  hash="$(hash_file "$file")"
  printf '%s\t%s\n' "$lines" "$rel" >> "$line_tmp"
  printf '%s\t%s\n' "$hash" "$rel" >> "$hash_tmp"
done < <(
  find "$ROOT_DIR" \
    \( -type d \( -name .git -o -name node_modules -o -name target -o -name target-shared -o -name dist -o -name vendor -o -name logs -o -name docs \) -prune \) \
    -o \( -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.tsx' -o -name '*.py' \) -print0 \)
)

if [[ ! -s "$line_tmp" ]]; then
  echo "No source files matched."
  exit 0
fi

echo "== Large Source Files (Top ${TOP_N}) =="
sort -nr "$line_tmp" | awk -F'\t' -v top="$TOP_N" 'NR <= top {printf "%6s  %s\n", $1, $2}'

warn_count="$(awk -F'\t' -v t="$WARN_LINES" '$1 >= t {c++} END {print c+0}' "$line_tmp")"
hard_count="$(awk -F'\t' -v t="$MAX_LINES" '$1 >= t {c++} END {print c+0}' "$line_tmp")"

echo
echo "Warn threshold (>= ${WARN_LINES} lines): ${warn_count} file(s)"
echo "Hard threshold (>= ${MAX_LINES} lines): ${hard_count} file(s)"

sort "$hash_tmp" > "$hash_sorted_tmp"
awk -F'\t' -v summary_file="$dup_summary_tmp" '
function flush_group(   i) {
  if (count > 1) {
    dup_groups++;
    dup_files += count;
    printf "hash=%s count=%d\n", current_hash, count;
    for (i = 1; i <= count; i++) {
      printf "  %s\n", paths[i];
    }
    printf "\n";
  }
  delete paths;
  count = 0;
}
{
  if (NR == 1) {
    current_hash = $1;
  }
  if ($1 != current_hash) {
    flush_group();
    current_hash = $1;
  }
  count++;
  paths[count] = $2;
}
END {
  flush_group();
  printf "%d\t%d\n", dup_groups + 0, dup_files + 0 > summary_file;
}
' "$hash_sorted_tmp" > "$dup_report_tmp"

dup_groups=0
dup_files=0
if [[ -s "$dup_summary_tmp" ]]; then
  read -r dup_groups dup_files < "$dup_summary_tmp"
fi

echo
echo "== Exact Duplicate Source Files =="
if [[ "$dup_groups" -eq 0 ]]; then
  echo "No exact duplicate source files detected."
else
  cat "$dup_report_tmp"
  echo "Duplicate groups: ${dup_groups}"
  echo "Duplicate files: ${dup_files}"
fi

if [[ "$FAIL_ON_HIT" -eq 1 ]] && { [[ "$hard_count" -gt 0 ]] || [[ "$dup_groups" -gt 0 ]]; }; then
  exit 2
fi
