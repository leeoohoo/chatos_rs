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
REMOTE_CHATOS_WORKSPACE_DIR="${REMOTE_CHATOS_WORKSPACE_DIR:-}"
REMOTE_STAGE_DIR="${REMOTE_STAGE_DIR:-/tmp/chatos_rs_deploy_staging}"
REMOTE_REBUILD_AUX_SERVICES="${REMOTE_REBUILD_AUX_SERVICES:-1}"
REMOTE_REBUILD_OFFICIAL_WEBSITE="${REMOTE_REBUILD_OFFICIAL_WEBSITE:-1}"
REMOTE_REBUILD_DB_CONNECTION_HUB="${REMOTE_REBUILD_DB_CONNECTION_HUB:-1}"
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

SSH_OPTIONS=(
  -p "$REMOTE_PORT"
  -o StrictHostKeyChecking=accept-new
  -o PreferredAuthentications=password
  -o PubkeyAuthentication=no
  -o NumberOfPasswordPrompts=1
)
RSYNC_SSH="ssh -p $REMOTE_PORT -o StrictHostKeyChecking=accept-new -o PreferredAuthentications=password -o PubkeyAuthentication=no -o NumberOfPasswordPrompts=1"

remote_run() {
  sshpass -p "$REMOTE_PASSWORD" ssh "${SSH_OPTIONS[@]}" "$REMOTE_USER@$REMOTE_HOST" "$@"
}

remote_run_bash() {
  sshpass -p "$REMOTE_PASSWORD" ssh "${SSH_OPTIONS[@]}" "$REMOTE_USER@$REMOTE_HOST" 'bash -s' --
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
- 远端 workspace: ${REMOTE_CHATOS_WORKSPACE_DIR:-auto}
- 远端 nginx: /etc/nginx/sites-available/chatos.conf -> /etc/nginx/sites-enabled/chatos.conf
- Rust target-dir: $TARGET_DIR
- 更新范围: 主服务 + 附属服务(REMOTE_REBUILD_AUX_SERVICES=$REMOTE_REBUILD_AUX_SERVICES, OFFICIAL=$REMOTE_REBUILD_OFFICIAL_WEBSITE, DB_HUB=$REMOTE_REBUILD_DB_CONNECTION_HUB)
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
  -e "$RSYNC_SSH" \
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
REMOTE_CHATOS_WORKSPACE_DIR="__REMOTE_CHATOS_WORKSPACE_DIR__"
REMOTE_REBUILD_AUX_SERVICES="__REMOTE_REBUILD_AUX_SERVICES__"
REMOTE_REBUILD_OFFICIAL_WEBSITE="__REMOTE_REBUILD_OFFICIAL_WEBSITE__"
REMOTE_REBUILD_DB_CONNECTION_HUB="__REMOTE_REBUILD_DB_CONNECTION_HUB__"

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

env_bool() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

env_file_value() {
  local key="$1"
  local file="$2"
  if [[ ! -f "$file" ]]; then
    return 0
  fi
  awk -F= -v key="$key" '
    $1 == key {
      sub(/^[^=]*=/, "")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "")
      print
      exit
    }
  ' "$file"
}

