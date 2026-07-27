#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HTTP_CONFIG="$SCRIPT_DIR/nginx/jgoool-http.conf"
HTTPS_CONFIG="$SCRIPT_DIR/nginx/jgoool-https.conf"
PARENT_ORIGIN="${CHATOS_PLUGIN_UI_PARENT_ORIGIN:-https://app.jgoool.com}"
RESOURCE_ORIGIN="${CHATOS_PLUGIN_UI_RESOURCE_ORIGIN:-https://plugin-ui.jgoool.com}"
LIVE=0

usage() {
  cat <<'EOF'
Usage: verify-plugin-ui-origin.sh [--live] [--parent-origin URL] [--resource-origin URL]

Without --live, validates the production origin contract and checked-in Nginx isolation block.
With --live, also verifies DNS, certificate trust, and public reverse-proxy isolation using HEAD.
The script never starts Nginx, ChatOS, Docker, or any listener.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --live)
      LIVE=1
      shift
      ;;
    --parent-origin)
      PARENT_ORIGIN="${2:?missing parent origin}"
      shift 2
      ;;
    --resource-origin)
      RESOURCE_ORIGIN="${2:?missing resource origin}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[ERROR] unsupported argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

CHATOS_ENV=production \
CHATOS_PLUGIN_UI_PARENT_ORIGIN="$PARENT_ORIGIN" \
CHATOS_PLUGIN_UI_RESOURCE_ORIGIN="$RESOURCE_ORIGIN" \
  "$SCRIPT_DIR/deploy.sh" validate-plugin-ui-origin >/dev/null

expect_origin_rejected() {
  local label="$1"
  local parent="$2"
  local resource="$3"
  if CHATOS_ENV=production \
    CHATOS_DOCKER_ENV_FILE=/dev/null \
    CHATOS_PLUGIN_UI_PARENT_ORIGIN="$parent" \
    CHATOS_PLUGIN_UI_RESOURCE_ORIGIN="$resource" \
      "$SCRIPT_DIR/deploy.sh" validate-plugin-ui-origin >/dev/null 2>&1; then
    echo "[ERROR] invalid Plugin UI origin case was accepted: $label" >&2
    exit 1
  fi
}

expect_origin_rejected "missing resource origin" "https://app.example.com" ""
expect_origin_rejected "non-HTTPS resource origin" "https://app.example.com" "http://plugin-ui.example.com"
expect_origin_rejected "same origins" "https://app.example.com" "https://app.example.com"
expect_origin_rejected "origin path" "https://app.example.com/root" "https://plugin-ui.example.com"
expect_origin_rejected "invalid port" "https://app.example.com" "https://plugin-ui.example.com:65536"
expect_origin_rejected "uppercase authority" "https://APP.example.com" "https://plugin-ui.example.com"
expect_origin_rejected "default HTTPS port" "https://app.example.com:443" "https://plugin-ui.example.com"

resource_block="$({
  sed -n \
    '/^# CHATOS_PLUGIN_UI_RESOURCE_SERVER_BEGIN$/,/^# CHATOS_PLUGIN_UI_RESOURCE_SERVER_END$/p' \
    "$HTTPS_CONFIG"
})"

require_block_text() {
  local expected="$1"
  if ! grep -Fq "$expected" <<< "$resource_block"; then
    echo "[ERROR] Plugin UI Nginx block is missing: $expected" >&2
    exit 1
  fi
}

require_block_text 'server_name plugin-ui.jgoool.com;'
require_block_text 'location ^~ /api/plugin-ui/workbench/ {'
require_block_text 'limit_except GET {'
require_block_text 'proxy_pass_request_body off;'
require_block_text 'proxy_hide_header Access-Control-Allow-Origin;'
require_block_text 'proxy_pass http://127.0.0.1:3997;'
require_block_text 'return 404;'

proxy_pass_count="$(grep -c '^[[:space:]]*proxy_pass[[:space:]]' <<< "$resource_block")"
if [[ "$proxy_pass_count" != "1" ]]; then
  echo "[ERROR] Plugin UI resource server must contain exactly one proxy_pass" >&2
  exit 1
fi
location_count="$(grep -c '^[[:space:]]*location[[:space:]]' <<< "$resource_block")"
if [[ "$location_count" != "2" ]]; then
  echo "[ERROR] Plugin UI resource server must contain exactly two locations" >&2
  exit 1
fi
if grep -Fq 'include snippets/chatos-proxy-headers.conf;' <<< "$resource_block"; then
  echo "[ERROR] Plugin UI resource server must not inherit WebSocket-capable shared proxy headers" >&2
  exit 1
