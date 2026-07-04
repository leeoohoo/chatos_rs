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
REMOTE_DEPLOY_SERVICES="${REMOTE_DEPLOY_SERVICES:-all}"
REMOTE_REBUILD_AUX_SERVICES="${REMOTE_REBUILD_AUX_SERVICES:-1}"
REMOTE_REBUILD_OFFICIAL_WEBSITE="${REMOTE_REBUILD_OFFICIAL_WEBSITE:-1}"
REMOTE_REBUILD_DB_CONNECTION_HUB="${REMOTE_REBUILD_DB_CONNECTION_HUB:-0}"
REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC="${REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC:-180}"
REMOTE_NPM_INSTALL_MODE="${REMOTE_NPM_INSTALL_MODE:-install}"
REMOTE_CLEAN_TARGET="${REMOTE_CLEAN_TARGET:-1}"
REMOTE_CARGO_RELEASE_LTO="${REMOTE_CARGO_RELEASE_LTO:-false}"
REMOTE_CARGO_RELEASE_CODEGEN_UNITS="${REMOTE_CARGO_RELEASE_CODEGEN_UNITS:-16}"
REMOTE_CARGO_BUILD_JOBS="${REMOTE_CARGO_BUILD_JOBS:-}"
REMOTE_ENABLE_PROCESS_ISOLATION="${REMOTE_ENABLE_PROCESS_ISOLATION:-1}"
REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE="${REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE:-capabilities}"
REMOTE_PROCESS_ISOLATION_FS_ENABLED="${REMOTE_PROCESS_ISOLATION_FS_ENABLED:-true}"
REMOTE_PROCESS_ISOLATION_FS_ROOT="${REMOTE_PROCESS_ISOLATION_FS_ROOT:-/tmp/chatos-process-isolation}"
REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC="${REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC:-false}"
PLAN_ONLY="${PLAN_ONLY:-0}"
SYNC_ONLY="${SYNC_ONLY:-0}"

# Optional deployment scope. Default "all" preserves the historical full deploy.
# Examples:
#   REMOTE_DEPLOY_SERVICES=main              # chat_app + chat_app_server_rs + nginx install
#   REMOTE_DEPLOY_SERVICES=task-runner       # task runner backend/frontend only
#   REMOTE_DEPLOY_SERVICES=project-management
#   REMOTE_DEPLOY_SERVICES=memory-engine
#   REMOTE_DEPLOY_SERVICES=user-service
#   REMOTE_DEPLOY_SERVICES=sandbox-manager
#   REMOTE_DEPLOY_SERVICES=nginx             # render/reload nginx only
# Multiple services can be comma-separated, for example:
#   REMOTE_DEPLOY_SERVICES=task-runner,project-management

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
  -o ServerAliveInterval=30
  -o ServerAliveCountMax=3
  -o ConnectTimeout=15
)
RSYNC_SSH="ssh -p $REMOTE_PORT -o StrictHostKeyChecking=accept-new -o PreferredAuthentications=password -o PubkeyAuthentication=no -o NumberOfPasswordPrompts=1 -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -o ConnectTimeout=15"

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
- 本地 Rust target-dir: $TARGET_DIR
- 远端 Rust target-dir: $REMOTE_APP_ROOT/target-shared
- 更新范围: REMOTE_DEPLOY_SERVICES=$REMOTE_DEPLOY_SERVICES (all/main/user-service/memory-engine/project-management/task-runner/sandbox-manager/nginx/db-hub)
- 附属服务: REMOTE_REBUILD_AUX_SERVICES=$REMOTE_REBUILD_AUX_SERVICES, OFFICIAL=$REMOTE_REBUILD_OFFICIAL_WEBSITE, DB_HUB=$REMOTE_REBUILD_DB_CONNECTION_HUB
- npm 安装模式: $REMOTE_NPM_INSTALL_MODE
- 清理远端 Rust target: ${REMOTE_CLEAN_TARGET} (1=全量重编译，慢但省空间)
- Rust release 编译参数: lto=$REMOTE_CARGO_RELEASE_LTO codegen-units=$REMOTE_CARGO_RELEASE_CODEGEN_UNITS jobs=${REMOTE_CARGO_BUILD_JOBS:-auto}
- OS 用户级进程隔离: $REMOTE_ENABLE_PROCESS_ISOLATION ($REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE)
- FS view isolation: $REMOTE_PROCESS_ISOLATION_FS_ENABLED root=$REMOTE_PROCESS_ISOLATION_FS_ROOT proc=$REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC
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
  --exclude 'target-*/' \
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
REMOTE_DEPLOY_SERVICES="__REMOTE_DEPLOY_SERVICES__"
REMOTE_REBUILD_AUX_SERVICES="__REMOTE_REBUILD_AUX_SERVICES__"
REMOTE_REBUILD_OFFICIAL_WEBSITE="__REMOTE_REBUILD_OFFICIAL_WEBSITE__"
REMOTE_REBUILD_DB_CONNECTION_HUB="__REMOTE_REBUILD_DB_CONNECTION_HUB__"
REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC="__REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC__"
REMOTE_NPM_INSTALL_MODE="__REMOTE_NPM_INSTALL_MODE__"
REMOTE_CLEAN_TARGET="__REMOTE_CLEAN_TARGET__"
REMOTE_CARGO_RELEASE_LTO="__REMOTE_CARGO_RELEASE_LTO__"
REMOTE_CARGO_RELEASE_CODEGEN_UNITS="__REMOTE_CARGO_RELEASE_CODEGEN_UNITS__"
REMOTE_CARGO_BUILD_JOBS="__REMOTE_CARGO_BUILD_JOBS__"
REMOTE_ENABLE_PROCESS_ISOLATION="__REMOTE_ENABLE_PROCESS_ISOLATION__"
REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE="__REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE__"
REMOTE_PROCESS_ISOLATION_FS_ENABLED="__REMOTE_PROCESS_ISOLATION_FS_ENABLED__"
REMOTE_PROCESS_ISOLATION_FS_ROOT="__REMOTE_PROCESS_ISOLATION_FS_ROOT__"
REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC="__REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC__"
export REMOTE_NPM_INSTALL_MODE

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

export CARGO_PROFILE_RELEASE_LTO="$REMOTE_CARGO_RELEASE_LTO"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$REMOTE_CARGO_RELEASE_CODEGEN_UNITS"
if [[ -z "$REMOTE_CARGO_BUILD_JOBS" ]] && command -v nproc >/dev/null 2>&1; then
  REMOTE_CARGO_BUILD_JOBS="$(nproc)"
fi
if [[ -n "$REMOTE_CARGO_BUILD_JOBS" ]]; then
  export CARGO_BUILD_JOBS="$REMOTE_CARGO_BUILD_JOBS"
fi

log() { printf '[REMOTE] %s\n' "$*"; }
warn() { printf '[REMOTE-WARN] %s\n' "$*"; }