load_chatos_env() {
  local env_file="/etc/chatos/chatos-backend.env"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

mongo_url_for() {
  local database="$1"
  local host="${MONGODB_HOST:-127.0.0.1}"
  local port="${MONGODB_PORT:-27018}"
  local user="${MONGODB_USER:-admin}"
  local password="${MONGODB_PASSWORD:-admin}"
  local auth_source="${MONGODB_AUTH_SOURCE:-admin}"
  printf 'mongodb://%s:%s@%s:%s/%s?authSource=%s' "$user" "$password" "$host" "$port" "$database" "$auth_source"
}

mongo_admin_uri() {
  local host="${MONGODB_HOST:-127.0.0.1}"
  local port="${MONGODB_PORT:-27018}"
  local user="${MONGODB_USER:-admin}"
  local password="${MONGODB_PASSWORD:-admin}"
  local auth_source="${MONGODB_AUTH_SOURCE:-admin}"
  printf 'mongodb://%s:%s@%s:%s/%s' "$user" "$password" "$host" "$port" "$auth_source"
}

install_frontend_deps() {
  local dir="$1"
  local label="$2"
  if [[ ! -f "$dir/package.json" ]]; then
    return 0
  fi
  log "安装/刷新 ${label} 前端依赖"
  if [[ -f "$dir/package-lock.json" ]]; then
    if ! npm --prefix "$dir" ci; then
      warn "${label} npm ci 失败，回退到 npm install 以刷新不匹配的 lockfile"
      npm --prefix "$dir" install
    fi
  else
    npm --prefix "$dir" install
  fi
}

prepare_aux_service_env() {
  load_chatos_env

  export MAIN_BACKEND_PORT="${EFFECTIVE_BACKEND_PORT}"
  export BACKEND_PORT="${EFFECTIVE_BACKEND_PORT}"
  export START_DEV_MONGO="${START_DEV_MONGO:-auto}"

  export USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
  export USER_SERVICE_FRONTEND_PORT="${USER_SERVICE_FRONTEND_PORT:-39191}"
  export CHATOS_USER_SERVICE_BASE_URL="${CHATOS_USER_SERVICE_BASE_URL:-http://127.0.0.1:${USER_SERVICE_PORT}}"
  export USER_SERVICE_BASE_URL="${USER_SERVICE_BASE_URL:-$CHATOS_USER_SERVICE_BASE_URL}"
  export USER_SERVICE_DATABASE_URL="${USER_SERVICE_DATABASE_URL:-$(mongo_url_for user_service)}"

  export MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
  export MEMORY_ENGINE_FRONTEND_PORT="${MEMORY_ENGINE_FRONTEND_PORT:-4178}"
  export MEMORY_ENGINE_BASE_URL="${MEMORY_ENGINE_BASE_URL:-http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1}"
  export MEMORY_ENGINE_MONGODB_DATABASE="${MEMORY_ENGINE_MONGODB_DATABASE:-memory_engine}"
  export MEMORY_ENGINE_MONGODB_URI="${MEMORY_ENGINE_MONGODB_URI:-$(mongo_admin_uri)}"
  export MEMORY_ENGINE_OPERATOR_TOKEN="${MEMORY_ENGINE_OPERATOR_TOKEN:-${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-chatos-memory-engine-prod-operator-token}}"
  export TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN="${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-$MEMORY_ENGINE_OPERATOR_TOKEN}"

  export PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
  export PROJECT_SERVICE_FRONTEND_PORT="${PROJECT_SERVICE_FRONTEND_PORT:-39211}"
  export PROJECT_SERVICE_BASE_URL="${PROJECT_SERVICE_BASE_URL:-http://127.0.0.1:${PROJECT_SERVICE_PORT}}"
  export CHATOS_PROJECT_SERVICE_BASE_URL="${CHATOS_PROJECT_SERVICE_BASE_URL:-$PROJECT_SERVICE_BASE_URL}"
  export PROJECT_SERVICE_DATABASE_URL="${PROJECT_SERVICE_DATABASE_URL:-$(mongo_url_for project_management_service)}"
  export PROJECT_SERVICE_SYNC_SECRET="${PROJECT_SERVICE_SYNC_SECRET:-${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-$PROJECT_SERVICE_SYNC_SECRET}"

  export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}"
  export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-$TASK_RUNNER_BACKEND_PORT}"
  export TASK_RUNNER_FRONTEND_PORT="${TASK_RUNNER_FRONTEND_PORT:-39091}"
  export TASK_RUNNER_BASE_URL="${TASK_RUNNER_BASE_URL:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
  export CHATOS_TASK_RUNNER_BASE_URL="${CHATOS_TASK_RUNNER_BASE_URL:-$TASK_RUNNER_BASE_URL}"
  export TASK_RUNNER_DATABASE_URL="${TASK_RUNNER_DATABASE_URL:-$(mongo_url_for task_runner_service)}"
  export TASK_RUNNER_CHATOS_CALLBACK_URL="${TASK_RUNNER_CHATOS_CALLBACK_URL:-http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/api/agent/chat/task-runner/callback}"
  export TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-change_me_chatos_task_runner_secret}}"
  export CHATOS_TASK_RUNNER_CALLBACK_SECRET="${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-$TASK_RUNNER_CHATOS_CALLBACK_SECRET}"

  export SANDBOX_MANAGER_PORT="${SANDBOX_MANAGER_PORT:-8095}"
  export SANDBOX_MANAGER_FRONTEND_PORT="${SANDBOX_MANAGER_FRONTEND_PORT:-8096}"

  export OFFICIAL_WEBSITE_MODE="${OFFICIAL_WEBSITE_MODE:-prod}"
  export OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-39250}"
  export OFFICIAL_WEBSITE_FRONTEND_PORT="${OFFICIAL_WEBSITE_FRONTEND_PORT:-39251}"

  export DB_HUB_BACKEND_PORT="${DB_HUB_BACKEND_PORT:-8099}"
  export DB_HUB_FRONTEND_PORT="${DB_HUB_FRONTEND_PORT:-5174}"
}

