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
REMOTE_SERVICE_NAME="${REMOTE_SERVICE_NAME:-chatos-backend}"
REMOTE_BACKEND_PORT="${REMOTE_BACKEND_PORT:-13001}"
REMOTE_SERVER_NAME="${REMOTE_SERVER_NAME:-_}"
REMOTE_STAGE_DIR="${REMOTE_STAGE_DIR:-/tmp/chatos_rs_deploy_staging}"
PLAN_ONLY="${PLAN_ONLY:-0}"
SYNC_ONLY="${SYNC_ONLY:-0}"

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
if [[ "$REMOTE_APP_ROOT" == "$REMOTE_DEPLOY_ROOT" ]]; then
  err "REMOTE_APP_ROOT 不能与 REMOTE_DEPLOY_ROOT 相同；建议保持源码目录独立于生产部署根目录"
  exit 1
fi

remote_run() {
  sshpass -p "$REMOTE_PASSWORD" ssh -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" "$@"
}

remote_run_bash() {
  sshpass -p "$REMOTE_PASSWORD" ssh -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" 'bash -s' --
}

cat <<EOF
[PLAN]
- 本地仓库: $ROOT_DIR
- 远端: ${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_PORT}
- 远端同步暂存目录: $REMOTE_STAGE_DIR
- 远端代码目录: $REMOTE_APP_ROOT
- 远端部署根: $REMOTE_DEPLOY_ROOT
- 远端服务: $REMOTE_SERVICE_NAME
- 远端 env: /etc/chatos/chatos-backend.env
- 远端 nginx: /etc/nginx/sites-available/chatos.conf -> /etc/nginx/sites-enabled/chatos.conf
- Rust target-dir: $TARGET_DIR
- 模式: PLAN_ONLY=$PLAN_ONLY SYNC_ONLY=$SYNC_ONLY
EOF

if [[ "$PLAN_ONLY" == "1" ]]; then
  log "plan only, stop before any remote changes"
  exit 0
fi

log "检查本地依赖通过：ssh/rsync/sshpass"
log "准备远端同步暂存目录"
remote_run "mkdir -p $(printf '%q' "$REMOTE_STAGE_DIR")"

log "同步仓库到远端暂存目录（排除构建产物和敏感 env）"
sshpass -p "$REMOTE_PASSWORD" rsync -az --delete \
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
  "$ROOT_DIR/" "$REMOTE_USER@$REMOTE_HOST:$REMOTE_STAGE_DIR/"

if [[ "$SYNC_ONLY" == "1" ]]; then
  log "SYNC_ONLY=1，已完成同步，未执行远端构建/重启"
  exit 0
fi

read -r -d '' REMOTE_SCRIPT <<'REMOTE_EOF' || true
set -euo pipefail

REMOTE_STAGE_DIR="__REMOTE_STAGE_DIR__"
REMOTE_APP_ROOT="__REMOTE_APP_ROOT__"
REMOTE_DEPLOY_ROOT="__REMOTE_DEPLOY_ROOT__"
REMOTE_SERVICE_NAME="__REMOTE_SERVICE_NAME__"
REMOTE_BACKEND_PORT="__REMOTE_BACKEND_PORT__"
REMOTE_SERVER_NAME="__REMOTE_SERVER_NAME__"

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

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
need_cmd npm
need_cmd rsync
need_cmd systemctl
need_cmd nginx