canonical_deploy_service() {
  local service
  service="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr '_' '-')"
  case "$service" in
    ""|all|full) printf 'all\n' ;;
    main|backend|chat|chat-app|chat-app-server|chatos|chatos-backend) printf 'main\n' ;;
    nginx|gateway|reverse-proxy) printf 'nginx\n' ;;
    aux|auxiliary|aux-services) printf 'aux\n' ;;
    user|user-service|user-service-backend) printf 'user-service\n' ;;
    memory|memory-engine|memory-engine-backend) printf 'memory-engine\n' ;;
    project|project-management|project-management-service|project-service) printf 'project-management\n' ;;
    task|task-runner|task-runner-service) printf 'task-runner\n' ;;
    sandbox|sandbox-manager|sandbox-manager-service) printf 'sandbox-manager\n' ;;
    official|official-website|official-website-service) printf 'official-website\n' ;;
    db|db-hub|db-connection-hub|db_connection_hub) printf 'db-hub\n' ;;
    *) printf '%s\n' "$service" ;;
  esac
}

deployment_is_full() {
  local raw="${REMOTE_DEPLOY_SERVICES:-all}"
  raw="${raw// /}"
  [[ -z "$raw" || "$(canonical_deploy_service "$raw")" == "all" ]]
}

service_selected() {
  local needle="$1"
  if deployment_is_full; then
    return 0
  fi
  local raw="${REMOTE_DEPLOY_SERVICES// /}"
  local item canonical
  IFS=',' read -r -a items <<< "$raw"
  for item in "${items[@]}"; do
    canonical="$(canonical_deploy_service "$item")"
    if [[ "$canonical" == "$needle" ]]; then
      return 0
    fi
    if [[ "$canonical" == "aux" ]]; then
      case "$needle" in
        user-service|memory-engine|project-management|task-runner|sandbox-manager|official-website|db-hub)
          return 0
          ;;
      esac
    fi
  done
  return 1
}

any_service_selected() {
  local service
  for service in "$@"; do
    if service_selected "$service"; then
      return 0
    fi
  done
  return 1
}

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

