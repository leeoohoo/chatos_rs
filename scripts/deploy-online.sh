#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEPLOY_SERVER="${CHATOS_DEPLOY_SERVER:-root@8.155.171.124}"
REMOTE_DEPLOY_ROOT="${CHATOS_DEPLOY_ROOT:-/opt/chatos-deploy}"
ADMIN_AUTH_BASE_URL="${CHATOS_ADMIN_AUTH_BASE_URL:-https://admin.jgoool.com/api/admin/user-service}"
PLUGIN_API_BASE_URL="${CHATOS_PLUGIN_API_BASE_URL:-https://admin.jgoool.com/api/admin/plugin-management}"
ADMIN_USERNAME="${CHATOS_DEPLOY_ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${CHATOS_DEPLOY_ADMIN_PASSWORD:-}"
DEPLOY_TMP=""

BACKEND_SERVICES=(
  configuration-center-backend
  user-service-backend
  memory-engine-backend
  project-management-backend
  plugin-management-backend
  local-connector-service-backend
  mcp-management-service-backend
  task-runner-backend
  chatos-backend
  official-website-backend
)

FRONTEND_SERVICES=(
  admin-console-frontend
  official-website-frontend
)

cleanup() {
  if [[ -n "$DEPLOY_TMP" && -d "$DEPLOY_TMP" ]]; then
    rm -rf -- "$DEPLOY_TMP"
  fi
}
trap cleanup EXIT

usage() {
  cat <<'EOF'
Usage:
  scripts/deploy-online.sh                    # interactive menu
  scripts/deploy-online.sh all                # cloud services + all three Plugins
  scripts/deploy-online.sh cloud              # all cloud services
  scripts/deploy-online.sh cloud-backends     # all backend services
  scripts/deploy-online.sh cloud-frontends    # all frontend services
  scripts/deploy-online.sh gateway            # APISIX + public Nginx configuration only
  scripts/deploy-online.sh service NAME [...] # selected cloud services
  scripts/deploy-online.sh plugin all
  scripts/deploy-online.sh plugin browser
  scripts/deploy-online.sh plugin computer-use
  scripts/deploy-online.sh plugin document
  scripts/deploy-online.sh plugin browser computer-use
  scripts/deploy-online.sh client mac
  scripts/deploy-online.sh client windows
  scripts/deploy-online.sh status
  scripts/deploy-online.sh logs [SERVICE]
  scripts/deploy-online.sh list

Environment:
  CHATOS_DEPLOY_SERVER          SSH target (default: root@8.155.171.124)
  CHATOS_DEPLOY_ADMIN_USERNAME Plugin administrator (default: admin)
  CHATOS_DEPLOY_ADMIN_PASSWORD Plugin administrator password; prompts when omitted
  CHATOS_ADMIN_AUTH_BASE_URL    Unified admin User Service gateway prefix
  CHATOS_PLUGIN_API_BASE_URL    Unified admin Plugin Management gateway prefix
EOF
}

join_csv() {
  local IFS=,
  printf '%s' "$*"
}

deploy_cloud() {
  local services_csv="${1:-}"
  if [[ -n "$services_csv" ]]; then
    CHATOS_DEPLOY_SERVICES="$services_csv" "$SCRIPT_DIR/deploy-production.sh"
  else
    "$SCRIPT_DIR/deploy-production.sh"
  fi
}

show_status() {
  ssh -o BatchMode=yes "$DEPLOY_SERVER" bash -s -- "$REMOTE_DEPLOY_ROOT" <<'REMOTE'
set -euo pipefail
deploy_root="$1"
echo "== Deployment progress =="
if [[ -f "$deploy_root/deploy-status" ]]; then
  cat "$deploy_root/deploy-status"
else
  echo "status=unknown"
fi
echo
echo "== Active release =="
readlink -f "$deploy_root/current" || true
if [[ -f "$deploy_root/current/RELEASE_COMMIT" ]]; then
  printf 'commit='
  cat "$deploy_root/current/RELEASE_COMMIT"
fi
echo
echo "== Services =="
cd "$deploy_root/current"
./docker/deploy.sh ps
REMOTE
}

show_logs() {
  local service="${1:-}"
  if [[ -n "$service" ]]; then
    ssh -t "$DEPLOY_SERVER" "cd '$REMOTE_DEPLOY_ROOT/current' && ./docker/deploy.sh logs '$service'"
  else
    ssh -t "$DEPLOY_SERVER" "cd '$REMOTE_DEPLOY_ROOT/current' && ./docker/deploy.sh logs"
  fi
}