TARGET_DIR="${CARGO_TARGET_DIR:-$REMOTE_APP_ROOT/target-shared}"
if [[ "$TARGET_DIR" != /* ]]; then
  TARGET_DIR="$REMOTE_APP_ROOT/$TARGET_DIR"
fi

log() { printf '[REMOTE] %s\n' "$*"; }
warn() { printf '[REMOTE-WARN] %s\n' "$*"; }

ENV_PREEXISTED=0
EFFECTIVE_BACKEND_PORT="$REMOTE_BACKEND_PORT"
if [[ -f "/etc/chatos/chatos-backend.env" ]]; then
  ENV_PREEXISTED=1
  log "保留已有 /etc/chatos/chatos-backend.env，不覆盖敏感值"
  EXISTING_BACKEND_PORT="$(awk -F= '$1=="BACKEND_PORT" {gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2); print $2; exit}' /etc/chatos/chatos-backend.env)"
  if [[ -n "$EXISTING_BACKEND_PORT" ]]; then
    EFFECTIVE_BACKEND_PORT="$EXISTING_BACKEND_PORT"
    if [[ "$REMOTE_BACKEND_PORT" != "$EXISTING_BACKEND_PORT" ]]; then
      warn "检测到已有 env 中 BACKEND_PORT=$EXISTING_BACKEND_PORT；为避免破坏现网，将忽略传入的 REMOTE_BACKEND_PORT=$REMOTE_BACKEND_PORT。若需改端口，请先手工修改 /etc/chatos/chatos-backend.env 后再重跑。"
    fi
  fi
else
  warn "首次部署将由现有安装脚本生成 /etc/chatos/chatos-backend.env；完成后请手工确认其中敏感值"
fi

log "远端代码目录: $REMOTE_APP_ROOT"
log "Rust target-dir: $TARGET_DIR"
sudo_run mkdir -p "$REMOTE_DEPLOY_ROOT" "$REMOTE_APP_ROOT" /etc/chatos
sudo_run chown -R "$(id -un)":"$(id -gn)" "$REMOTE_APP_ROOT"

log "从远端暂存目录同步到正式代码目录"
rsync -a --delete \
  --exclude '.env' \
  --exclude '*.env' \
  --exclude 'target/' \
  --exclude 'target-shared/' \
  --exclude 'node_modules/' \
  --exclude 'logs/' \
  --exclude '.local/' \
  --exclude '.vite/' \
  "$REMOTE_STAGE_DIR/" "$REMOTE_APP_ROOT/"

cd "$REMOTE_APP_ROOT"

if command -v rustup >/dev/null 2>&1; then
  ACTIVE_TOOLCHAIN="$(cd "$REMOTE_APP_ROOT" && rustup show active-toolchain 2>/dev/null | awk 'NR==1 {print $1}')"
  if [[ -n "$ACTIVE_TOOLCHAIN" ]]; then
    export RUSTUP_TOOLCHAIN="$ACTIVE_TOOLCHAIN"
  fi
fi

# 生产前端需要 dist；不走 npm run dev。
log "构建 chat_app 前端生产 dist"
if [[ -f "$REMOTE_APP_ROOT/chat_app/package-lock.json" ]]; then
  npm --prefix "$REMOTE_APP_ROOT/chat_app" ci
else
  npm --prefix "$REMOTE_APP_ROOT/chat_app" install
fi
npm --prefix "$REMOTE_APP_ROOT/chat_app" run build

# 仅做最小生产构建：Rust 后端统一使用 target-shared；前端由现有部署流程按需处理。
log "构建 Rust 后端（target-shared）"
cargo build --release --manifest-path "$REMOTE_APP_ROOT/chat_app_server_rs/Cargo.toml" --target-dir "$TARGET_DIR"

# 使用现有的无 Docker 生产安装约定（systemd + nginx + /etc/chatos env）。
if [[ -x "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh" ]]; then
  log "执行现有生产安装脚本（不使用 dev 模式）"
  sudo_run env \
    SOURCE_ROOT="$REMOTE_APP_ROOT" \
    APP_ROOT="$REMOTE_DEPLOY_ROOT" \
    SERVICE_NAME="$REMOTE_SERVICE_NAME" \
    BACKEND_PORT="$EFFECTIVE_BACKEND_PORT" \
    SERVER_NAME="$REMOTE_SERVER_NAME" \
    CARGO_TARGET_DIR="$TARGET_DIR" \
    bash "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh"
else
  warn "缺少 scripts/server-install-nodocker.sh，跳过 systemd/nginx 自动安装"
fi

if [[ "$ENV_PREEXISTED" == "0" ]]; then
  warn "首次部署已生成 /etc/chatos/chatos-backend.env；如启用 user_service，请手工补齐 CHATOS_ADMIN_PASSWORD / USER_SERVICE_SUPER_ADMIN_PASSWORD 等敏感值后再重跑或重启相关服务"
fi

# 主要服务重部署路径：
# - chatos-backend: systemd + nginx
# - user_service: 依赖 /etc/chatos/chatos-backend.env 内的 USER_SERVICE_* / CHATOS_* env，脚本不覆盖现有敏感值
# - sandbox_manager_service: 仅在 Docker backend 约定下保留 SANDBOX_MANAGER_BACKEND=docker / SANDBOX_MANAGER_DOCKER_IMAGE / SANDBOX_MANAGER_DOCKER_NETWORK / SANDBOX_MANAGER_IMAGE_DOCKERFILE 等关键 env；如为 Docker 降级模式，需要确保 docker 已安装且沙箱镜像可构建

log "最小健康检查"
if command -v curl >/dev/null 2>&1; then
  curl -fsS "http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/health" >/dev/null
  log "health ok: http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/health"
else
  warn "curl 不存在，跳过 health 检查"
fi

log "状态输出"
sudo_run systemctl --no-pager --full status "$REMOTE_SERVICE_NAME" | sed -n '1,12p' || true
sudo_run nginx -t || true
REMOTE_EOF

REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_STAGE_DIR__/$REMOTE_STAGE_DIR}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_APP_ROOT__/$REMOTE_APP_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_DEPLOY_ROOT__/$REMOTE_DEPLOY_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVICE_NAME__/$REMOTE_SERVICE_NAME}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_BACKEND_PORT__/$REMOTE_BACKEND_PORT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVER_NAME__/$REMOTE_SERVER_NAME}"

log "执行远端更新与部署"
sshpass -p "$REMOTE_PASSWORD" ssh -tt -p "$REMOTE_PORT" -o StrictHostKeyChecking=accept-new "$REMOTE_USER@$REMOTE_HOST" \
  "REMOTE_SUDO_PASSWORD=$(printf '%q' "$REMOTE_SUDO_PASSWORD") bash -s" <<EOF
$REMOTE_SCRIPT
EOF

log "完成：可在远端查看 $REMOTE_SERVICE_NAME、nginx 和 /etc/chatos/chatos-backend.env"
log "提示：若首次生成了 /etc/chatos/chatos-backend.env，请手工确认其中敏感值；如启用 sandbox_manager_service 的 Docker 降级模式，还需确认 SANDBOX_MANAGER_BACKEND=docker、SANDBOX_MANAGER_DOCKER_IMAGE、SANDBOX_MANAGER_DOCKER_NETWORK、SANDBOX_MANAGER_IMAGE_BUILD_CONTEXT、SANDBOX_MANAGER_IMAGE_DOCKERFILE 以及 docker daemon 可用"