clean_remote_target_dir() {
  if ! env_bool "$REMOTE_CLEAN_TARGET"; then
    log "跳过远端 Rust target 清理 (REMOTE_CLEAN_TARGET=$REMOTE_CLEAN_TARGET)"
    return 0
  fi

  local target_base
  target_base="${TARGET_DIR##*/}"
  case "$TARGET_DIR" in
    ""|"/"|"$REMOTE_APP_ROOT"|"$REMOTE_DEPLOY_ROOT")
      warn "跳过远端 Rust target 清理：目标目录不安全 ($TARGET_DIR)"
      return 0
      ;;
    "$REMOTE_APP_ROOT"/*)
      ;;
    *)
      warn "跳过远端 Rust target 清理：$TARGET_DIR 不在 $REMOTE_APP_ROOT 下"
      return 0
      ;;
  esac

  case "$target_base" in
    target|target-*|target_shared|target-shared)
      ;;
    *)
      warn "跳过远端 Rust target 清理：目录名不像 target ($TARGET_DIR)"
      return 0
      ;;
  esac

  if [[ -e "$TARGET_DIR" ]]; then
    log "清理远端 Rust target: $TARGET_DIR"
    sudo_run rm -rf "$TARGET_DIR"
    log "已清理 Rust 编译缓存；本次 cargo build 会全量编译，停在最后几个 crate 几分钟通常正常"
  else
    log "远端 Rust target 不存在，跳过清理: $TARGET_DIR"
  fi
}

install_frontend_deps() {
  local dir="$1"
  local label="$2"
  if [[ ! -f "$dir/package.json" ]]; then
    return 0
  fi
  log "安装/刷新 ${label} 前端依赖"
  if [[ "$REMOTE_NPM_INSTALL_MODE" == "ci" && -f "$dir/package-lock.json" ]]; then
    if ! npm --prefix "$dir" ci --include=dev; then
      warn "${label} npm ci 失败，回退到 npm install 以刷新不匹配的 lockfile"
      npm --prefix "$dir" install --include=dev
    fi
  else
    npm --prefix "$dir" install --include=dev
  fi
  if [[ -d "$dir/node_modules" ]] && id -u chatos >/dev/null 2>&1; then
    sudo_run chown -R chatos:chatos "$dir/node_modules"
    sudo_run chmod -R u+rwX,go+rX "$dir/node_modules"
    sudo_run rm -rf "$dir/node_modules/.vite"
  fi
}

prepare_aux_service_env() {
  load_chatos_env

  export CHATOS_SHARED_ENV_FILE="${CHATOS_SHARED_ENV_FILE:-/etc/chatos/chatos-backend.env}"
  export CHATOS_SKIP_SERVICE_LOCAL_ENV="${CHATOS_SKIP_SERVICE_LOCAL_ENV:-1}"
  export MAIN_BACKEND_PORT="${EFFECTIVE_BACKEND_PORT}"
  export BACKEND_PORT="${EFFECTIVE_BACKEND_PORT}"
  export START_DEV_MONGO="${START_DEV_MONGO:-auto}"

  export USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}"
  export USER_SERVICE_FRONTEND_PORT="${USER_SERVICE_FRONTEND_PORT:-39191}"
  export USER_SERVICE_FRONTEND_BASE_PATH="${USER_SERVICE_FRONTEND_BASE_PATH:-/user-service/}"
  export USER_SERVICE_FRONTEND_API_BASE_URL="${USER_SERVICE_FRONTEND_API_BASE_URL:-/user-service}"
  export CHATOS_USER_SERVICE_BASE_URL="${CHATOS_USER_SERVICE_BASE_URL:-http://127.0.0.1:${USER_SERVICE_PORT}}"
  export USER_SERVICE_BASE_URL="${USER_SERVICE_BASE_URL:-$CHATOS_USER_SERVICE_BASE_URL}"
  export USER_SERVICE_DATABASE_URL="${USER_SERVICE_DATABASE_URL:-$(mongo_url_for user_service)}"
  export USER_SERVICE_API_PROXY_TARGET="${USER_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${USER_SERVICE_PORT}}"

  export MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}"
  export MEMORY_ENGINE_FRONTEND_PORT="${MEMORY_ENGINE_FRONTEND_PORT:-4178}"
  export MEMORY_ENGINE_FRONTEND_BASE_PATH="${MEMORY_ENGINE_FRONTEND_BASE_PATH:-/memory-engine/}"
  export MEMORY_ENGINE_BASE_URL="${MEMORY_ENGINE_BASE_URL:-http://127.0.0.1:${MEMORY_ENGINE_PORT}/api/memory-engine/v1}"
  export MEMORY_ENGINE_MONGODB_DATABASE="${MEMORY_ENGINE_MONGODB_DATABASE:-memory_engine}"
  export MEMORY_ENGINE_MONGODB_URI="${MEMORY_ENGINE_MONGODB_URI:-$(mongo_admin_uri)}"
  if [[ -n "${MEMORY_ENGINE_OPERATOR_TOKEN:-}" ]]; then
    export TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN="$MEMORY_ENGINE_OPERATOR_TOKEN"
  elif [[ -n "${TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN:-}" ]]; then
    export MEMORY_ENGINE_OPERATOR_TOKEN="$TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN"
  else
    export MEMORY_ENGINE_OPERATOR_TOKEN="chatos-memory-engine-prod-operator-token"
    export TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN="$MEMORY_ENGINE_OPERATOR_TOKEN"
  fi
  export MEMORY_ENGINE_API_PROXY_TARGET="${MEMORY_ENGINE_API_PROXY_TARGET:-http://127.0.0.1:${MEMORY_ENGINE_PORT}}"
  export VITE_MEMORY_ENGINE_API_BASE="${VITE_MEMORY_ENGINE_API_BASE:-/memory-engine/api/memory-engine/v1}"
  export VITE_USER_SERVICE_API_BASE="${VITE_USER_SERVICE_API_BASE:-/user-service}"

  export PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}"
  export PROJECT_SERVICE_FRONTEND_PORT="${PROJECT_SERVICE_FRONTEND_PORT:-39211}"
  export PROJECT_SERVICE_FRONTEND_BASE_PATH="${PROJECT_SERVICE_FRONTEND_BASE_PATH:-/project-management/}"
  export PROJECT_SERVICE_BASE_URL="${PROJECT_SERVICE_BASE_URL:-http://127.0.0.1:${PROJECT_SERVICE_PORT}}"
  export CHATOS_PROJECT_SERVICE_BASE_URL="${CHATOS_PROJECT_SERVICE_BASE_URL:-$PROJECT_SERVICE_BASE_URL}"
  export PROJECT_SERVICE_DATABASE_URL="${PROJECT_SERVICE_DATABASE_URL:-$(mongo_url_for project_management_service)}"
  export PROJECT_SERVICE_API_PROXY_TARGET="${PROJECT_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${PROJECT_SERVICE_PORT}}"
  export PROJECT_SERVICE_VITE_API_BASE_URL="${PROJECT_SERVICE_VITE_API_BASE_URL:-/project-management}"
  export PROJECT_SERVICE_CARGO_TARGET_DIR="${PROJECT_SERVICE_CARGO_TARGET_DIR:-$REMOTE_APP_ROOT/project_management_service/target}"
  export PROJECT_SERVICE_SYNC_SECRET="${PROJECT_SERVICE_SYNC_SECRET:-${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-change_me_project_sync_secret}}"
  export CHATOS_PROJECT_SERVICE_SYNC_SECRET="${CHATOS_PROJECT_SERVICE_SYNC_SECRET:-$PROJECT_SERVICE_SYNC_SECRET}"

  export TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}"
  export TASK_RUNNER_PORT="${TASK_RUNNER_PORT:-$TASK_RUNNER_BACKEND_PORT}"
  export TASK_RUNNER_FRONTEND_PORT="${TASK_RUNNER_FRONTEND_PORT:-39091}"
  export TASK_RUNNER_FRONTEND_BASE_PATH="${TASK_RUNNER_FRONTEND_BASE_PATH:-/task-runner/}"
  export TASK_RUNNER_BASE_URL="${TASK_RUNNER_BASE_URL:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
  export CHATOS_TASK_RUNNER_BASE_URL="${CHATOS_TASK_RUNNER_BASE_URL:-$TASK_RUNNER_BASE_URL}"
  export TASK_RUNNER_DATABASE_URL="${TASK_RUNNER_DATABASE_URL:-$(mongo_url_for task_runner_service)}"
  export TASK_RUNNER_WORKSPACE_DIR="${TASK_RUNNER_WORKSPACE_DIR:-$EFFECTIVE_CHATOS_WORKSPACE_DIR}"
  export TASK_RUNNER_CARGO_TARGET_DIR="${TASK_RUNNER_CARGO_TARGET_DIR:-$REMOTE_APP_ROOT/task_runner_service/target}"
  export TASK_RUNNER_API_PROXY_TARGET="${TASK_RUNNER_API_PROXY_TARGET:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT}}"
  export TASK_RUNNER_VITE_API_BASE_URL="${TASK_RUNNER_VITE_API_BASE_URL:-/task-runner}"
  export TASK_RUNNER_CHATOS_CALLBACK_URL="${TASK_RUNNER_CHATOS_CALLBACK_URL:-http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/api/agent/chat/task-runner/callback}"
  export TASK_RUNNER_CHATOS_CALLBACK_SECRET="${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-change_me_chatos_task_runner_secret}}"
  export CHATOS_TASK_RUNNER_CALLBACK_SECRET="${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-$TASK_RUNNER_CHATOS_CALLBACK_SECRET}"

  export SANDBOX_MANAGER_PORT="${SANDBOX_MANAGER_PORT:-8095}"
  export SANDBOX_MANAGER_FRONTEND_PORT="${SANDBOX_MANAGER_FRONTEND_PORT:-8096}"
  export SANDBOX_MANAGER_FRONTEND_BASE_PATH="${SANDBOX_MANAGER_FRONTEND_BASE_PATH:-/sandbox-manager/}"
  export SANDBOX_MANAGER_FRONTEND_API_BASE_URL="${SANDBOX_MANAGER_FRONTEND_API_BASE_URL:-/sandbox-manager}"
  export SANDBOX_MANAGER_CARGO_TARGET_DIR="${SANDBOX_MANAGER_CARGO_TARGET_DIR:-$REMOTE_APP_ROOT/sandbox_manager_service/target}"

  export OFFICIAL_WEBSITE_MODE="${OFFICIAL_WEBSITE_MODE:-prod}"
  export OFFICIAL_WEBSITE_PORT="${OFFICIAL_WEBSITE_PORT:-39250}"
  export OFFICIAL_WEBSITE_FRONTEND_PORT="${OFFICIAL_WEBSITE_FRONTEND_PORT:-39251}"

  export DB_HUB_BACKEND_PORT="${DB_HUB_BACKEND_PORT:-8099}"
  export DB_HUB_FRONTEND_PORT="${DB_HUB_FRONTEND_PORT:-5174}"
}

remote_ensure_env_line() {
  local key="$1"
  local value="$2"
  local file="$3"
  sudo_run mkdir -p "$(dirname "$file")"
  sudo_run touch "$file"
  sudo_run chmod 600 "$file"
  if sudo_run grep -qE "^${key}=" "$file"; then
    sudo_run sed -i "s|^${key}=.*|${key}=${value}|" "$file"
  else
    printf '%s=%s\n' "$key" "$value" | sudo_run tee -a "$file" >/dev/null
  fi
}

sync_aux_systemd_env() {
  if [[ ! -d /etc/systemd/system ]]; then
    return 0
  fi
  local memory_env="/etc/chatos/memory-engine.env"
  local user_env="/etc/chatos/user-service.env"
  local task_env="/etc/chatos/task-runner-service.env"
  local workspace_dir="${TASK_RUNNER_WORKSPACE_DIR:-${CHATOS_WORKSPACE_DIR:-$EFFECTIVE_CHATOS_WORKSPACE_DIR}}"

  if [[ -f "$memory_env" ]]; then
    remote_ensure_env_line MEMORY_ENGINE_OPERATOR_TOKEN "$MEMORY_ENGINE_OPERATOR_TOKEN" "$memory_env"
    remote_ensure_env_line MEMORY_ENGINE_USER_SERVICE_BASE_URL "$CHATOS_USER_SERVICE_BASE_URL" "$memory_env"
  fi
  if [[ -f "$user_env" ]]; then
    remote_ensure_env_line MEMORY_ENGINE_OPERATOR_TOKEN "$MEMORY_ENGINE_OPERATOR_TOKEN" "$user_env"
    remote_ensure_env_line MEMORY_ENGINE_BASE_URL "$MEMORY_ENGINE_BASE_URL" "$user_env"
    remote_ensure_env_line TASK_RUNNER_BASE_URL "$TASK_RUNNER_BASE_URL" "$user_env"
    remote_ensure_env_line CHATOS_TASK_RUNNER_BASE_URL "$CHATOS_TASK_RUNNER_BASE_URL" "$user_env"
    remote_ensure_env_line TASK_RUNNER_CHATOS_CALLBACK_SECRET "$TASK_RUNNER_CHATOS_CALLBACK_SECRET" "$user_env"
    remote_ensure_env_line CHATOS_TASK_RUNNER_CALLBACK_SECRET "$CHATOS_TASK_RUNNER_CALLBACK_SECRET" "$user_env"
  fi
  if [[ -f "$task_env" ]]; then
    remote_ensure_env_line TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN "$MEMORY_ENGINE_OPERATOR_TOKEN" "$task_env"
    remote_ensure_env_line MEMORY_ENGINE_OPERATOR_TOKEN "$MEMORY_ENGINE_OPERATOR_TOKEN" "$task_env"
    remote_ensure_env_line TASK_RUNNER_MEMORY_ENGINE_BASE_URL "$MEMORY_ENGINE_BASE_URL" "$task_env"
    remote_ensure_env_line MEMORY_ENGINE_BASE_URL "$MEMORY_ENGINE_BASE_URL" "$task_env"
    remote_ensure_env_line TASK_RUNNER_WORKSPACE_DIR "$workspace_dir" "$task_env"
    remote_ensure_env_line TASK_RUNNER_USER_SERVICE_BASE_URL "$CHATOS_USER_SERVICE_BASE_URL" "$task_env"
    remote_ensure_env_line CHATOS_USER_SERVICE_BASE_URL "$CHATOS_USER_SERVICE_BASE_URL" "$task_env"
    remote_ensure_env_line TASK_RUNNER_PROJECT_SERVICE_BASE_URL "$PROJECT_SERVICE_BASE_URL" "$task_env"
    remote_ensure_env_line PROJECT_SERVICE_BASE_URL "$PROJECT_SERVICE_BASE_URL" "$task_env"
    remote_ensure_env_line CHATOS_PROCESS_ISOLATION_ENABLED "$REMOTE_ENABLE_PROCESS_ISOLATION" "$task_env"
    remote_ensure_env_line CHATOS_PROCESS_ISOLATION_FS_ENABLED "$REMOTE_PROCESS_ISOLATION_FS_ENABLED" "$task_env"
    remote_ensure_env_line CHATOS_PROCESS_ISOLATION_FS_ROOT "$REMOTE_PROCESS_ISOLATION_FS_ROOT" "$task_env"
    remote_ensure_env_line CHATOS_PROCESS_ISOLATION_FS_MOUNT_PROC "$REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC" "$task_env"
  fi

  if systemctl list-unit-files task-runner-service-backend.service >/dev/null 2>&1; then
    sudo_run mkdir -p /etc/systemd/system/task-runner-service-backend.service.d
    sudo_run tee /etc/systemd/system/task-runner-service-backend.service.d/process-isolation.conf >/dev/null <<'EOF_CAPS'
[Service]
User=chatos
Group=chatos
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_CHOWN CAP_FOWNER CAP_SYS_ADMIN CAP_DAC_READ_SEARCH
AmbientCapabilities=CAP_SETUID CAP_SETGID CAP_CHOWN CAP_FOWNER CAP_SYS_ADMIN CAP_DAC_READ_SEARCH
NoNewPrivileges=false
EOF_CAPS
    sudo_run systemctl daemon-reload
  fi
}

apply_nginx_config_only() {
  prepare_aux_service_env
  local template="$REMOTE_APP_ROOT/deploy/linux/nginx/chatos.conf.tpl"
  local nginx_site="/etc/nginx/sites-available/chatos.conf"
  local nginx_link="/etc/nginx/sites-enabled/chatos.conf"
  local frontend_dir="$REMOTE_DEPLOY_ROOT/frontend"

  if [[ ! -f "$template" ]]; then
    echo "[ERROR] nginx template missing: $template"
    return 1
  fi

  log "apply nginx config only"
  sudo_run mkdir -p /etc/nginx/sites-available /etc/nginx/sites-enabled
  sed \
    -e "s|__SERVER_NAME__|$REMOTE_SERVER_NAME|g" \
    -e "s|__BACKEND_PORT__|$EFFECTIVE_BACKEND_PORT|g" \
    -e "s|__USER_SERVICE_PORT__|${USER_SERVICE_PORT:-39190}|g" \
    -e "s|__USER_SERVICE_FRONTEND_PORT__|${USER_SERVICE_FRONTEND_PORT:-39191}|g" \
    -e "s|__MEMORY_ENGINE_PORT__|${MEMORY_ENGINE_PORT:-7081}|g" \
    -e "s|__MEMORY_ENGINE_FRONTEND_PORT__|${MEMORY_ENGINE_FRONTEND_PORT:-4178}|g" \
    -e "s|__PROJECT_SERVICE_PORT__|${PROJECT_SERVICE_PORT:-39210}|g" \
    -e "s|__PROJECT_SERVICE_FRONTEND_PORT__|${PROJECT_SERVICE_FRONTEND_PORT:-39211}|g" \
    -e "s|__TASK_RUNNER_PORT__|${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}|g" \
    -e "s|__TASK_RUNNER_FRONTEND_PORT__|${TASK_RUNNER_FRONTEND_PORT:-39091}|g" \
    -e "s|__SANDBOX_MANAGER_PORT__|${SANDBOX_MANAGER_PORT:-8095}|g" \
    -e "s|__SANDBOX_MANAGER_FRONTEND_PORT__|${SANDBOX_MANAGER_FRONTEND_PORT:-8096}|g" \
    -e "s|__FRONTEND_ROOT__|$frontend_dir|g" \
    "$template" | sudo_run tee "$nginx_site" >/dev/null

  local site_link
  for site_link in \
    /etc/nginx/sites-enabled/default \
    /etc/nginx/sites-enabled/memory-engine.conf \
    /etc/nginx/sites-enabled/project-management-service.conf \
    /etc/nginx/sites-enabled/task-runner-service.conf \
    /etc/nginx/sites-enabled/user-service.conf; do
    if [[ -e "$site_link" && "$site_link" != "$nginx_link" ]]; then
      sudo_run rm -f "$site_link"
    fi
  done

  sudo_run ln -sfn "$nginx_site" "$nginx_link"
  sudo_run nginx -t
  sudo_run systemctl enable --now nginx
  if systemctl is-active --quiet nginx; then
    sudo_run systemctl reload nginx
  else
    sudo_run systemctl restart nginx
  fi
}

systemd_unit_exists() {
  local unit="$1"
  systemctl list-unit-files "$unit" --no-legend --no-pager 2>/dev/null | awk '{print $1}' | grep -qx "$unit"
}

systemd_unit_exec_path() {
  local unit="$1"
  systemctl show -p ExecStart --value "$unit" 2>/dev/null \
    | sed -n 's|.*path=\([^ ;]*\).*|\1|p' \
    | head -n 1
}

install_aux_binary_for_unit() {
  local unit="$1"
  local src="$2"
  local dest
  dest="$(systemd_unit_exec_path "$unit")"
  if [[ -z "$dest" || "$dest" != /* ]]; then
    warn "无法解析 $unit 的 ExecStart，跳过二进制安装"
    return 0
  fi
  if [[ ! -f "$src" ]]; then
    echo "[ERROR] 附属服务二进制不存在: $src"
    return 1
  fi
  log "安装 $unit 二进制: $dest"
  sudo_run install -m 0755 "$src" "$dest"
  if id -u chatos >/dev/null 2>&1; then
    sudo_run chown chatos:chatos "$dest"
  fi
}

restart_aux_services_via_systemd_if_present() {
  local units=()
  local workspace_packages=()

  if service_selected memory-engine && systemd_unit_exists memory-engine-backend.service; then
    log "构建 memory_engine release"
    cargo build --release --manifest-path "$REMOTE_APP_ROOT/memory_engine/backend/Cargo.toml" \
      --target-dir "$REMOTE_APP_ROOT/memory_engine/backend/target"
    install_aux_binary_for_unit \
      memory-engine-backend.service \
      "$REMOTE_APP_ROOT/memory_engine/backend/target/release/memory_engine"
    units+=(memory-engine-backend.service)
  fi

  if service_selected user-service && systemd_unit_exists user-service-backend.service; then
    log "构建 user_service release"
    cargo build --release --manifest-path "$REMOTE_APP_ROOT/user_service/backend/Cargo.toml" \
      --target-dir "$REMOTE_APP_ROOT/user_service/backend/target"
    install_aux_binary_for_unit \
      user-service-backend.service \
      "$REMOTE_APP_ROOT/user_service/backend/target/release/user_service_backend"
    units+=(user-service-backend.service)
  fi

  if service_selected project-management && systemd_unit_exists project-management-service-backend.service; then
    workspace_packages+=(project_management_service_backend)
    units+=(project-management-service-backend.service)
  fi
  if service_selected sandbox-manager && systemd_unit_exists sandbox-manager-service-backend.service; then
    workspace_packages+=(sandbox_manager_service_backend)
    units+=(sandbox-manager-service-backend.service)
  fi
  if service_selected task-runner && systemd_unit_exists task-runner-service-backend.service; then
    workspace_packages+=(task_runner_service_backend)
    units+=(task-runner-service-backend.service)
  fi

  if (( ${#workspace_packages[@]} > 0 )); then
    log "构建附属 workspace release: ${workspace_packages[*]}"
    for package in "${workspace_packages[@]}"; do
      cargo build --release --manifest-path "$REMOTE_APP_ROOT/Cargo.toml" \
        --target-dir "$TARGET_DIR" \
        -p "$package"
    done
  fi

  if service_selected project-management && systemd_unit_exists project-management-service-backend.service; then
    install_aux_binary_for_unit \
      project-management-service-backend.service \
      "$TARGET_DIR/release/project_management_service_backend"
  fi
  if service_selected sandbox-manager && systemd_unit_exists sandbox-manager-service-backend.service; then
    install_aux_binary_for_unit \
      sandbox-manager-service-backend.service \
      "$TARGET_DIR/release/sandbox_manager_service_backend"
  fi
  if service_selected task-runner && systemd_unit_exists task-runner-service-backend.service; then
    install_aux_binary_for_unit \
      task-runner-service-backend.service \
      "$TARGET_DIR/release/task_runner_service_backend"
  fi

  if (( ${#units[@]} == 0 )); then
    return 1
  fi

  log "通过 systemd 重启附属服务，跳过 dev cargo/Vite 后端启动: ${units[*]}"
  sudo_run systemctl daemon-reload
  sudo_run systemctl restart "${units[@]}"
  return 0
}

restart_sandbox_manager_frontend_dev_if_present() {
  local frontend_dir="$REMOTE_APP_ROOT/sandbox_manager_service/frontend"
  local log_dir="$REMOTE_APP_ROOT/logs"
  local frontend_port="${SANDBOX_MANAGER_FRONTEND_PORT:-8096}"
  local backend_port="${SANDBOX_MANAGER_PORT:-8095}"
  local base_path="${SANDBOX_MANAGER_FRONTEND_BASE_PATH:-/sandbox-manager/}"
  local api_base="${SANDBOX_MANAGER_FRONTEND_API_BASE_URL:-/sandbox-manager}"

  if [[ ! -d "$frontend_dir" ]]; then
    return 0
  fi

  log "重启 sandbox_manager_service 前端 dev server (:${frontend_port})"
  sudo_run mkdir -p "$log_dir"
  if id -u chatos >/dev/null 2>&1; then
    sudo_run chown -R chatos:chatos "$log_dir"
  fi

  if command -v tmux >/dev/null 2>&1 && tmux has-session -t chatos_sandbox_manager_frontend 2>/dev/null; then
    tmux kill-session -t chatos_sandbox_manager_frontend 2>/dev/null || true
  fi

  local pids=""
  if command -v lsof >/dev/null 2>&1; then
    pids="$(lsof -tiTCP:"$frontend_port" -sTCP:LISTEN 2>/dev/null || true)"
  elif command -v fuser >/dev/null 2>&1; then
    pids="$(fuser "${frontend_port}/tcp" 2>/dev/null || true)"
  else
    pids="$(ss -ltnp 2>/dev/null | awk -v port=":${frontend_port}" '$0 ~ port {print $NF}' | sed -n 's/.*pid=\([0-9]*\).*/\1/p' | sort -u)"
  fi
  if [[ -n "$pids" ]]; then
    sudo_run kill $pids 2>/dev/null || true
    sleep 1
    sudo_run kill -9 $pids 2>/dev/null || true
  fi

  local start_cmd
  printf -v start_cmd \
    "cd %q && SANDBOX_MANAGER_FRONTEND_PORT=%q SANDBOX_MANAGER_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_API_BASE_URL=%q nohup npm run dev -- --host 0.0.0.0 --port %q >%q 2>&1 < /dev/null &" \
    "$frontend_dir" \
    "$frontend_port" \
    "http://127.0.0.1:${backend_port}" \
    "$base_path" \
    "$api_base" \
    "$frontend_port" \
    "$log_dir/sandbox_manager_frontend.log"
  if id -u chatos >/dev/null 2>&1; then
    sudo_run runuser -u chatos -- bash -lc "$start_cmd"
  else
    bash -lc "$start_cmd"
  fi
}

