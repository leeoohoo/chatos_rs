#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEPLOY_SERVER="${CHATOS_DEPLOY_SERVER:-root@8.155.171.124}"
DEPLOY_BRANCH="${CHATOS_DEPLOY_BRANCH:-3.0.0}"
REMOTE_SOURCE_REPO="${CHATOS_DEPLOY_SOURCE_REPO:-/opt/chatos_rs}"
REMOTE_DEPLOY_ROOT="${CHATOS_DEPLOY_ROOT:-/opt/chatos-deploy}"
DEPLOY_SERVICES_CSV="${CHATOS_DEPLOY_SERVICES:-}"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] missing command: $1" >&2
    exit 1
  fi
}

need_cmd git
need_cmd ssh
need_cmd bash
need_cmd cargo

cd "$ROOT_DIR"

if [[ -n "$DEPLOY_SERVICES_CSV" ]]; then
  IFS=',' read -r -a requested_services <<< "$DEPLOY_SERVICES_CSV"
  available_services="$(./docker/deploy.sh build-services)"
  normalized_services=()
  for service in "${requested_services[@]}"; do
    service="$(printf '%s' "$service" | xargs)"
    [[ -n "$service" ]] || continue
    if [[ "$service" == "gateway-config" ]]; then
      normalized_services+=("$service")
      continue
    fi
    if ! grep -Fxq "$service" <<< "$available_services"; then
      echo "[ERROR] service is not independently buildable: $service" >&2
      echo "Available services:" >&2
      printf '%s\n' "$available_services" >&2
      printf '%s\n' "gateway-config" >&2
      exit 2
    fi
    normalized_services+=("$service")
  done
  if [[ ${#normalized_services[@]} -eq 0 ]]; then
    echo "[ERROR] CHATOS_DEPLOY_SERVICES did not contain a valid service" >&2
    exit 2
  fi
  DEPLOY_SERVICES_CSV="$(IFS=,; printf '%s' "${normalized_services[*]}")"
  echo "[INFO] selected production services: $DEPLOY_SERVICES_CSV"
else
  echo "[INFO] selected production scope: all cloud services"
fi

current_branch="$(git branch --show-current)"
if [[ "$current_branch" != "$DEPLOY_BRANCH" ]]; then
  echo "[ERROR] production deploy must run from branch $DEPLOY_BRANCH; current branch is $current_branch" >&2
  exit 1
fi

dirty_status="$(git status --porcelain --untracked-files=no)"
blocked_dirty_status=""
ignored_dirty_count=0
while IFS= read -r dirty_line; do
  [[ -n "$dirty_line" ]] || continue
  dirty_path="${dirty_line:3}"
  case "$dirty_path" in
    clients/macos/*|clients/windows/*|plugins/web-design-studio/*)
      ignored_dirty_count=$((ignored_dirty_count + 1))
      ;;
    *)
      blocked_dirty_status+="$dirty_line"$'\n'
      ;;
  esac
done <<< "$dirty_status"

if [[ -n "$blocked_dirty_status" ]]; then
  echo "[ERROR] tracked files are not clean; commit and push the release first" >&2
  printf '%s' "$blocked_dirty_status" >&2
  exit 1
fi
if (( ignored_dirty_count > 0 )); then
  echo "[WARN] ignoring $ignored_dirty_count tracked client/Web Design development changes; cloud deployment uses the pushed commit only" >&2
fi

echo "[INFO] checking origin/$DEPLOY_BRANCH"
git fetch --quiet origin "$DEPLOY_BRANCH"
release_commit="$(git rev-parse HEAD)"
origin_commit="$(git rev-parse "origin/$DEPLOY_BRANCH")"
if [[ "$release_commit" != "$origin_commit" ]]; then
  echo "[ERROR] HEAD is not the pushed origin/$DEPLOY_BRANCH commit" >&2
  echo "        HEAD:   $release_commit" >&2
  echo "        origin: $origin_commit" >&2
  exit 1
fi

if [[ "$DEPLOY_SERVICES_CSV" == "gateway-config" ]]; then
  echo "[INFO] gateway-only deployment skips unrelated Rust service builds"
else
  echo "[INFO] checking every production Rust service before remote deployment"
  bash scripts/verify-repository.sh rust-build
fi

release_tag="$(date +%Y%m%d-%H%M%S)-${release_commit:0:8}"
remote_deploy_services_arg="${DEPLOY_SERVICES_CSV:-__CHATOS_ALL_SERVICES__}"
echo "[INFO] starting background deployment of $release_commit as $release_tag on $DEPLOY_SERVER"

if ! ssh -o BatchMode=yes "$DEPLOY_SERVER" bash -s -- \
  "$release_commit" \
  "$release_tag" \
  "$DEPLOY_BRANCH" \
  "$REMOTE_SOURCE_REPO" \
  "$REMOTE_DEPLOY_ROOT" \
  "$remote_deploy_services_arg" <<'REMOTE_SCRIPT'
set -euo pipefail

run_deployment() {
set -euo pipefail
release_commit="$1"
release_tag="$2"
deploy_branch="$3"
source_repo="$4"
deploy_root="$5"
deploy_services_arg="$6"
if [[ "$deploy_services_arg" == "__CHATOS_ALL_SERVICES__" ]]; then
  deploy_services_csv=""
else
  deploy_services_csv="$deploy_services_arg"
fi
release_dir="$deploy_root/releases/$release_tag"
current_link="$deploy_root/current"
previous_release=""
nginx_target="/etc/nginx/sites-available/chatos.conf"
nginx_backup=""
switched=0
status_file="$deploy_root/deploy-status"
pid_file="$deploy_root/deploy.pid"
job_pid="${CHATOS_DEPLOY_JOB_PID:-}"
log_file="$deploy_root/deploy-logs/$release_tag.log"

clear_deploy_pid() {
  local recorded_pid=""
  if [[ -f "$pid_file" ]]; then
    read -r recorded_pid < "$pid_file" || true
  fi
  if [[ -n "$job_pid" && "$recorded_pid" == "$job_pid" ]]; then
    rm -f -- "$pid_file"
  fi
}

write_deploy_status() {
  local status="$1"
  local stage="$2"
  local message="$3"
  local temporary_status="$deploy_root/.deploy-status.tmp"
  mkdir -p "$deploy_root"
  {
    printf 'status=%s\n' "$status"
    printf 'stage=%s\n' "$stage"
    printf 'message=%s\n' "$message"
    printf 'release=%s\n' "$release_tag"
    printf 'commit=%s\n' "$release_commit"
    printf 'services=%s\n' "${deploy_services_csv:-all}"
    printf 'pid=%s\n' "${job_pid:-unknown}"
    printf 'log=%s\n' "$log_file"
    printf 'updated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$temporary_status"
  mv -f "$temporary_status" "$status_file"
}

service_is_selected() {
  local requested="$1"
  local selected
  for selected in "${deploy_services[@]}"; do
    if [[ "$selected" == "$requested" ]]; then
      return 0
    fi
  done
  return 1
}

resolve_service_image() {
  local target_release="$1"
  local service="$2"
  (
    cd "$target_release"
    docker compose \
      -f docker/compose.yml \
      -f docker/compose.platform.yml \
      --env-file docker/bootstrap.conf \
      config --format json
  ) | python3 -c '
import json, sys
service = sys.argv[1]
document = json.load(sys.stdin)
try:
    print(document["services"][service]["image"])
except KeyError as exc:
    raise SystemExit(f"missing image for service {service}: {exc}")
' "$service"
}

resolve_runtime_services_for_images() {
  local target_release="$1"
  shift
  (
    cd "$target_release"
    docker compose \
      -f docker/compose.yml \
      -f docker/compose.platform.yml \
      --env-file docker/bootstrap.conf \
      config --format json
  ) | python3 -c '
import json, sys
selected = set(sys.argv[1:])
document = json.load(sys.stdin)
services = document.get("services", {})
missing = sorted(name for name in selected if name not in services)
if missing:
    raise SystemExit(f"missing selected Compose services: {missing}")
images = {services[name].get("image") for name in selected}
for name, service in services.items():
    if service.get("image") in images:
        print(name)
' "$@"
}

deploy_all=0
deploy_gateway_config=0
deploy_services=()
selected_runtime_services=()
if [[ -z "$deploy_services_csv" ]]; then
  deploy_all=1
else
  IFS=',' read -r -a requested_components <<< "$deploy_services_csv"
  for component in "${requested_components[@]}"; do
    if [[ "$component" == "gateway-config" ]]; then
      deploy_gateway_config=1
    else
      deploy_services+=("$component")
    fi
  done
fi

ensure_admin_certificate() {
  local certificate=/etc/letsencrypt/live/jgoool.com/fullchain.pem
  local expected_domains=(
    jgoool.com
    www.jgoool.com
    app.jgoool.com
    gateway.jgoool.com
    plugin-ui.jgoool.com
    admin.jgoool.com
    config.jgoool.com
    user.jgoool.com
    memory.jgoool.com
    project.jgoool.com
    plugin.jgoool.com
    task.jgoool.com
    official.jgoool.com
    connector.jgoool.com
    local-connector.jgoool.com
    harness.jgoool.com
    ci.jgoool.com
  )
  local expected_sans current_sans
  expected_sans="$(printf 'DNS:%s\n' "${expected_domains[@]}" | sort)"
  current_sans="$(
    openssl x509 -in "$certificate" -noout -ext subjectAltName \
      | grep -oE 'DNS:[^,[:space:]]+' \
      | sort
  )"
  if [[ "$current_sans" == "$expected_sans" ]]; then
    return 0
  fi

  echo "[INFO] refreshing the public certificate domain set"
  local certbot_domains=()
  local domain
  for domain in "${expected_domains[@]}"; do
    certbot_domains+=( -d "$domain" )
  done
  certbot certonly \
    --non-interactive \
    --agree-tos \
    --webroot \
    --webroot-path /var/www/letsencrypt \
    --cert-name jgoool.com \
    --force-renewal \
    "${certbot_domains[@]}"

  current_sans="$(
    openssl x509 -in "$certificate" -noout -ext subjectAltName \
      | grep -oE 'DNS:[^,[:space:]]+' \
      | sort
  )"
  [[ "$current_sans" == "$expected_sans" ]]
}

start_release_with_retries() {
  local target_release="$1"
  local attempt
  for attempt in 1 2 3; do
    if (
      cd "$target_release"
      ./docker/deploy.sh fast
    ); then
      return 0
    fi
    echo "[WARN] release startup attempt $attempt failed: $target_release" >&2
    if (( attempt < 3 )); then
      sleep 10
    fi
  done
  return 1
}

restart_selected_release_with_retries() {
  local target_release="$1"
  shift
  local attempt
  for attempt in 1 2 3; do
    if (
      cd "$target_release"
      ./docker/deploy.sh restart-fast "$@"
    ); then
      return 0
    fi
    echo "[WARN] selected service restart attempt $attempt failed: $target_release" >&2
    if (( attempt < 3 )); then
      sleep 10
    fi
  done
  return 1
}

rollback() {
  local exit_code=$?
  trap - EXIT
  if (( exit_code == 0 )); then
    return
  fi

  echo "[ERROR] deployment failed; restoring the previous release" >&2
  write_deploy_status failed rollback "Deployment failed; restoring the previous release"
  if (( switched == 1 )) && [[ -n "$previous_release" && -d "$previous_release" ]]; then
    ln -sfn "$previous_release" "$deploy_root/current.rollback"
    mv -Tf "$deploy_root/current.rollback" "$current_link"
    if [[ ${#selected_runtime_services[@]} -gt 0 ]]; then
      restart_selected_release_with_retries "$previous_release" "${selected_runtime_services[@]}" || true
    else
      start_release_with_retries "$previous_release" || true
    fi
  fi
  if [[ -n "$nginx_backup" && -f "$nginx_backup" ]]; then
    install -m 0644 "$nginx_backup" "$nginx_target"
    nginx -t >/dev/null 2>&1 && systemctl reload nginx || true
  fi
  clear_deploy_pid
  exit "$exit_code"
}
cancel_deployment() {
  echo "[WARN] deployment was superseded by a newer deployment request" >&2
  exit 143
}
trap cancel_deployment TERM INT
trap rollback EXIT

write_deploy_status running prepare "Validating server and preparing the release"

for command_name in git docker curl nginx systemctl python3 certbot openssl; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "[ERROR] server is missing command: $command_name" >&2
    exit 1
  fi
done

if [[ ! -d "$source_repo/.git" ]]; then
  echo "[ERROR] server source repository is missing: $source_repo" >&2
  exit 1
fi
if [[ ! -L "$current_link" ]]; then
  echo "[ERROR] current production release link is missing: $current_link" >&2
  exit 1
fi
if [[ -e "$release_dir" ]]; then
  echo "[ERROR] release directory already exists: $release_dir" >&2
  exit 1
fi

previous_release="$(readlink -f "$current_link")"
if [[ ! -f "$previous_release/docker/bootstrap.conf" ]]; then
  echo "[ERROR] active production bootstrap file is missing" >&2
  exit 1
fi
if [[ ! -d "$previous_release/docker/secrets" ]]; then
  echo "[ERROR] active production secret directory is missing" >&2
  exit 1
fi

echo "[INFO] fetching origin/$deploy_branch on the server"
write_deploy_status running fetch "Fetching the requested Git commit"
git -C "$source_repo" fetch --quiet origin "$deploy_branch"
server_origin_commit="$(git -C "$source_repo" rev-parse "origin/$deploy_branch")"
if [[ "$server_origin_commit" != "$release_commit" ]]; then
  echo "[ERROR] server resolved a different origin/$deploy_branch commit" >&2
  exit 1
fi

mkdir -p "$release_dir"
git -C "$source_repo" archive "$release_commit" | tar -x -C "$release_dir"
printf '%s\n' "$release_commit" > "$release_dir/RELEASE_COMMIT"
cp -p "$previous_release/docker/bootstrap.conf" "$release_dir/docker/bootstrap.conf"
cp -a "$previous_release/docker/secrets" "$release_dir/docker/secrets"
retired_sandbox_mtls="$release_dir/docker/secrets/sandbox-manager-mtls"
if [[ -e "$retired_sandbox_mtls" ]]; then
  echo "[INFO] removing retired Sandbox Manager mTLS material from the release"
  rm -rf -- "$retired_sandbox_mtls"
fi

ensure_admin_certificate

echo "[INFO] validating copied production secrets and mTLS material"
write_deploy_status running validate "Validating production configuration and mTLS material"
(
  cd "$release_dir"
  ./docker/deploy.sh validate-runtime-material
)

update_image_tag=true
if (( deploy_all == 0 )) && [[ ${#deploy_services[@]} -eq 0 ]]; then
  update_image_tag=false
fi
python3 - "$release_dir/docker/bootstrap.conf" "$release_tag" "$update_image_tag" <<'PY'
from pathlib import Path
import secrets
import sys

path = Path(sys.argv[1])
release_tag = sys.argv[2]
update_image_tag = sys.argv[3] == "true"
secret_key = "CHATOS_USER_SERVICE_INTERNAL_API_SECRET"
development_secret = "change_me_chatos_user_service_secret"
updates = {
    "CHATOS_DOCKER_MODE": "build",
}
if update_image_tag:
    updates["CHATOS_IMAGE_TAG"] = release_tag
lines = path.read_text().splitlines()
current_values = {}
for line in lines:
    key, separator, value = line.partition("=")
    if separator:
        current_values[key] = value
website_public_base_key = "OFFICIAL_WEBSITE_PUBLIC_BASE_URL"
website_public_base = current_values.get(website_public_base_key, "").strip().lower()
if (
    not website_public_base
    or "localhost" in website_public_base
    or "127.0.0.1" in website_public_base
):
    updates[website_public_base_key] = "https://www.jgoool.com"
current_secret = current_values.get(secret_key, "").strip()
if not current_secret or current_secret == development_secret or len(current_secret) < 32:
    updates[secret_key] = secrets.token_urlsafe(48)
seen = set()
rendered = []
for line in lines:
    key, separator, _ = line.partition("=")
    if separator and key in updates:
        rendered.append(f"{key}={updates[key]}")
        seen.add(key)
    else:
        rendered.append(line)
for key, value in updates.items():
    if key not in seen:
        rendered.append(f"{key}={value}")
path.write_text("\n".join(rendered) + "\n")
path.chmod(0o600)
PY

echo "[INFO] building release images while the current release stays online"
write_deploy_status running build "Building selected production images"
(
  cd "$release_dir"
  if (( deploy_all == 1 )); then
    ./docker/deploy.sh build
  elif [[ ${#deploy_services[@]} -gt 0 ]]; then
    all_build_services="$(./docker/deploy.sh build-services)"
    while IFS= read -r service; do
      [[ -n "$service" ]] || continue
      if service_is_selected "$service"; then
        continue
      fi
      old_image="$(resolve_service_image "$previous_release" "$service")"
      new_image="$(resolve_service_image "$release_dir" "$service")"
      echo "[INFO] carrying forward image for unchanged service: $service"
      docker image inspect "$old_image" >/dev/null
      docker tag "$old_image" "$new_image"
    done <<< "$all_build_services"
    ./docker/deploy.sh build "${deploy_services[@]}"
  else
    echo "[INFO] no service image build is required for gateway configuration"
  fi
)

if [[ ${#deploy_services[@]} -gt 0 ]]; then
  while IFS= read -r service; do
    [[ -n "$service" ]] || continue
    selected_runtime_services+=("$service")
  done < <(resolve_runtime_services_for_images "$release_dir" "${deploy_services[@]}")
  if [[ ${#selected_runtime_services[@]} -eq 0 ]]; then
    echo "[ERROR] selected images do not map to any Compose runtime service" >&2
    exit 1
  fi
  echo "[INFO] runtime services selected for restart: ${selected_runtime_services[*]}"
fi
if (( deploy_gateway_config == 1 )); then
  selected_runtime_services+=("apisix-gateway")
  echo "[INFO] gateway configuration selected for restart"
fi

if (( deploy_all == 1 || deploy_gateway_config == 1 )); then
  nginx_backup="$deploy_root/backups/chatos-nginx-$release_tag.conf"
  mkdir -p "$deploy_root/backups"
  cp -p "$nginx_target" "$nginx_backup"
fi

ln -sfn "$release_dir" "$deploy_root/current.next"
mv -Tf "$deploy_root/current.next" "$current_link"
switched=1

echo "[INFO] switching containers to the new release"
write_deploy_status running switch "Switching containers to the new release"
if (( deploy_all == 0 )); then
  restart_selected_release_with_retries "$release_dir" "${selected_runtime_services[@]}"
else
  start_release_with_retries "$release_dir"
fi
if (( deploy_all == 1 || deploy_gateway_config == 1 )); then
  install -m 0644 "$release_dir/docker/nginx/jgoool-https.conf" "$nginx_target"
  nginx -t
  systemctl reload nginx
fi

deadline=$((SECONDS + 300))
write_deploy_status running health "Waiting for service health checks"
while true; do
  health_state="$(
    docker ps \
      --filter label=com.docker.compose.project=chatos-rs \
      --format '{{.Names}} {{.Status}}' \
      | grep -E 'unhealthy|health: starting' || true
  )"
  if [[ -z "$health_state" ]]; then
    break
  fi
  if (( SECONDS >= deadline )); then
    echo "[ERROR] services did not become healthy in time" >&2
    printf '%s\n' "$health_state" >&2
    exit 1
  fi
  sleep 5
done

curl --fail --silent --show-error --max-time 20 \
  http://127.0.0.1:9080/api/chatos/health >/dev/null

curl --fail --silent --show-error --max-time 20 \
  --header "Host: admin.jgoool.com" \
  http://127.0.0.1:9080/api/admin/user-service/health >/dev/null

frontend_hosts=(
  admin.jgoool.com
  config.jgoool.com
  user.jgoool.com
  memory.jgoool.com
  project.jgoool.com
  plugin.jgoool.com
  task.jgoool.com
  official.jgoool.com
)
for host in "${frontend_hosts[@]}"; do
  curl --fail --silent --show-error --max-time 20 \
    --header "Host: $host" \
    http://127.0.0.1:9080/ >/dev/null
done

connector_status="$(
  curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --max-time 20 --header 'Host: connector.jgoool.com' \
    http://127.0.0.1:9080/health
)"
case "$connector_status" in
  000|502|503|504)
    echo "[ERROR] connector gateway route is unavailable: HTTP $connector_status" >&2
    exit 1
    ;;
esac

for url in \
  https://gateway.jgoool.com/api/chatos/health \
  https://jgoool.com \
  https://admin.jgoool.com \
  https://user.jgoool.com \
  https://memory.jgoool.com \
  https://project.jgoool.com \
  https://plugin.jgoool.com \
  https://official.jgoool.com
do
  curl --fail --silent --show-error --max-time 30 "$url" >/dev/null
done

echo "[OK] production release is healthy: $release_tag"
if [[ ${#deploy_services[@]} -gt 0 ]]; then
  echo "[OK] updated services: ${deploy_services[*]}"
fi
echo "[OK] active release: $(readlink -f "$current_link")"
write_deploy_status complete healthy "Production release is healthy"
clear_deploy_pid
trap - EXIT
}

release_commit="$1"
release_tag="$2"
deploy_root="$5"
deploy_services_arg="$6"
status_file="$deploy_root/deploy-status"
pid_file="$deploy_root/deploy.pid"
log_dir="$deploy_root/deploy-logs"
log_file="$log_dir/$release_tag.log"
active_pid=""

mkdir -p "$deploy_root" "$log_dir"
for command_name in flock nohup setsid; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "[ERROR] server is missing command: $command_name" >&2
    exit 1
  fi
done
exec 9> "$deploy_root/deploy.lock"
if ! flock -n 9; then
  if [[ -f "$pid_file" ]]; then
    read -r active_pid < "$pid_file" || true
  fi
  if [[ ! "$active_pid" =~ ^[0-9]+$ ]] || ! kill -0 "$active_pid" 2>/dev/null; then
    echo "[ERROR] deployment lock is held but the active deployment PID is unavailable" >&2
    exit 1
  fi
  echo "[INFO] stopping previous deployment with PID $active_pid"
  kill -TERM -- "-$active_pid" 2>/dev/null || kill -TERM "$active_pid" 2>/dev/null || true
  lock_acquired=0
  for _ in {1..120}; do
    if flock -n 9; then
      lock_acquired=1
      break
    fi
    sleep 1
  done
  if (( lock_acquired == 0 )); then
    echo "[WARN] previous deployment did not stop cleanly; forcing its process group to exit" >&2
    kill -KILL -- "-$active_pid" 2>/dev/null || kill -KILL "$active_pid" 2>/dev/null || true
    for _ in {1..10}; do
      if flock -n 9; then
        lock_acquired=1
        break
      fi
      sleep 1
    done
  fi
  if (( lock_acquired == 0 )); then
    echo "[ERROR] previous deployment did not release the deployment lock" >&2
    exit 1
  fi
  echo "[OK] previous deployment stopped; starting the new deployment"
fi
rm -f -- "$pid_file"

: > "$log_file"
ln -sfn "deploy-logs/$release_tag.log" "$deploy_root/deploy.log.next"
mv -Tf "$deploy_root/deploy.log.next" "$deploy_root/deploy.log"
temporary_status="$deploy_root/.deploy-status.tmp"
{
  printf 'status=queued\n'
  printf 'stage=queued\n'
  printf 'message=Background deployment queued\n'
  printf 'release=%s\n' "$release_tag"
  printf 'commit=%s\n' "$release_commit"
  if [[ "$deploy_services_arg" == "__CHATOS_ALL_SERVICES__" ]]; then
    printf 'services=all\n'
  else
    printf 'services=%s\n' "$deploy_services_arg"
  fi
  printf 'pid=pending\n'
  printf 'log=%s\n' "$log_file"
  printf 'updated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$temporary_status"
mv -f "$temporary_status" "$status_file"

export -f run_deployment
nohup setsid bash -c '
  trap "" HUP
  CHATOS_DEPLOY_JOB_PID="$BASHPID"
  run_deployment "$@"
' bash "$@" 9>&9 </dev/null >> "$log_file" 2>&1 &
deployment_pid=$!
printf '%s\n' "$deployment_pid" > "$pid_file"
disown "$deployment_pid" 2>/dev/null || true
exec 9>&-

sleep 1
if ! kill -0 "$deployment_pid" 2>/dev/null; then
  deployment_status="$(awk -F= '$1 == "status" { print $2; exit }' "$status_file" 2>/dev/null || true)"
  if [[ "$deployment_status" != "complete" ]]; then
    echo "[ERROR] background deployment exited during startup" >&2
    tail -n 40 "$log_file" >&2 || true
    exit 1
  fi
fi

echo "[OK] background deployment started"
echo "release=$release_tag"
echo "pid=$deployment_pid"
echo "log=$log_file"
REMOTE_SCRIPT
then
  echo "[ERROR] failed to start remote background deployment: $release_tag" >&2
  echo "[INFO] inspect the server state with: ./scripts/deploy-online.sh status" >&2
  exit 1
fi

echo "[OK] deployment is running in the server background: $release_tag"
echo "[INFO] follow progress with: ./scripts/deploy-online.sh logs"
echo "[INFO] inspect status with: ./scripts/deploy-online.sh status"
