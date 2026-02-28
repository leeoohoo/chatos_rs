#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1; then
  echo "[ERROR] Docker 未安装，请先安装 Docker Desktop 或 Docker Engine。" >&2
  exit 1
fi

if docker compose version >/dev/null 2>&1; then
  COMPOSE_CMD=(docker compose)
elif command -v docker-compose >/dev/null 2>&1; then
  COMPOSE_CMD=(docker-compose)
else
  echo "[ERROR] 未检测到 docker compose 或 docker-compose。" >&2
  exit 1
fi

ENV_FILE="$ROOT_DIR/chat_app_server_rs/.env"
ENV_EXAMPLE="$ROOT_DIR/chat_app_server_rs/.env.example"

if [[ ! -f "$ENV_FILE" ]]; then
  cp "$ENV_EXAMPLE" "$ENV_FILE"
  echo "[INFO] 已创建 chat_app_server_rs/.env（可选配置，默认可直接启动）。"
fi

mkdir -p "$ROOT_DIR/chat_app_server_rs/data" "$ROOT_DIR/chat_app_server_rs/logs"

"${COMPOSE_CMD[@]}" --env-file "$ENV_FILE" up -d --build

echo
echo "[OK] 服务已启动"
echo "- 前端: http://localhost:8080"
echo "- 后端健康检查: http://localhost:3001/health"
echo "- API Key: 可在前端“模型配置”页面按模型填写；OPENAI_API_KEY 仅作兜底"
echo "- 查看日志: ${COMPOSE_CMD[*]} --env-file $ENV_FILE logs -f"