restart_frontend_dev_server_if_present() {
  local label="$1"
  local frontend_dir="$2"
  local frontend_port="$3"
  local tmux_session="$4"
  local log_file="$5"
  local env_prefix="$6"
  local extra_args="${7:-}"

  if [[ ! -d "$frontend_dir" ]]; then
    return 0
  fi

  local log_dir
  log_dir="$(dirname "$log_file")"
  log "restart ${label} frontend dev server (:${frontend_port})"
  sudo_run mkdir -p "$log_dir"
  if id -u chatos >/dev/null 2>&1; then
    sudo_run chown -R chatos:chatos "$log_dir"
  fi

  if command -v tmux >/dev/null 2>&1 && tmux has-session -t "$tmux_session" 2>/dev/null; then
    tmux kill-session -t "$tmux_session" 2>/dev/null || true
  fi

  local pids=""
  if command -v lsof >/dev/null 2>&1; then
    pids="$(lsof -tiTCP:"$frontend_port" -sTCP:LISTEN 2>/dev/null || true)"
  elif command -v fuser >/dev/null 2>&1; then
    pids="$(fuser "${frontend_port}/tcp" 2>/dev/null || true)"
  else
    pids="$(ss -ltnp 2>/dev/null | awk -v port=":${frontend_port}" '$0 ~ port {print $NF}' | sed -n 's/.*pid=\([0-9]*\).*/\1/p' | sort -u)"
  fi
  if [[ -n "$pids" ]]; then
    sudo_run kill $pids 2>/dev/null || true
    sleep 1
    sudo_run kill -9 $pids 2>/dev/null || true
  fi

  local start_cmd
  printf -v start_cmd \
    "cd %q && %s nohup npm run dev -- --host 0.0.0.0 --port %q %s >%q 2>&1 < /dev/null &" \
    "$frontend_dir" \
    "$env_prefix" \
    "$frontend_port" \
    "$extra_args" \
    "$log_file"
  if id -u chatos >/dev/null 2>&1; then
    sudo_run runuser -u chatos -- bash -lc "$start_cmd"
  else
    bash -lc "$start_cmd"
  fi
}

