#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

BASE_URL="${1:-${OFFICIAL_WEBSITE_SMOKE_BASE_URL:-http://127.0.0.1:${OFFICIAL_WEBSITE_PORT:-39250}}}"
BASE_URL="${BASE_URL%/}"

if ! command -v curl >/dev/null 2>&1; then
  echo "[ERROR] curl is required" >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "[ERROR] node is required" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fetch() {
  local path="$1"
  local output="$2"
  curl -fsS --max-time 5 "$BASE_URL$path" -o "$output"
}

echo "[INFO] smoke official website: $BASE_URL"

health_file="$tmp_dir/health.txt"
fetch "/health" "$health_file"
if [[ "$(cat "$health_file")" != "ok" ]]; then
  echo "[ERROR] /health did not return ok" >&2
  exit 1
fi

headers_file="$tmp_dir/index.headers"
curl -fsSI --max-time 5 "$BASE_URL/" -o "$headers_file"
if ! grep -qi '^content-type: text/html' "$headers_file"; then
  echo "[ERROR] / did not return text/html" >&2
  cat "$headers_file" >&2
  exit 1
fi

manifest_file="$tmp_dir/manifest.json"
fetch "/api/site/manifest" "$manifest_file"
node - "$manifest_file" <<'NODE'
const fs = require('fs');
const path = process.argv[2];
const manifest = JSON.parse(fs.readFileSync(path, 'utf8'));
if (manifest.product_name !== 'Chatos RS') {
  throw new Error(`unexpected product_name: ${manifest.product_name}`);
}
if (!manifest.app_url || manifest.registration_enabled !== true) {
  throw new Error('manifest public app/registration configuration is missing');
}
if (!Array.isArray(manifest.services) || manifest.services.length < 6) {
  throw new Error('manifest.services is missing expected service entries');
}
if (!Array.isArray(manifest.showcase_images) || manifest.showcase_images.length < 5) {
  throw new Error('manifest.showcase_images is missing expected screenshots');
}
NODE

downloads_file="$tmp_dir/downloads.json"
fetch "/api/site/downloads" "$downloads_file"
node - "$downloads_file" <<'NODE'
const fs = require('fs');
const path = process.argv[2];
const payload = JSON.parse(fs.readFileSync(path, 'utf8'));
if (typeof payload.storage_configured !== 'boolean' || typeof payload.available !== 'boolean') {
  throw new Error('download catalog flags are missing');
}
if (!payload.message) {
  throw new Error('download catalog message is missing');
}
NODE

status_file="$tmp_dir/status.json"
fetch "/api/site/status" "$status_file"
node - "$status_file" <<'NODE'
const fs = require('fs');
const path = process.argv[2];
const payload = JSON.parse(fs.readFileSync(path, 'utf8'));
const allowed = new Set(['online', 'degraded', 'offline']);
if (!Number.isFinite(Number(payload.checked_at_ms))) {
  throw new Error('status.checked_at_ms is missing');
}
if (payload.live_status_enabled === false) {
  if (!Array.isArray(payload.services) || payload.services.length !== 0) {
    throw new Error('disabled live status should not expose service entries');
  }
  if (!payload.detail) {
    throw new Error('disabled live status should include detail');
  }
  process.exit(0);
}
if (payload.live_status_enabled !== true) {
  throw new Error('status.live_status_enabled is missing');
}
if (!Array.isArray(payload.services) || payload.services.length < 6) {
  throw new Error('status.services is missing expected service entries');
}
for (const service of payload.services) {
  if (!service.name || !allowed.has(service.state)) {
    throw new Error(`invalid service status entry: ${JSON.stringify(service)}`);
  }
}
NODE

robots_file="$tmp_dir/robots.txt"
fetch "/robots.txt" "$robots_file"
if ! grep -Eq '^Sitemap: https?://.*/sitemap\.xml$' "$robots_file"; then
  echo "[ERROR] robots.txt does not contain an absolute sitemap URL" >&2
  cat "$robots_file" >&2
  exit 1
fi

sitemap_file="$tmp_dir/sitemap.xml"
fetch "/sitemap.xml" "$sitemap_file"
if ! grep -q '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' "$sitemap_file" ||
  ! grep -Eq '<loc>https?://.*/</loc>' "$sitemap_file"; then
  echo "[ERROR] sitemap.xml does not contain expected urlset/root URL" >&2
  cat "$sitemap_file" >&2
  exit 1
fi

echo "[OK] official website live smoke passed"
