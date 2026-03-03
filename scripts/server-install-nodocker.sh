#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "[ERROR] 请用 root 运行（例如: sudo bash scripts/server-install-nodocker.sh）"
  exit 1
fi

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

generate_secret() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
    return 0
  fi
  LC_ALL=C tr -dc 'a-f0-9' < /dev/urandom | head -c 64
  echo
}

need_cmd install
need_cmd rsync
need_cmd sed
need_cmd systemctl
need_cmd nginx

SOURCE_ROOT="${SOURCE_ROOT:-$(pwd)}"
APP_ROOT="${APP_ROOT:-/opt/chatos}"
SERVICE_NAME="${SERVICE_NAME:-chatos-backend}"
SERVICE_USER="${SERVICE_USER:-chatos}"
SERVICE_GROUP="${SERVICE_GROUP:-chatos}"
BACKEND_PORT="${BACKEND_PORT:-13001}"
SERVER_NAME="${SERVER_NAME:-_}"

BACKEND_BIN_SRC="${BACKEND_BIN_SRC:-$SOURCE_ROOT/chat_app_server_rs/target/release/chat_app_server_rs}"
BACKEND_CONFIG_SRC="${BACKEND_CONFIG_SRC:-$SOURCE_ROOT/chat_app_server_rs/config}"
FRONTEND_DIST_SRC="${FRONTEND_DIST_SRC:-$SOURCE_ROOT/chat_app/dist}"
SERVICE_TEMPLATE="${SERVICE_TEMPLATE:-$SOURCE_ROOT/deploy/linux/systemd/chatos-backend.service.tpl}"
NGINX_TEMPLATE="${NGINX_TEMPLATE:-$SOURCE_ROOT/deploy/linux/nginx/chatos.conf.tpl}"
ENV_TEMPLATE="${ENV_TEMPLATE:-$SOURCE_ROOT/deploy/linux/chatos-backend.env.example}"

BACKEND_DIR="$APP_ROOT/backend"
FRONTEND_DIR="$APP_ROOT/frontend"
BACKEND_BIN_DEST="$BACKEND_DIR/chat_app_server_rs"
ENV_DIR="/etc/chatos"
ENV_FILE="$ENV_DIR/chatos-backend.env"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
NGINX_SITE="/etc/nginx/sites-available/chatos.conf"
NGINX_LINK="/etc/nginx/sites-enabled/chatos.conf"

if [[ ! -f "$BACKEND_BIN_SRC" ]]; then
  echo "[ERROR] 后端二进制不存在: $BACKEND_BIN_SRC"
  echo "        先在源码目录执行: cargo build --release --manifest-path chat_app_server_rs/Cargo.toml"
  exit 1
fi

if [[ ! -d "$BACKEND_CONFIG_SRC" ]]; then
  echo "[ERROR] 后端配置目录不存在: $BACKEND_CONFIG_SRC"
  exit 1
fi

if [[ ! -d "$FRONTEND_DIST_SRC" ]]; then
  echo "[ERROR] 前端构建目录不存在: $FRONTEND_DIST_SRC"
  echo "        先在源码目录执行: npm --prefix chat_app ci && npm --prefix chat_app run build"
  exit 1
fi

if [[ ! -f "$SERVICE_TEMPLATE" || ! -f "$NGINX_TEMPLATE" || ! -f "$ENV_TEMPLATE" ]]; then
  echo "[ERROR] 部署模板文件缺失，请确认 deploy/linux 目录完整"
  exit 1
fi

if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
  groupadd --system "$SERVICE_GROUP"
fi

if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
  useradd --system --gid "$SERVICE_GROUP" --home "$APP_ROOT" --shell /usr/sbin/nologin "$SERVICE_USER"
fi

install -d -m 0755 "$APP_ROOT" "$BACKEND_DIR" "$FRONTEND_DIR"
install -d -m 0755 "$BACKEND_DIR/config" "$BACKEND_DIR/data" "$BACKEND_DIR/logs"
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$BACKEND_DIR"

install -m 0755 "$BACKEND_BIN_SRC" "$BACKEND_BIN_DEST"
rsync -a --delete "$BACKEND_CONFIG_SRC"/ "$BACKEND_DIR/config/"
rsync -a --delete "$FRONTEND_DIST_SRC"/ "$FRONTEND_DIR/"

chown "$SERVICE_USER:$SERVICE_GROUP" "$BACKEND_BIN_DEST"
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$BACKEND_DIR/config" "$BACKEND_DIR/data" "$BACKEND_DIR/logs"
chmod -R u=rwX,g=rX,o=rX "$FRONTEND_DIR"

install -d -m 0755 "$ENV_DIR"
if [[ ! -f "$ENV_FILE" || "${FORCE_ENV_REWRITE:-0}" == "1" ]]; then
  jwt_secret="$(generate_secret)"
  sed \
    -e "s|__BACKEND_PORT__|$BACKEND_PORT|g" \
    -e "s|__JWT_SECRET__|$jwt_secret|g" \
    "$ENV_TEMPLATE" > "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
  chown root:"$SERVICE_GROUP" "$ENV_FILE"
fi

sed \
  -e "s|__SERVICE_USER__|$SERVICE_USER|g" \
  -e "s|__SERVICE_GROUP__|$SERVICE_GROUP|g" \
  -e "s|__BACKEND_WORKDIR__|$BACKEND_DIR|g" \
  -e "s|__ENV_FILE__|$ENV_FILE|g" \
  -e "s|__BACKEND_BIN__|$BACKEND_BIN_DEST|g" \
  "$SERVICE_TEMPLATE" > "$SERVICE_FILE"

chmod 0644 "$SERVICE_FILE"

sed \
  -e "s|__SERVER_NAME__|$SERVER_NAME|g" \
  -e "s|__BACKEND_PORT__|$BACKEND_PORT|g" \
  -e "s|__FRONTEND_ROOT__|$FRONTEND_DIR|g" \
  "$NGINX_TEMPLATE" > "$NGINX_SITE"

ln -sfn "$NGINX_SITE" "$NGINX_LINK"

nginx -t
systemctl daemon-reload
systemctl enable --now "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"
systemctl reload nginx

echo
echo "[OK] 无 Docker 部署完成"
echo "- 后端服务: $SERVICE_NAME"
echo "- 后端监听: http://127.0.0.1:$BACKEND_PORT"
echo "- 前端目录: $FRONTEND_DIR"
echo "- 访问地址: http://$(hostname -I | awk '{print $1}')"
echo "- 环境文件: $ENV_FILE"
echo
echo "常用命令:"
echo "  systemctl status $SERVICE_NAME"
echo "  journalctl -u $SERVICE_NAME -f"
echo "  nginx -t && systemctl reload nginx"