ensure_plugin_admin_password() {
  if [[ -n "$ADMIN_PASSWORD" ]]; then
    return
  fi
  if [[ ! -t 0 ]]; then
    echo "[ERROR] set CHATOS_DEPLOY_ADMIN_PASSWORD for non-interactive Plugin deployment" >&2
    exit 2
  fi
  read -r -s -p "Plugin administrator password: " ADMIN_PASSWORD
  echo >&2
}

plugin_admin_token() {
  curl --fail-with-body --silent --show-error \
    -H 'content-type: application/json' \
    --data "$(jq -nc --arg username "$ADMIN_USERNAME" --arg password "$ADMIN_PASSWORD" '{username:$username,password:$password}')" \
    "$ADMIN_AUTH_BASE_URL/auth/login" \
    | jq -er '.token'
}

ensure_deploy_tmp() {
  if [[ -z "$DEPLOY_TMP" ]]; then
    DEPLOY_TMP="$(mktemp -d "${TMPDIR:-/tmp}/chatos-online-deploy.XXXXXX")"
  fi
}

build_plugin_artifact() {
  local plugin="$1"
  local artifact_name version
  ensure_deploy_tmp
  case "$plugin" in
    browser)
      echo "[INFO] building Browser CDP Plugin"
      if [[ ! "${CHATOS_BROWSER_EXTENSION_ID:-}" =~ ^[a-p]{32}$ ]]; then
        echo "[ERROR] Browser Plugin publishing requires the 32-character Chrome Web Store Extension ID in CHATOS_BROWSER_EXTENSION_ID" >&2
        exit 1
      fi
      export CHATOS_BROWSER_EXTENSION_ID
      "$ROOT_DIR/plugins/browser/scripts/stage-local-npm.sh"
      local browser_target_dir browser_doctor configured_extension_id
      browser_target_dir="$(cd "$ROOT_DIR/plugins/browser" && cargo metadata --format-version 1 --no-deps | jq -er '.target_directory')"
      browser_doctor="$("$browser_target_dir/release/chatos-browser-cdp" doctor)"
      configured_extension_id="$(jq -r '.extension_id_configured // empty' <<< "$browser_doctor")"
      if [[ "$configured_extension_id" != "$CHATOS_BROWSER_EXTENSION_ID" ]]; then
        echo "[ERROR] Browser Plugin binary does not contain the requested Chrome Web Store Extension ID" >&2
        exit 1
      fi
      artifact_name="$(cd "$ROOT_DIR/plugins/browser/npm" && npm pack --pack-destination "$DEPLOY_TMP" | tail -n 1)"
      printf '%s\n' "$DEPLOY_TMP/$artifact_name"
      ;;
    computer-use)
      echo "[INFO] building Computer Use Plugin"
      npm --prefix "$ROOT_DIR/plugins/computer-use" run pack:chatos >&2
      version="$(jq -r '.version' "$ROOT_DIR/plugins/computer-use/package.json")"
      printf '%s\n' "$ROOT_DIR/plugins/computer-use/dist/chatos-artifacts/open-computer-use-$version.tgz"
      ;;
    document)
      echo "[INFO] verifying and building Document Tools Plugin"
      npm --prefix "$ROOT_DIR/plugins/document" run pack:verify >&2
      artifact_name="$(cd "$ROOT_DIR/plugins/document" && npm pack --pack-destination "$DEPLOY_TMP" | tail -n 1)"
      printf '%s\n' "$DEPLOY_TMP/$artifact_name"
      ;;
    *)
      echo "[ERROR] unknown Plugin: $plugin" >&2
      exit 2
      ;;
  esac
}

plugin_publisher_json() {
  case "$1" in
    browser|document)
      jq -nc '{id:"chatos",name:"Chatos",website:"https://github.com/chatos-ai"}'
      ;;
    computer-use)
      jq -nc '{id:"open-computer-use",name:"Open Computer Use",website:"https://github.com/iFurySt/open-codex-computer-use"}'
      ;;
  esac
}

