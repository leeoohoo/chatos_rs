#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"

# --------- 用户配置区 ---------
REMOTE_HOST="${REMOTE_HOST:-}"
REMOTE_USER="${REMOTE_USER:-ubuntu}"
REMOTE_PASSWORD="${REMOTE_PASSWORD:-}"
REMOTE_PORT="${REMOTE_PORT:-22}"
REMOTE_APP_ROOT="${REMOTE_APP_ROOT:-/opt/chatos/chatos_rs}"
REMOTE_DEPLOY_ROOT="${REMOTE_DEPLOY_ROOT:-/opt/chatos}"
REMOTE_ENV_DIR="${REMOTE_ENV_DIR:-/etc/chatos}"
REMOTE_SERVICE_NAME="${REMOTE_SERVICE_NAME:-chatos-backend}"
REMOTE_NGINX_SITE="${REMOTE_NGINX_SITE:-/etc/nginx/sites-available/chatos.conf}"
REMOTE_NGINX_LINK="${REMOTE_NGINX_LINK:-/etc/nginx/sites-enabled/chatos.conf}"
REMOTE_BACKEND_PORT="${REMOTE_BACKEND_PORT:-13001}"
REMOTE_SERVER_NAME="${REMOTE_SERVER_NAME:-_}"
PLAN_ONLY="${PLAN_ONLY:-0}"
SYNC_ONLY="${SYNC_ONLY:-0}"
SKIP_RESTART="${SKIP_RESTART:-0}"

# 可选：当远端需要 sudo 且不是无密码 sudo 时使用；默认沿用 SSH 密码。
REMOTE_SUDO_PASSWORD="${REMOTE_SUDO_PASSWORD:-${REMOTE_PASSWORD:-}}"

