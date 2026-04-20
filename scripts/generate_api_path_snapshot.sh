#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MAIN_API_DIR="$ROOT_DIR/chat_app_server_rs/src/api"
MEMORY_API_DIR="$ROOT_DIR/memory_server/backend/src/api"

extract_endpoints_from_file() {
  local file="$1"
  awk '
    function count_char(s, c,   i, n) {
      n = 0
      for (i = 1; i <= length(s); i++) {
        if (substr(s, i, 1) == c) n++
      }
      return n
    }

    function has_method(block, method) {
      pattern = "(^|[^A-Za-z_])" method "[[:space:]]*\\("
      return block ~ pattern
    }

    function flush_block(block,   path, methods, method_count) {
      path = block
      sub(/^.*\.route\([[:space:]]*"/, "", path)
      if (path == block) {
        return
      }
      sub(/".*$/, "", path)

      method_count = 0
      delete methods

      if (has_method(block, "get")) {
        method_count++
        methods[method_count] = "GET"
      }
      if (has_method(block, "post")) {
        method_count++
        methods[method_count] = "POST"
      }
      if (has_method(block, "put")) {
        method_count++
        methods[method_count] = "PUT"
      }
      if (has_method(block, "patch")) {
        method_count++
        methods[method_count] = "PATCH"
      }
      if (has_method(block, "delete")) {
        method_count++
        methods[method_count] = "DELETE"
      }

      if (method_count == 0) {
        method_count++
        methods[method_count] = "UNKNOWN"
      }

      for (i = 1; i <= method_count; i++) {
        print methods[i] " " path
      }
    }

    {
      line = $0
      if (!in_route && line ~ /\.route\(/) {
        in_route = 1
        block = line
        paren_balance = count_char(line, "(") - count_char(line, ")")
        if (paren_balance <= 0) {
          flush_block(block)
          in_route = 0
          block = ""
          paren_balance = 0
        }
        next
      }

      if (in_route) {
        block = block " " line
        paren_balance += count_char(line, "(") - count_char(line, ")")
        if (paren_balance <= 0) {
          flush_block(block)
          in_route = 0
          block = ""
          paren_balance = 0
        }
      }
    }

    END {
      if (in_route) {
        flush_block(block)
      }
    }
  ' "$file"
}

collect_endpoints() {
  local dir="$1"
  find "$dir" -type f -name '*.rs' | sort | while IFS= read -r file; do
    extract_endpoints_from_file "$file"
  done | sed '/^[[:space:]]*$/d' | sort -u
}

count_lines() {
  local content="$1"
  printf "%s\n" "$content" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' '
}

main_endpoints="$(collect_endpoints "$MAIN_API_DIR")"
memory_endpoints="$(collect_endpoints "$MEMORY_API_DIR")"

main_count="$(count_lines "$main_endpoints")"
memory_count="$(count_lines "$memory_endpoints")"

cat <<EOF
# API Path Baseline

main_backend_endpoint_count=${main_count}
memory_backend_endpoint_count=${memory_count}
total_endpoint_count=$((main_count + memory_count))

## chat_app_server_rs endpoints (method + path)
${main_endpoints}

## memory_server endpoints (method + path)
${memory_endpoints}
EOF