publish_plugin() {
  local plugin="$1"
  local token artifact analysis artifact_sha name version license publisher publisher_id publisher_name publisher_website
  local catalog_response catalog_id latest_release_id releases current_release current_version current_sha license_url
  if [[ "$plugin" == "browser" && ! "${CHATOS_BROWSER_EXTENSION_ID:-}" =~ ^[a-p]{32}$ ]]; then
    echo "[ERROR] Browser Plugin publishing requires the 32-character Chrome Web Store Extension ID in CHATOS_BROWSER_EXTENSION_ID" >&2
    exit 1
  fi
  token="$(plugin_admin_token)"
  artifact="$(build_plugin_artifact "$plugin" | tail -n 1)"
  [[ -f "$artifact" ]] || { echo "[ERROR] Plugin artifact was not created: $artifact" >&2; exit 1; }

  echo "[INFO] analyzing $artifact"
  analysis="$(curl --fail-with-body --silent --show-error \
    -H "authorization: Bearer $token" \
    -F "package=@$artifact" \
    "$PLUGIN_API_BASE_URL/admin/plugin-package/analyze")"
  artifact_sha="$(jq -er '.artifact_sha256' <<< "$analysis")"
  name="$(jq -er '.manifest.name' <<< "$analysis")"
  version="$(jq -er '.manifest.version' <<< "$analysis")"
  license="$(jq -er '.manifest.license' <<< "$analysis")"
  publisher="$(plugin_publisher_json "$plugin")"
  publisher_id="$(jq -r '.id' <<< "$publisher")"
  publisher_name="$(jq -r '.name' <<< "$publisher")"
  publisher_website="$(jq -r '.website' <<< "$publisher")"
  license_url=""
  if [[ "$plugin" == "computer-use" ]]; then
    license_url="https://github.com/iFurySt/open-codex-computer-use/blob/main/LICENSE"
  fi

  catalog_response="$(curl --fail-with-body --silent --show-error \
    -H "authorization: Bearer $token" \
    "$PLUGIN_API_BASE_URL/admin/plugins?q=$name&limit=50")"
  catalog_id="$(jq -r --arg name "$name" '.items[]? | select(.name == $name) | .id' <<< "$catalog_response" | head -n 1)"
  latest_release_id="$(jq -r --arg name "$name" '.items[]? | select(.name == $name) | .latest_release_id' <<< "$catalog_response" | head -n 1)"

  if [[ -n "$catalog_id" && -n "$latest_release_id" ]]; then
    releases="$(curl --fail-with-body --silent --show-error \
      -H "authorization: Bearer $token" \
      "$PLUGIN_API_BASE_URL/admin/plugins/$catalog_id/releases?limit=100")"
    current_release="$(jq -c --arg id "$latest_release_id" '.items[]? | select(.id == $id)' <<< "$releases")"
    current_version="$(jq -r '.version // empty' <<< "$current_release")"
    current_sha="$(jq -r '.artifact_sha256 // empty' <<< "$current_release")"
    if [[ "$current_version" == "$version" && "$current_sha" != "$artifact_sha" ]]; then
      echo "[ERROR] $name $version already exists with a different artifact; bump the Plugin version" >&2
      exit 1
    fi
  fi

  if [[ -z "$catalog_id" || "$current_version" != "$version" || "$current_sha" != "$artifact_sha" ]]; then
    echo "[INFO] publishing $name $version"
    curl --fail-with-body --silent --show-error \
      -H "authorization: Bearer $token" \
      -H 'content-type: application/json' \
      --data "$(jq -nc \
        --arg artifact_sha256 "$artifact_sha" \
        --arg publisher_id "$publisher_id" \
        --arg publisher_name "$publisher_name" \
        --arg publisher_website "$publisher_website" \
        --arg license_id "$license" \
        --arg license_url "$license_url" \
        '{artifact_sha256:$artifact_sha256,marketplace_id:"chatos-marketplace",publisher_id:$publisher_id,publisher_name:$publisher_name,publisher_website:$publisher_website,license_id:$license_id,license_url:(if $license_url == "" then null else $license_url end),redistributable:true,visibility:"public",featured:true,release_channel:"stable"}')" \
      "$PLUGIN_API_BASE_URL/admin/plugin-package/publish" >/dev/null
    catalog_response="$(curl --fail-with-body --silent --show-error \
      -H "authorization: Bearer $token" \
      "$PLUGIN_API_BASE_URL/admin/plugins?q=$name&limit=50")"
    catalog_id="$(jq -er --arg name "$name" '.items[] | select(.name == $name) | .id' <<< "$catalog_response" | head -n 1)"
  else
    echo "[INFO] $name $version is already published with the same artifact"
  fi

  echo "[INFO] approving redistribution metadata for $name"
  curl --fail-with-body --silent --show-error \
    -X PATCH \
    -H "authorization: Bearer $token" \
    -H 'content-type: application/json' \
    --data "$(jq -nc --arg license_id "$license" --arg license_url "$license_url" '{license_id:$license_id,license_url:(if $license_url == "" then null else $license_url end),redistributable:true}')" \
    "$PLUGIN_API_BASE_URL/admin/plugins/$catalog_id/license" >/dev/null
  echo "[OK] Plugin deployed: $name $version"
}

