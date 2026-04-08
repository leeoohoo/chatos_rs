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

ENV_FILE="$ROOT_DIR/agent_orchestrator/.env"
ENV_EXAMPLE="$ROOT_DIR/agent_orchestrator/.env.example"

if [[ ! -f "$ENV_FILE" ]]; then
  cp "$ENV_EXAMPLE" "$ENV_FILE"
  echo "[INFO] 已创建 agent_orchestrator/.env（可选配置，默认可直接启动）。"
fi

"${COMPOSE_CMD[@]}" --env-file "$ENV_FILE" up -d --build

BACKEND_HOST_PORT="$(grep -E '^BACKEND_HOST_PORT=' "$ENV_FILE" | tail -n1 | cut -d'=' -f2- | tr -d '[:space:]')"
if [[ -z "$BACKEND_HOST_PORT" ]]; then
  BACKEND_HOST_PORT=3001
fi

echo
echo "[OK] 服务已启动"
echo "- 前端: http://localhost:8080"
echo "- 后端健康检查: http://localhost:${BACKEND_HOST_PORT}/health"
echo "- API Key: 可在前端“模型配置”页面按模型填写；OPENAI_API_KEY 仅作兜底"
echo "- 查看日志: ${COMPOSE_CMD[*]} --env-file $ENV_FILE logs -f"