fi
if ! grep -Fq 'server_name plugin-ui.jgoool.com;' "$HTTP_CONFIG"; then
  echo "[ERROR] Plugin UI resource hostname is missing from the HTTP ACME/redirect config" >&2
  exit 1
fi

validate_nginx_syntax() {
  local config_path="$1"
  local needs_tls="$2"
  (
    local temp_root
    temp_root="$(mktemp -d /tmp/chatos-plugin-ui-nginx.XXXXXX)"
    trap 'find "$temp_root" -depth -delete' EXIT
    mkdir -p "$temp_root/conf/snippets" "$temp_root/logs"
    cp "$config_path" "$temp_root/conf/site.conf"
    cp "$SCRIPT_DIR/nginx/chatos-letsencrypt.conf" "$temp_root/conf/snippets/chatos-letsencrypt.conf"
    cp "$SCRIPT_DIR/nginx/chatos-proxy-headers.conf" "$temp_root/conf/snippets/chatos-proxy-headers.conf"
    if [[ "$needs_tls" == "true" ]]; then
      openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj '/CN=jgoool.com' \
        -keyout "$temp_root/key.pem" -out "$temp_root/cert.pem" >/dev/null 2>&1
      sed \
        -e "s#/etc/letsencrypt/live/jgoool.com/fullchain.pem#$temp_root/cert.pem#" \
        -e "s#/etc/letsencrypt/live/jgoool.com/privkey.pem#$temp_root/key.pem#" \
        "$SCRIPT_DIR/nginx/chatos-ssl.conf" > "$temp_root/conf/snippets/chatos-ssl.conf"
    fi
    printf '%s\n' \
      'pid logs/nginx.pid;' \
      'error_log logs/error.log;' \
      'events {}' \
      'http {' \
      '  include site.conf;' \
      '}' > "$temp_root/conf/nginx.conf"
    nginx -t -p "$temp_root/" -c conf/nginx.conf >/dev/null
  )
}

if command -v nginx >/dev/null 2>&1 && command -v openssl >/dev/null 2>&1; then
  validate_nginx_syntax "$HTTP_CONFIG" false
  validate_nginx_syntax "$HTTPS_CONFIG" true
  echo "[OK] Nginx parsed the HTTP and HTTPS production configs without binding ports."
else
  echo "[INFO] nginx or openssl unavailable; skipped parser-level Nginx validation."
fi

echo "[OK] Plugin UI origin and checked-in Nginx isolation contract are valid."

if (( LIVE == 0 )); then
  exit 0
fi

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] missing command for live verification: $1" >&2
    exit 1
  fi
}

origin_host() {
  local authority="${1#https://}"
  if [[ "$authority" == \[* ]]; then
    authority="${authority:1}"
    printf '%s' "${authority%%]*}"
    return
  fi
  printf '%s' "${authority%%:*}"
}

resolve_host() {
  local host="$1"
  if command -v dscacheutil >/dev/null 2>&1; then
    dscacheutil -q host -a name "$host" | grep -Eq '(^|[[:space:]])ip_address:'
  elif command -v getent >/dev/null 2>&1; then
    getent ahosts "$host" >/dev/null
  elif command -v dig >/dev/null 2>&1; then
    [[ -n "$(dig +short "$host")" ]]
  else
    echo "[ERROR] live verification requires dscacheutil, getent, or dig" >&2
    return 1
  fi
}

head_status() {
  curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --head --max-time 20 "$1"
}

need_cmd curl
parent_host="$(origin_host "$PARENT_ORIGIN")"
resource_host="$(origin_host "$RESOURCE_ORIGIN")"
resolve_host "$parent_host" || {
  echo "[ERROR] parent origin DNS did not resolve: $parent_host" >&2
  exit 1
}
resolve_host "$resource_host" || {
  echo "[ERROR] resource origin DNS did not resolve: $resource_host" >&2
  exit 1
}

invalid_path="/api/plugin-ui/workbench/pui_invalid/ui/index.html"
parent_status="$(head_status "${PARENT_ORIGIN}${invalid_path}")"
resource_root_status="$(head_status "${RESOURCE_ORIGIN}/")"
resource_path_status="$(head_status "${RESOURCE_ORIGIN}${invalid_path}")"

if [[ "$parent_status" != "404" ]]; then
  echo "[ERROR] parent origin must reject Plugin UI resource paths with 404; got $parent_status" >&2
  exit 1
fi
if [[ "$resource_root_status" != "404" ]]; then
  echo "[ERROR] resource origin root must return 404; got $resource_root_status" >&2
  exit 1
fi
if [[ "$resource_path_status" != "404" ]]; then
  echo "[ERROR] invalid resource session path must return 404; got $resource_path_status" >&2
  exit 1
fi

echo "[OK] DNS, TLS certificate trust, and public reverse-proxy isolation checks passed."
