#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "[ERROR] 请用 root 运行（例如: sudo bash scripts/configure-linux-process-isolation.sh）"
  exit 1
fi

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "[ERROR] OS 用户级进程隔离只支持 Linux"
  exit 1
fi

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

env_bool() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

upsert_env() {
  local key="$1"
  local value="$2"
  local file="$3"
  if grep -qE "^${key}=" "$file"; then
    sed -i "s|^${key}=.*|${key}=${value}|g" "$file"
  else
    printf '%s=%s\n' "$key" "$value" >> "$file"
  fi
}

need_cmd install
need_cmd grep
need_cmd sed
need_cmd systemctl

SERVICE_NAME="${SERVICE_NAME:-chatos-backend}"
SERVICE_USER="${SERVICE_USER:-chatos}"
SERVICE_GROUP="${SERVICE_GROUP:-chatos}"
ENV_FILE="${ENV_FILE:-/etc/chatos/chatos-backend.env}"
PRIVILEGE_MODE="${PROCESS_ISOLATION_PRIVILEGE_MODE:-capabilities}"
SYSTEMD_DAEMON_RELOAD="${SYSTEMD_DAEMON_RELOAD:-1}"
RESTART_SERVICE="${RESTART_SERVICE:-1}"

PROCESS_ISOLATION_UID_BASE="${PROCESS_ISOLATION_UID_BASE:-200000}"
PROCESS_ISOLATION_UID_SPAN="${PROCESS_ISOLATION_UID_SPAN:-1000000000}"
PROCESS_ISOLATION_GID_BASE="${PROCESS_ISOLATION_GID_BASE:-200000}"
PROCESS_ISOLATION_GID_SPAN="${PROCESS_ISOLATION_GID_SPAN:-1000000000}"
PROCESS_ISOLATION_CHOWN_WORKSPACE="${PROCESS_ISOLATION_CHOWN_WORKSPACE:-true}"
PROCESS_ISOLATION_CHOWN_MAX_ENTRIES="${PROCESS_ISOLATION_CHOWN_MAX_ENTRIES:-200000}"

SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
DROPIN_DIR="/etc/systemd/system/${SERVICE_NAME}.service.d"
DROPIN_FILE="${DROPIN_DIR}/process-isolation.conf"

if [[ ! -f "$SERVICE_FILE" ]]; then
  echo "[ERROR] systemd service 不存在: $SERVICE_FILE"
  echo "        请先完成后端服务部署，或设置 SERVICE_NAME"
  exit 1
fi

install -d -m 0755 "$(dirname "$ENV_FILE")"
if [[ ! -f "$ENV_FILE" ]]; then
  install -m 0640 /dev/null "$ENV_FILE"
fi

upsert_env "CHATOS_PROCESS_ISOLATION_ENABLED" "true" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_UID_BASE" "$PROCESS_ISOLATION_UID_BASE" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_UID_SPAN" "$PROCESS_ISOLATION_UID_SPAN" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_GID_BASE" "$PROCESS_ISOLATION_GID_BASE" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_GID_SPAN" "$PROCESS_ISOLATION_GID_SPAN" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_CHOWN_WORKSPACE" "$PROCESS_ISOLATION_CHOWN_WORKSPACE" "$ENV_FILE"
upsert_env "CHATOS_PROCESS_ISOLATION_CHOWN_MAX_ENTRIES" "$PROCESS_ISOLATION_CHOWN_MAX_ENTRIES" "$ENV_FILE"

if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
  chown root:"$SERVICE_GROUP" "$ENV_FILE"
else
  chown root:root "$ENV_FILE"
fi
chmod 0640 "$ENV_FILE"

install -d -m 0755 "$DROPIN_DIR"
case "$PRIVILEGE_MODE" in
  capabilities)
    cat > "$DROPIN_FILE" <<EOF
[Service]
User=${SERVICE_USER}
Group=${SERVICE_GROUP}
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_CHOWN
AmbientCapabilities=CAP_SETUID CAP_SETGID CAP_CHOWN
NoNewPrivileges=false
EOF
    ;;
  root)
    cat > "$DROPIN_FILE" <<EOF
[Service]
User=root
Group=root
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_CHOWN
AmbientCapabilities=
NoNewPrivileges=false
EOF
    ;;
  *)
    echo "[ERROR] PROCESS_ISOLATION_PRIVILEGE_MODE 仅支持 capabilities 或 root"
    exit 1
    ;;
esac
chmod 0644 "$DROPIN_FILE"

if env_bool "$SYSTEMD_DAEMON_RELOAD"; then
  systemctl daemon-reload
fi

if env_bool "$RESTART_SERVICE"; then
  systemctl restart "$SERVICE_NAME"
fi

echo
echo "[OK] Linux OS 用户级进程隔离已配置"
echo "- 服务: $SERVICE_NAME"
echo "- 环境文件: $ENV_FILE"
echo "- systemd drop-in: $DROPIN_FILE"
echo "- 权限模式: $PRIVILEGE_MODE"
echo
echo "检查命令:"
echo "  systemctl cat $SERVICE_NAME"
echo "  systemctl status $SERVICE_NAME"
echo "  journalctl -u $SERVICE_NAME -f"