# 统一 target-shared 构建产物路径，兼容当前仓库 .cargo/config.toml。
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target-shared}"
if [[ "$TARGET_DIR" != /* ]]; then
  TARGET_DIR="$ROOT_DIR/$TARGET_DIR"
fi

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

log() { printf '[INFO] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*"; }
err() { printf '[ERROR] %s\n' "$*"; }

need_cmd ssh
need_cmd rsync
need_cmd sshpass
need_cmd bash

if [[ -z "$REMOTE_HOST" ]]; then
  err "请先在脚本顶部或环境变量中填写 REMOTE_HOST"
  exit 1
fi
if [[ -z "$REMOTE_PASSWORD" ]]; then
  err "请先在脚本顶部或环境变量中填写 REMOTE_PASSWORD"
  exit 1
fi

SSH_BASE=(ssh -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=15 -o ServerAliveCountMax=3)
SSHPASS_SSH=(sshpass -p "$REMOTE_PASSWORD" "${SSH_BASE[@]}" "$REMOTE_USER@$REMOTE_HOST")
SSHPASS_SCP=(sshpass -p "$REMOTE_PASSWORD" rsync -a -e "ssh -p $REMOTE_PORT -o StrictHostKeyChecking=accept-new")

remote_run() {
  sshpass -p "$REMOTE_PASSWORD" ssh -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" "$@"
}

remote_run_bash() {
  sshpass -p "$REMOTE_PASSWORD" ssh -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" 'bash -s' --
}

remote_sudo_bash() {
  sshpass -p "$REMOTE_PASSWORD" ssh -tt -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" 'sudo -S -p "" bash -s' <<EOF
$REMOTE_SUDO_PASSWORD
EOF
}

cat <<EOF
[PLAN]
- 本地仓库: $ROOT_DIR
- 远端: ${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_PORT}
- 远端代码目录: $REMOTE_APP_ROOT
- 远端部署根: $REMOTE_DEPLOY_ROOT
- 远端服务: $REMOTE_SERVICE_NAME
- 远端 env: $REMOTE_ENV_DIR/chatos-backend.env
- 远端 nginx: $REMOTE_NGINX_SITE -> $REMOTE_NGINX_LINK
- Rust target-dir: $TARGET_DIR
- 模式: PLAN_ONLY=$PLAN_ONLY SYNC_ONLY=$SYNC_ONLY SKIP_RESTART=$SKIP_RESTART
EOF

if [[ "$PLAN_ONLY" == "1" ]]; then
  log "plan only, stop before any remote changes"
  exit 0
fi

log "检查本地依赖通过：ssh/rsync/sshpass"
log "同步仓库到远端（排除构建产物和敏感 env）"
rsync -az --delete \
  --exclude '.git/' \
  --exclude 'target/' \
  --exclude 'target-shared/' \
  --exclude 'node_modules/' \
  --exclude '.env' \
  --exclude '*.env' \
  --exclude 'logs/' \
  --exclude '.local/' \
  --exclude '.vite/' \
  -e "ssh -p $REMOTE_PORT -o StrictHostKeyChecking=accept-new" \
  "$ROOT_DIR/" "$REMOTE_USER@$REMOTE_HOST:$REMOTE_APP_ROOT/"

if [[ "$SYNC_ONLY" == "1" ]]; then
  log "SYNC_ONLY=1，已完成同步，未执行远端构建/重启"
  exit 0
fi

read -r -d '' REMOTE_SCRIPT <<'REMOTE_EOF' || true
set -euo pipefail

REMOTE_APP_ROOT="__REMOTE_APP_ROOT__"
REMOTE_DEPLOY_ROOT="__REMOTE_DEPLOY_ROOT__"
REMOTE_ENV_DIR="__REMOTE_ENV_DIR__"
REMOTE_SERVICE_NAME="__REMOTE_SERVICE_NAME__"
REMOTE_NGINX_SITE="__REMOTE_NGINX_SITE__"
REMOTE_NGINX_LINK="__REMOTE_NGINX_LINK__"
REMOTE_BACKEND_PORT="__REMOTE_BACKEND_PORT__"
REMOTE_SERVER_NAME="__REMOTE_SERVER_NAME__"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] remote 缺少命令: $cmd"
    exit 1
  fi
}

sudo_run() {
  if sudo -n true >/dev/null 2>&1; then
    sudo "$@"
    return
  fi
  if [[ -z "${REMOTE_SUDO_PASSWORD:-}" ]]; then
    echo "[ERROR] 远端需要 sudo，但未提供 REMOTE_SUDO_PASSWORD"
    exit 1
  fi
  printf '%s\n' "$REMOTE_SUDO_PASSWORD" | sudo -S -p '' "$@"
}

need_cmd bash
need_cmd cargo
need_cmd rsync
need_cmd systemctl
need_cmd nginx

TARGET_DIR="${CARGO_TARGET_DIR:-$REMOTE_APP_ROOT/target-shared}"
if [[ "$TARGET_DIR" != /* ]]; then
  TARGET_DIR="$REMOTE_APP_ROOT/$TARGET_DIR"
fi

log() { printf '[REMOTE] %s\n' "$*"; }
warn() { printf '[REMOTE-WARN] %s\n' "$*"; }

log "远端代码目录: $REMOTE_APP_ROOT"
log "Rust target-dir: $TARGET_DIR"
mkdir -p "$REMOTE_DEPLOY_ROOT" "$REMOTE_ENV_DIR"

cd "$REMOTE_APP_ROOT"

if [[ ! -f "$REMOTE_ENV_DIR/chatos-backend.env" ]]; then
  if [[ -f "$REMOTE_APP_ROOT/deploy/linux/chatos-backend.env.example" ]]; then
    install -m 0640 "$REMOTE_APP_ROOT/deploy/linux/chatos-backend.env.example" "$REMOTE_ENV_DIR/chatos-backend.env"
    warn "已创建 /etc/chatos/chatos-backend.env，但其中敏感值仍需手工补齐/确认，脚本不会覆盖现有敏感 env"
  else
    warn "未找到 env example；请手工创建 $REMOTE_ENV_DIR/chatos-backend.env"
  fi
else
  log "保留已有 /etc/chatos/chatos-backend.env，不覆盖敏感值"
fi

# 仅做最小生产构建：Rust 后端统一使用 target-shared；前端由现有部署流程按需处理。
log "构建 Rust 后端（target-shared）"
cargo build --release --manifest-path "$REMOTE_APP_ROOT/chat_app_server_rs/Cargo.toml" --target-dir "$TARGET_DIR"

# 使用现有的无 Docker 生产安装约定（systemd + nginx + /etc/chatos env）。
if [[ -x "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh" ]]; then
  log "执行现有生产安装脚本（不使用 dev 模式）"
  sudo_run env SOURCE_ROOT="$REMOTE_APP_ROOT" BACKEND_PORT="$REMOTE_BACKEND_PORT" SERVER_NAME="$REMOTE_SERVER_NAME" bash "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh"
else
  warn "缺少 scripts/server-install-nodocker.sh，跳过 systemd/nginx 自动安装"
fi

# 主要服务重部署路径：
# - chatos-backend: systemd + nginx
# - user_service: 依赖 /etc/chatos/chatos-backend.env 内的 USER_SERVICE_* / CHATOS_* env，脚本不覆盖现有敏感值
# - sandbox_manager_service: 仅在 Docker backend 约定下保留 SANDBOX_MANAGER_BACKEND=docker / SANDBOX_MANAGER_DOCKER_IMAGE / SANDBOX_MANAGER_DOCKER_NETWORK / SANDBOX_MANAGER_IMAGE_DOCKERFILE 等关键 env；如为 Docker 降级模式，需要确保 docker 已安装且沙箱镜像可构建
if [[ -x "$REMOTE_APP_ROOT/restart_services_prod.sh" ]]; then
  log "调用仓库内生产重启包装脚本（保持 target-shared 约定）"
  bash "$REMOTE_APP_ROOT/restart_services_prod.sh" >/tmp/chatos_remote_restart_prod.log 2>&1 || {
    warn "仓库内生产重启脚本返回非 0，继续进行状态检查"
    tail -n 80 /tmp/chatos_remote_restart_prod.log || true
  }
fi

if [[ "$REMOTE_SERVICE_NAME" == "chatos-backend" ]]; then
  sudo_run systemctl daemon-reload
  sudo_run systemctl enable --now "$REMOTE_SERVICE_NAME"
  sudo_run systemctl restart "$REMOTE_SERVICE_NAME"
  sudo_run systemctl reload nginx
fi

log "最小健康检查"
if command -v curl >/dev/null 2>&1; then
  curl -fsS "http://127.0.0.1:${REMOTE_BACKEND_PORT}/health" >/dev/null
  log "health ok: http://127.0.0.1:${REMOTE_BACKEND_PORT}/health"
else
  warn "curl 不存在，跳过 health 检查"
fi

log "状态输出"
sudo_run systemctl --no-pager --full status "$REMOTE_SERVICE_NAME" | sed -n '1,12p' || true
sudo_run nginx -t || true
REMOTE_EOF

REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_APP_ROOT__/$REMOTE_APP_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_DEPLOY_ROOT__/$REMOTE_DEPLOY_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_ENV_DIR__/$REMOTE_ENV_DIR}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVICE_NAME__/$REMOTE_SERVICE_NAME}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_NGINX_SITE__/$REMOTE_NGINX_SITE}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_NGINX_LINK__/$REMOTE_NGINX_LINK}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_BACKEND_PORT__/$REMOTE_BACKEND_PORT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVER_NAME__/$REMOTE_SERVER_NAME}"

log "执行远端更新与部署"
sshpass -p "$REMOTE_PASSWORD" ssh -tt -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" \
  "REMOTE_SUDO_PASSWORD=$(printf '%q' "$REMOTE_SUDO_PASSWORD") bash -s" <<EOF
$REMOTE_SCRIPT
EOF

log "完成：可在远端查看 $REMOTE_SERVICE_NAME、nginx 和 /etc/chatos/chatos-backend.env"
log "提示：若 /etc/chatos/chatos-backend.env 刚创建，请手工补齐敏感 env 后再重跑一次"