rebuild_aux_services() {
  if ! env_bool "$REMOTE_REBUILD_AUX_SERVICES"; then
    log "跳过附属服务重建 (REMOTE_REBUILD_AUX_SERVICES=$REMOTE_REBUILD_AUX_SERVICES)"
    return 0
  fi

  prepare_aux_service_env

  install_frontend_deps "$REMOTE_APP_ROOT/memory_engine/frontend" "memory_engine"
  install_frontend_deps "$REMOTE_APP_ROOT/user_service/frontend" "user_service"
  install_frontend_deps "$REMOTE_APP_ROOT/project_management_service/frontend" "project_management_service"
  install_frontend_deps "$REMOTE_APP_ROOT/task_runner_service/frontend" "task_runner_service"
  install_frontend_deps "$REMOTE_APP_ROOT/sandbox_manager_service/frontend" "sandbox_manager_service"

  if env_bool "$REMOTE_REBUILD_OFFICIAL_WEBSITE"; then
    install_frontend_deps "$REMOTE_APP_ROOT/official_website_service/frontend" "official_website_service"
    log "构建 official_website_service 前端静态文件"
    npm --prefix "$REMOTE_APP_ROOT/official_website_service/frontend" run build
  fi

  log "重建/重启附属服务（memory/user/project/sandbox/task/official）"
  START_CHATOS=0 \
    START_MEMORY_ENGINE=1 \
    START_USER_SERVICE=1 \
    START_PROJECT_MANAGEMENT=1 \
    START_SANDBOX_MANAGER=1 \
    START_TASK_RUNNER=1 \
    START_OFFICIAL_WEBSITE="$REMOTE_REBUILD_OFFICIAL_WEBSITE" \
    "$REMOTE_APP_ROOT/restart_all_services.sh" restart

  if env_bool "$REMOTE_REBUILD_DB_CONNECTION_HUB" && [[ -x "$REMOTE_APP_ROOT/db_connection_hub/restart_services.sh" ]]; then
    install_frontend_deps "$REMOTE_APP_ROOT/db_connection_hub/frontend" "db_connection_hub"
    log "重建/重启 db_connection_hub"
    "$REMOTE_APP_ROOT/db_connection_hub/restart_services.sh" restart
  fi
}

ENV_PREEXISTED=0
EFFECTIVE_BACKEND_PORT="$REMOTE_BACKEND_PORT"
EFFECTIVE_CHATOS_WORKSPACE_DIR="$REMOTE_CHATOS_WORKSPACE_DIR"
if [[ -f "/etc/chatos/chatos-backend.env" ]]; then
  ENV_PREEXISTED=1
  log "保留已有 /etc/chatos/chatos-backend.env，不覆盖敏感值"
  EXISTING_BACKEND_PORT="$(env_file_value BACKEND_PORT /etc/chatos/chatos-backend.env)"
  if [[ -n "$EXISTING_BACKEND_PORT" ]]; then
    EFFECTIVE_BACKEND_PORT="$EXISTING_BACKEND_PORT"
    if [[ "$REMOTE_BACKEND_PORT" != "$EXISTING_BACKEND_PORT" ]]; then
      warn "检测到已有 env 中 BACKEND_PORT=$EXISTING_BACKEND_PORT；为避免破坏现网，将忽略传入的 REMOTE_BACKEND_PORT=$REMOTE_BACKEND_PORT。若需改端口，请先手工修改 /etc/chatos/chatos-backend.env 后再重跑。"
    fi
  fi
  EXISTING_CHATOS_WORKSPACE_DIR="$(env_file_value CHATOS_WORKSPACE_DIR /etc/chatos/chatos-backend.env)"
  if [[ -n "$EXISTING_CHATOS_WORKSPACE_DIR" ]]; then
    EFFECTIVE_CHATOS_WORKSPACE_DIR="$EXISTING_CHATOS_WORKSPACE_DIR"
    if [[ -n "$REMOTE_CHATOS_WORKSPACE_DIR" && "$REMOTE_CHATOS_WORKSPACE_DIR" != "$EXISTING_CHATOS_WORKSPACE_DIR" ]]; then
      warn "检测到已有 env 中 CHATOS_WORKSPACE_DIR=$EXISTING_CHATOS_WORKSPACE_DIR；将保留现网值，忽略 REMOTE_CHATOS_WORKSPACE_DIR=$REMOTE_CHATOS_WORKSPACE_DIR。若需迁移目录，请先手工修改 /etc/chatos/chatos-backend.env。"
    fi
  fi