deploy_plugins() {
  local plugins=("$@")
  local plugin
  if [[ ${#plugins[@]} -eq 0 ]]; then
    echo "[ERROR] expected Plugin: all, browser, computer-use, or document" >&2
    exit 2
  fi
  if [[ "${plugins[0]}" == "all" ]]; then
    if [[ ${#plugins[@]} -ne 1 ]]; then
      echo "[ERROR] Plugin 'all' cannot be combined with individual Plugins" >&2
      exit 2
    fi
    plugins=(browser computer-use document)
  fi
  for plugin in "${plugins[@]}"; do
    case "$plugin" in
      browser|computer-use|document) ;;
      *)
        echo "[ERROR] unknown Plugin: $plugin" >&2
        exit 2
        ;;
    esac
  done
  ensure_plugin_admin_password
  for plugin in "${plugins[@]}"; do
    publish_plugin "$plugin"
  done
}

package_client() {
  case "$1" in
    mac)
      "$ROOT_DIR/clients/macos/scripts/package-debug-app.sh"
      ;;
    windows)
      if command -v dotnet >/dev/null 2>&1; then
        dotnet publish "$ROOT_DIR/clients/windows/ChatOS.Win.sln" --configuration Release
      else
        echo "[ERROR] Windows client packaging requires the .NET SDK" >&2
        exit 2
      fi
      ;;
    *)
      echo "[ERROR] expected client: mac or windows" >&2
      exit 2
      ;;
  esac
}

list_components() {
  echo "Cloud build services:"
  "$ROOT_DIR/docker/deploy.sh" build-services | sed 's/^/  /'
  cat <<'EOF'
Cloud configuration components:
  gateway-config
Plugins:
  browser
  computer-use
  document
Clients:
  mac
  windows
EOF
}

interactive_menu() {
  cat <<'EOF'
ChatOS online deployment
  1) Deploy everything online (cloud + three Plugins)
  2) Deploy all cloud services
  3) Deploy all backend services
  4) Deploy selected cloud service(s)
  5) Deploy gateway configuration only
  6) Deploy all Plugins
  7) Deploy Browser CDP
  8) Deploy Computer Use
  9) Deploy Document Tools
 10) Show deployment status
 11) Follow service logs
EOF
  read -r -p "Select: " selection
  case "$selection" in
    1) set -- all ;;
    2) set -- cloud ;;
    3) set -- cloud-backends ;;
    4)
      list_components
      read -r -p "Service names separated by spaces: " services
      # shellcheck disable=SC2086
      set -- service $services
      ;;
    5) set -- gateway ;;
    6) set -- plugin all ;;
    7) set -- plugin browser ;;
    8) set -- plugin computer-use ;;
    9) set -- plugin document ;;
    10) set -- status ;;
    11)
      read -r -p "Service name (empty for all): " service
      set -- logs "$service"
      ;;
    *) echo "[ERROR] invalid selection" >&2; exit 2 ;;
  esac
  main "$@"
}

main() {
  cd "$ROOT_DIR"
  local action="${1:-}"
  if [[ -z "$action" ]]; then
    interactive_menu
    return
  fi
  shift || true
  case "$action" in
    all)
      deploy_cloud ""
      deploy_plugins all
      ;;
    cloud)
      deploy_cloud ""
      ;;
    cloud-backends)
      deploy_cloud "$(join_csv "${BACKEND_SERVICES[@]}")"
      ;;
    cloud-frontends)
      deploy_cloud "$(join_csv "${FRONTEND_SERVICES[@]}")"
      ;;
    gateway)
      deploy_cloud "gateway-config"
      ;;
    service)
      [[ $# -gt 0 ]] || { echo "[ERROR] provide at least one service" >&2; exit 2; }
      deploy_cloud "$(join_csv "$@")"
      ;;
    plugin)
      deploy_plugins "$@"
      ;;
    client)
      package_client "${1:-}"
      ;;
    status)
      show_status
      ;;
    logs)
      show_logs "${1:-}"
      ;;
    list)
      list_components
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