restart_aux_frontend_dev_servers_if_present() {
  local log_dir="$REMOTE_APP_ROOT/logs"
  local env_prefix

  printf -v env_prefix \
    "USER_SERVICE_FRONTEND_PORT=%q USER_SERVICE_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_API_BASE_URL=%q" \
    "${USER_SERVICE_FRONTEND_PORT:-39191}" \
    "${USER_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${USER_SERVICE_PORT:-39190}}" \
    "${USER_SERVICE_FRONTEND_BASE_PATH:-/user-service/}" \
    "${USER_SERVICE_FRONTEND_API_BASE_URL:-/user-service}"
  if service_selected user-service; then
    restart_frontend_dev_server_if_present \
      "user_service" \
      "$REMOTE_APP_ROOT/user_service/frontend" \
      "${USER_SERVICE_FRONTEND_PORT:-39191}" \
      "chatos_user_service_frontend" \
      "$log_dir/user_service_frontend.log" \
      "$env_prefix"
  fi

  printf -v env_prefix \
    "MEMORY_ENGINE_FRONTEND_PORT=%q MEMORY_ENGINE_API_PROXY_TARGET=%q USER_SERVICE_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_MEMORY_ENGINE_API_BASE=%q VITE_MEMORY_ENGINE_PORT=%q VITE_MEMORY_ENGINE_OPERATOR_TOKEN=%q VITE_USER_SERVICE_API_BASE=%q" \
    "${MEMORY_ENGINE_FRONTEND_PORT:-4178}" \
    "${MEMORY_ENGINE_API_PROXY_TARGET:-http://127.0.0.1:${MEMORY_ENGINE_PORT:-7081}}" \
    "${USER_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${USER_SERVICE_PORT:-39190}}" \
    "${MEMORY_ENGINE_FRONTEND_BASE_PATH:-/memory-engine/}" \
    "${VITE_MEMORY_ENGINE_API_BASE:-/memory-engine/api/memory-engine/v1}" \
    "${MEMORY_ENGINE_PORT:-7081}" \
    "${MEMORY_ENGINE_OPERATOR_TOKEN:-}" \
    "${VITE_USER_SERVICE_API_BASE:-/user-service}"
  if service_selected memory-engine; then
    restart_frontend_dev_server_if_present \
      "memory_engine" \
      "$REMOTE_APP_ROOT/memory_engine/frontend" \
      "${MEMORY_ENGINE_FRONTEND_PORT:-4178}" \
      "chatos_memory_engine_frontend" \
      "$log_dir/memory_engine_frontend.log" \
      "$env_prefix" \
      "--strictPort"
  fi

  printf -v env_prefix \
    "PROJECT_SERVICE_FRONTEND_PORT=%q PROJECT_SERVICE_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_API_BASE_URL=%q" \
    "${PROJECT_SERVICE_FRONTEND_PORT:-39211}" \
    "${PROJECT_SERVICE_API_PROXY_TARGET:-http://127.0.0.1:${PROJECT_SERVICE_PORT:-39210}}" \
    "${PROJECT_SERVICE_FRONTEND_BASE_PATH:-/project-management/}" \
    "${PROJECT_SERVICE_VITE_API_BASE_URL:-/project-management}"
  if service_selected project-management; then
    restart_frontend_dev_server_if_present \
      "project_management_service" \
      "$REMOTE_APP_ROOT/project_management_service/frontend" \
      "${PROJECT_SERVICE_FRONTEND_PORT:-39211}" \
      "chatos_project_management_frontend" \
      "$log_dir/project_management_frontend.log" \
      "$env_prefix"
  fi

  printf -v env_prefix \
    "TASK_RUNNER_FRONTEND_PORT=%q TASK_RUNNER_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_API_BASE_URL=%q" \
    "${TASK_RUNNER_FRONTEND_PORT:-39091}" \
    "${TASK_RUNNER_API_PROXY_TARGET:-http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT:-39090}}" \
    "${TASK_RUNNER_FRONTEND_BASE_PATH:-/task-runner/}" \
    "${TASK_RUNNER_VITE_API_BASE_URL:-/task-runner}"
  if service_selected task-runner; then
    restart_frontend_dev_server_if_present \
      "task_runner_service" \
      "$REMOTE_APP_ROOT/task_runner_service/frontend" \
      "${TASK_RUNNER_FRONTEND_PORT:-39091}" \
      "chatos_task_runner_frontend" \
      "$log_dir/task_runner_frontend.log" \
      "$env_prefix"
  fi

  printf -v env_prefix \
    "SANDBOX_MANAGER_FRONTEND_PORT=%q SANDBOX_MANAGER_API_PROXY_TARGET=%q VITE_BASE_PATH=%q VITE_API_BASE_URL=%q" \
    "${SANDBOX_MANAGER_FRONTEND_PORT:-8096}" \
    "${SANDBOX_MANAGER_API_PROXY_TARGET:-http://127.0.0.1:${SANDBOX_MANAGER_PORT:-8095}}" \
    "${SANDBOX_MANAGER_FRONTEND_BASE_PATH:-/sandbox-manager/}" \
    "${SANDBOX_MANAGER_FRONTEND_API_BASE_URL:-/sandbox-manager}"
  if service_selected sandbox-manager; then
    restart_frontend_dev_server_if_present \
      "sandbox_manager_service" \
      "$REMOTE_APP_ROOT/sandbox_manager_service/frontend" \
      "${SANDBOX_MANAGER_FRONTEND_PORT:-8096}" \
      "chatos_sandbox_manager_frontend" \
      "$log_dir/sandbox_manager_frontend.log" \
      "$env_prefix"
  fi
}

