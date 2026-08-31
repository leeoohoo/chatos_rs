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

current_branch="$(git branch --show-current)"
if [[ "$current_branch" != "$DEPLOY_BRANCH" ]]; then
  echo "[ERROR] production deploy must run from branch $DEPLOY_BRANCH; current branch is $current_branch" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "[ERROR] working tree is not clean; commit and push the release first" >&2
  git status --short >&2
  exit 1
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

echo "[INFO] checking every production Rust service before remote deployment"
bash scripts/verify-repository.sh rust-build

release_tag="$(date +%Y%m%d-%H%M%S)-${release_commit:0:8}"
echo "[INFO] deploying $release_commit as $release_tag to $DEPLOY_SERVER"

ssh -o BatchMode=yes "$DEPLOY_SERVER" bash -s -- \
  "$release_commit" \
  "$release_tag" \
  "$DEPLOY_BRANCH" \
  "$REMOTE_SOURCE_REPO" \
  "$REMOTE_DEPLOY_ROOT" <<'REMOTE_SCRIPT'
set -euo pipefail

release_commit="$1"
release_tag="$2"
deploy_branch="$3"
source_repo="$4"
deploy_root="$5"
release_dir="$deploy_root/releases/$release_tag"
current_link="$deploy_root/current"
previous_release=""
nginx_target="/etc/nginx/sites-available/chatos.conf"
nginx_backup=""
switched=0

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

rollback() {
  local exit_code=$?
  trap - EXIT
  if (( exit_code == 0 )); then
    return
  fi

  echo "[ERROR] deployment failed; restoring the previous release" >&2
  if (( switched == 1 )) && [[ -n "$previous_release" && -d "$previous_release" ]]; then
    ln -sfn "$previous_release" "$deploy_root/current.rollback"
    mv -Tf "$deploy_root/current.rollback" "$current_link"
    (
      cd "$previous_release"
      ./docker/deploy.sh fast
    ) || true
  fi
  if [[ -n "$nginx_backup" && -f "$nginx_backup" ]]; then
    install -m 0644 "$nginx_backup" "$nginx_target"
    nginx -t >/dev/null 2>&1 && systemctl reload nginx || true
  fi
  exit "$exit_code"
}
trap rollback EXIT

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

ensure_admin_certificate

python3 - "$release_dir/docker/bootstrap.conf" "$release_tag" <<'PY'
from pathlib import Path
import secrets
import sys

path = Path(sys.argv[1])
release_tag = sys.argv[2]
secret_key = "CHATOS_USER_SERVICE_INTERNAL_API_SECRET"
development_secret = "change_me_chatos_user_service_secret"
updates = {
    "CHATOS_DOCKER_MODE": "build",
    "CHATOS_IMAGE_TAG": release_tag,
}
lines = path.read_text().splitlines()
current_values = {}
for line in lines:
    key, separator, value = line.partition("=")
    if separator:
        current_values[key] = value
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
(
  cd "$release_dir"
  ./docker/deploy.sh build
)

nginx_backup="$deploy_root/backups/chatos-nginx-$release_tag.conf"
mkdir -p "$deploy_root/backups"
cp -p "$nginx_target" "$nginx_backup"

ln -sfn "$release_dir" "$deploy_root/current.next"
mv -Tf "$deploy_root/current.next" "$current_link"
switched=1

echo "[INFO] switching containers to the new release"
(
  cd "$release_dir"
  ./docker/deploy.sh fast
)

install -m 0644 "$release_dir/docker/nginx/jgoool-https.conf" "$nginx_target"
nginx -t
systemctl reload nginx

deadline=$((SECONDS + 300))
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
echo "[OK] active release: $(readlink -f "$current_link")"
trap - EXIT
REMOTE_SCRIPT

echo "[OK] deployed $release_tag"