else
  warn "首次部署将由现有安装脚本生成 /etc/chatos/chatos-backend.env；完成后请手工确认其中敏感值"
fi

if [[ -z "$EFFECTIVE_CHATOS_WORKSPACE_DIR" ]]; then
  EFFECTIVE_CHATOS_WORKSPACE_DIR="$REMOTE_DEPLOY_ROOT/backend/data/workspace"
fi
if [[ "$EFFECTIVE_CHATOS_WORKSPACE_DIR" != /* ]]; then
  EFFECTIVE_CHATOS_WORKSPACE_DIR="$REMOTE_DEPLOY_ROOT/$EFFECTIVE_CHATOS_WORKSPACE_DIR"
fi

log "远端代码目录: $REMOTE_APP_ROOT"
log "Rust target-dir: $TARGET_DIR"
log "Chatos workspace: $EFFECTIVE_CHATOS_WORKSPACE_DIR"
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
    CHATOS_WORKSPACE_DIR="$EFFECTIVE_CHATOS_WORKSPACE_DIR" \
    bash "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh"
else
  warn "缺少 scripts/server-install-nodocker.sh，跳过 systemd/nginx 自动安装"
fi

if [[ "$ENV_PREEXISTED" == "0" ]]; then
  warn "首次部署已生成 /etc/chatos/chatos-backend.env；如启用 user_service，请手工补齐 CHATOS_ADMIN_PASSWORD / USER_SERVICE_SUPER_ADMIN_PASSWORD 等敏感值后再重跑或重启相关服务"
fi

rebuild_aux_services

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
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CHATOS_WORKSPACE_DIR__/$REMOTE_CHATOS_WORKSPACE_DIR}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_AUX_SERVICES__/$REMOTE_REBUILD_AUX_SERVICES}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_OFFICIAL_WEBSITE__/$REMOTE_REBUILD_OFFICIAL_WEBSITE}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_DB_CONNECTION_HUB__/$REMOTE_REBUILD_DB_CONNECTION_HUB}"

log "执行远端更新与部署"
sshpass -p "$REMOTE_PASSWORD" ssh -T "${SSH_OPTIONS[@]}" "$REMOTE_USER@$REMOTE_HOST" \
  "REMOTE_SUDO_PASSWORD=$(printf '%q' "$REMOTE_SUDO_PASSWORD") bash -s" <<EOF
$REMOTE_SCRIPT
EOF

log "完成：可在远端查看 $REMOTE_SERVICE_NAME、nginx 和 /etc/chatos/chatos-backend.env"
log "说明：已同步并重建主服务；REMOTE_REBUILD_AUX_SERVICES=1 时也会重建 memory/user/project/sandbox/task/official/db_connection_hub。"
log "提示：若首次生成了 /etc/chatos/chatos-backend.env，请手工确认其中敏感值；如启用 sandbox_manager_service 的 Docker 降级模式，还需确认 SANDBOX_MANAGER_BACKEND=docker、SANDBOX_MANAGER_DOCKER_IMAGE、SANDBOX_MANAGER_DOCKER_NETWORK、SANDBOX_MANAGER_IMAGE_BUILD_CONTEXT、SANDBOX_MANAGER_IMAGE_DOCKERFILE 以及 docker daemon 可用"