rebuild_aux_services() {
  if ! env_bool "$REMOTE_REBUILD_AUX_SERVICES" && deployment_is_full; then
    log "跳过附属服务重建 (REMOTE_REBUILD_AUX_SERVICES=$REMOTE_REBUILD_AUX_SERVICES)"
    return 0
  fi

  if ! any_service_selected user-service memory-engine project-management task-runner sandbox-manager official-website db-hub; then
    log "skip auxiliary services: no auxiliary service selected"
    return 0
  fi

  prepare_aux_service_env
  sync_aux_systemd_env

  if service_selected memory-engine; then
    install_frontend_deps "$REMOTE_APP_ROOT/memory_engine/frontend" "memory_engine"
  fi
  if service_selected user-service; then
    install_frontend_deps "$REMOTE_APP_ROOT/user_service/frontend" "user_service"
  fi
  if service_selected project-management; then
    install_frontend_deps "$REMOTE_APP_ROOT/project_management_service/frontend" "project_management_service"
  fi
  if service_selected task-runner; then
    install_frontend_deps "$REMOTE_APP_ROOT/task_runner_service/frontend" "task_runner_service"
  fi
  if service_selected sandbox-manager; then
    install_frontend_deps "$REMOTE_APP_ROOT/sandbox_manager_service/frontend" "sandbox_manager_service"
  fi

  if service_selected official-website && { ! deployment_is_full || env_bool "$REMOTE_REBUILD_OFFICIAL_WEBSITE"; }; then
    install_frontend_deps "$REMOTE_APP_ROOT/official_website_service/frontend" "official_website_service"
    log "构建 official_website_service 前端静态文件"
    npm --prefix "$REMOTE_APP_ROOT/official_website_service/frontend" run build
  fi

  if restart_aux_services_via_systemd_if_present; then
    restart_aux_frontend_dev_servers_if_present
    return 0
  fi

  warn "远端未检测到附属 systemd unit，退回本地开发脚本启动附属服务"
  log "重建/重启附属服务（memory/user/project/sandbox/task/official）"
  START_CHATOS=0 \
    START_MEMORY_ENGINE="$(service_selected memory-engine && printf 1 || printf 0)" \
    START_USER_SERVICE="$(service_selected user-service && printf 1 || printf 0)" \
    START_PROJECT_MANAGEMENT="$(service_selected project-management && printf 1 || printf 0)" \
    START_SANDBOX_MANAGER="$(service_selected sandbox-manager && printf 1 || printf 0)" \
    START_TASK_RUNNER="$(service_selected task-runner && printf 1 || printf 0)" \
    START_OFFICIAL_WEBSITE="$(service_selected official-website && { ! deployment_is_full || env_bool "$REMOTE_REBUILD_OFFICIAL_WEBSITE"; } && printf 1 || printf 0)" \
    "$REMOTE_APP_ROOT/restart_all_services.sh" restart

  if service_selected db-hub && { ! deployment_is_full || env_bool "$REMOTE_REBUILD_DB_CONNECTION_HUB"; } && [[ -x "$REMOTE_APP_ROOT/db_connection_hub/restart_services.sh" ]]; then
    install_frontend_deps "$REMOTE_APP_ROOT/db_connection_hub/frontend" "db_connection_hub"
    log "重建/重启 db_connection_hub"
    STARTUP_HEALTHCHECK_TIMEOUT_SEC="$REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC" \
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
elif service_selected main; then
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
if any_service_selected main project-management task-runner sandbox-manager; then
  clean_remote_target_dir
else
  log "skip Rust target cleanup: selected services do not use target-shared"
fi
sudo_run chown -R "$(id -un)":"$(id -gn)" "$REMOTE_APP_ROOT"

log "从远端暂存目录同步到正式代码目录"
rsync -a --delete \
  --exclude '.env' \
  --exclude '*.env' \
  --exclude 'target/' \
  --exclude 'target-*/' \
  --exclude 'target-shared/' \
  --exclude 'node_modules/' \
  --exclude 'logs/' \
  --exclude '.local/' \
  --exclude '.vite/' \
  "$REMOTE_STAGE_DIR/" "$REMOTE_APP_ROOT/"

cd "$REMOTE_APP_ROOT"
log "归一化远端 shell 脚本行尾"
find "$REMOTE_APP_ROOT" -type f -name '*.sh' -exec sed -i 's/\r$//' {} +

if command -v rustup >/dev/null 2>&1; then
  ACTIVE_TOOLCHAIN="$(cd "$REMOTE_APP_ROOT" && rustup show active-toolchain 2>/dev/null | awk 'NR==1 {print $1}')"
  if [[ -n "$ACTIVE_TOOLCHAIN" ]]; then
    export RUSTUP_TOOLCHAIN="$ACTIVE_TOOLCHAIN"
  fi
fi

# 生产前端需要 dist；不走 npm run dev。
log "构建 chat_app 前端生产 dist"
if service_selected main; then
install_frontend_deps "$REMOTE_APP_ROOT/chat_app" "chat_app"
npm --prefix "$REMOTE_APP_ROOT/chat_app" run build

# 仅做最小生产构建：Rust 后端统一使用 target-shared；前端由现有部署流程按需处理。
log "构建 Rust 后端（target-shared）"
log "Rust release 编译参数: lto=$CARGO_PROFILE_RELEASE_LTO codegen-units=$CARGO_PROFILE_RELEASE_CODEGEN_UNITS jobs=${CARGO_BUILD_JOBS:-auto}"
if env_bool "$REMOTE_CLEAN_TARGET"; then
  log "提示：刚清理过 target-shared，这一步是全量 Rust 编译；chat_app_server_rs 编译/链接阶段可能较久没有新输出"
fi
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
    USER_SERVICE_PORT="${USER_SERVICE_PORT:-39190}" \
    USER_SERVICE_FRONTEND_PORT="${USER_SERVICE_FRONTEND_PORT:-39191}" \
    MEMORY_ENGINE_PORT="${MEMORY_ENGINE_PORT:-7081}" \
    MEMORY_ENGINE_FRONTEND_PORT="${MEMORY_ENGINE_FRONTEND_PORT:-4178}" \
    PROJECT_SERVICE_PORT="${PROJECT_SERVICE_PORT:-39210}" \
    PROJECT_SERVICE_FRONTEND_PORT="${PROJECT_SERVICE_FRONTEND_PORT:-39211}" \
    TASK_RUNNER_BACKEND_PORT="${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}" \
    TASK_RUNNER_FRONTEND_PORT="${TASK_RUNNER_FRONTEND_PORT:-39091}" \
    SANDBOX_MANAGER_PORT="${SANDBOX_MANAGER_PORT:-8095}" \
    SANDBOX_MANAGER_FRONTEND_PORT="${SANDBOX_MANAGER_FRONTEND_PORT:-8096}" \
    CARGO_TARGET_DIR="$TARGET_DIR" \
    CHATOS_WORKSPACE_DIR="$EFFECTIVE_CHATOS_WORKSPACE_DIR" \
    ENABLE_PROCESS_ISOLATION="$REMOTE_ENABLE_PROCESS_ISOLATION" \
    PROCESS_ISOLATION_PRIVILEGE_MODE="$REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE" \
    PROCESS_ISOLATION_FS_ENABLED="$REMOTE_PROCESS_ISOLATION_FS_ENABLED" \
    PROCESS_ISOLATION_FS_ROOT="$REMOTE_PROCESS_ISOLATION_FS_ROOT" \
    PROCESS_ISOLATION_FS_MOUNT_PROC="$REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC" \
    bash "$REMOTE_APP_ROOT/scripts/server-install-nodocker.sh"
else
  warn "缺少 scripts/server-install-nodocker.sh，跳过 systemd/nginx 自动安装"
fi
else
  log "skip main backend/frontend build: main is not selected"
fi

if [[ "$ENV_PREEXISTED" == "0" ]]; then
  warn "首次部署已生成 /etc/chatos/chatos-backend.env；如启用 user_service，请手工补齐 CHATOS_ADMIN_PASSWORD / USER_SERVICE_SUPER_ADMIN_PASSWORD 等敏感值后再重跑或重启相关服务"
fi

if service_selected nginx && ! service_selected main; then
  apply_nginx_config_only
fi

rebuild_aux_services

if env_bool "$REMOTE_REBUILD_AUX_SERVICES" && deployment_is_full; then
  log "重启主后端以重新执行依赖服务 bootstrap"
  sudo_run systemctl restart "$REMOTE_SERVICE_NAME"
fi

log "最小健康检查"
if service_selected main; then
if command -v curl >/dev/null 2>&1; then
  HEALTH_BODY=""
  HEALTH_DEADLINE=$((SECONDS + 60))
  while (( SECONDS < HEALTH_DEADLINE )); do
    if HEALTH_BODY="$(curl -fsS "http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/health" 2>/dev/null)"; then
      break
    fi
    sleep 1
  done
  if [[ -z "$HEALTH_BODY" ]]; then
    echo "[ERROR] health check timed out: http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/health"
    exit 1
  fi
  if printf '%s' "$HEALTH_BODY" | grep -q '"ready":[[:space:]]*true'; then
    log "health ready ok: http://127.0.0.1:${EFFECTIVE_BACKEND_PORT}/health"
  else
    echo "$HEALTH_BODY"
    echo "[ERROR] health returned ready=false"
    exit 1
  fi
else
  warn "curl 不存在，跳过 health 检查"
fi

log "状态输出"
fi

check_selected_http() {
  local name="$1"
  local url="$2"
  if ! command -v curl >/dev/null 2>&1; then
    warn "curl missing, skip health check: $name $url"
    return 0
  fi
  local body=""
  local deadline=$((SECONDS + 60))
  while (( SECONDS < deadline )); do
    if body="$(curl -fsS "$url" 2>/dev/null)"; then
      log "health ok: $name $url"
      return 0
    fi
    sleep 1
  done
  echo "[ERROR] health check timed out: $name $url"
  return 1
}

if service_selected user-service; then
  check_selected_http "user-service backend" "http://127.0.0.1:${USER_SERVICE_PORT:-39190}/api/health"
  check_selected_http "user-service frontend" "http://127.0.0.1:${USER_SERVICE_FRONTEND_PORT:-39191}/user-service/"
fi
if service_selected memory-engine; then
  check_selected_http "memory-engine backend" "http://127.0.0.1:${MEMORY_ENGINE_PORT:-7081}/health"
  check_selected_http "memory-engine frontend" "http://127.0.0.1:${MEMORY_ENGINE_FRONTEND_PORT:-4178}/memory-engine/"
fi
if service_selected project-management; then
  check_selected_http "project-management backend" "http://127.0.0.1:${PROJECT_SERVICE_PORT:-39210}/api/health"
  check_selected_http "project-management frontend" "http://127.0.0.1:${PROJECT_SERVICE_FRONTEND_PORT:-39211}/project-management/"
fi
if service_selected task-runner; then
  check_selected_http "task-runner backend" "http://127.0.0.1:${TASK_RUNNER_BACKEND_PORT:-${TASK_RUNNER_PORT:-39090}}/api/health"
  check_selected_http "task-runner frontend" "http://127.0.0.1:${TASK_RUNNER_FRONTEND_PORT:-39091}/task-runner/"
fi
if service_selected sandbox-manager; then
  check_selected_http "sandbox-manager backend" "http://127.0.0.1:${SANDBOX_MANAGER_PORT:-8095}/health"
  check_selected_http "sandbox-manager frontend" "http://127.0.0.1:${SANDBOX_MANAGER_FRONTEND_PORT:-8096}/sandbox-manager/"
fi
if service_selected nginx; then
  sudo_run nginx -t
fi

if service_selected main; then
  sudo_run systemctl --no-pager --full status "$REMOTE_SERVICE_NAME" | sed -n '1,12p' || true
fi
sudo_run nginx -t || true
REMOTE_EOF

REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_STAGE_DIR__/$REMOTE_STAGE_DIR}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_APP_ROOT__/$REMOTE_APP_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_DEPLOY_ROOT__/$REMOTE_DEPLOY_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVICE_NAME__/$REMOTE_SERVICE_NAME}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_BACKEND_PORT__/$REMOTE_BACKEND_PORT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_SERVER_NAME__/$REMOTE_SERVER_NAME}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CHATOS_WORKSPACE_DIR__/$REMOTE_CHATOS_WORKSPACE_DIR}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_DEPLOY_SERVICES__/$REMOTE_DEPLOY_SERVICES}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_AUX_SERVICES__/$REMOTE_REBUILD_AUX_SERVICES}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_OFFICIAL_WEBSITE__/$REMOTE_REBUILD_OFFICIAL_WEBSITE}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_REBUILD_DB_CONNECTION_HUB__/$REMOTE_REBUILD_DB_CONNECTION_HUB}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC__/$REMOTE_DB_HUB_STARTUP_HEALTHCHECK_TIMEOUT_SEC}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_NPM_INSTALL_MODE__/$REMOTE_NPM_INSTALL_MODE}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CLEAN_TARGET__/$REMOTE_CLEAN_TARGET}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CARGO_RELEASE_LTO__/$REMOTE_CARGO_RELEASE_LTO}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CARGO_RELEASE_CODEGEN_UNITS__/$REMOTE_CARGO_RELEASE_CODEGEN_UNITS}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_CARGO_BUILD_JOBS__/$REMOTE_CARGO_BUILD_JOBS}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_ENABLE_PROCESS_ISOLATION__/$REMOTE_ENABLE_PROCESS_ISOLATION}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE__/$REMOTE_PROCESS_ISOLATION_PRIVILEGE_MODE}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_PROCESS_ISOLATION_FS_ENABLED__/$REMOTE_PROCESS_ISOLATION_FS_ENABLED}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_PROCESS_ISOLATION_FS_ROOT__/$REMOTE_PROCESS_ISOLATION_FS_ROOT}"
REMOTE_SCRIPT="${REMOTE_SCRIPT/__REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC__/$REMOTE_PROCESS_ISOLATION_FS_MOUNT_PROC}"

log "执行远端更新与部署"
sshpass -p "$REMOTE_PASSWORD" ssh -T "${SSH_OPTIONS[@]}" "$REMOTE_USER@$REMOTE_HOST" \
  "REMOTE_SUDO_PASSWORD=$(printf '%q' "$REMOTE_SUDO_PASSWORD") bash -s" <<EOF
$REMOTE_SCRIPT
EOF

log "完成：可在远端查看 $REMOTE_SERVICE_NAME、nginx 和 /etc/chatos/chatos-backend.env"
log "说明：已同步并重建主服务；REMOTE_REBUILD_AUX_SERVICES=1 时也会重建 memory/user/project/sandbox/task/official/db_connection_hub。"
log "提示：若首次生成了 /etc/chatos/chatos-backend.env，请手工确认其中敏感值；如启用 sandbox_manager_service 的 Docker 降级模式，还需确认 SANDBOX_MANAGER_BACKEND=docker、SANDBOX_MANAGER_DOCKER_IMAGE、SANDBOX_MANAGER_DOCKER_NETWORK、SANDBOX_MANAGER_IMAGE_BUILD_CONTEXT、SANDBOX_MANAGER_IMAGE_DOCKERFILE 以及 docker daemon 可用"
