#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
AGENTFORGE_DIR="$ROOT_DIR/agentforge"
FRONTEND_DIR="$ROOT_DIR/chat_app"
BINARY_DIR="$ROOT_DIR/target-shared/debug"

load_optional_env() {
  local env_file="$1"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

load_optional_env "$ROOT_DIR/.env"
load_optional_env "$AGENTFORGE_DIR/.env"

if command -v shasum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | shasum | awk '{print substr($1,1,8)}')"
elif command -v sha1sum >/dev/null 2>&1; then
  ROOT_HASH="$(printf '%s' "$ROOT_DIR" | sha1sum | awk '{print substr($1,1,8)}')"
else
  ROOT_HASH="default"
fi

RUNTIME_DIR="${AGENTFORGE_RUNTIME_DIR:-/tmp/chatos_rs_agentforge_${ROOT_HASH}}"
DATA_DIR="${AGENTFORGE_DATA_DIR:-/tmp/chatos_rs_agentforge_data_${ROOT_HASH}}"
STOP_BY_PORT="${STOP_BY_PORT:-0}"

AGENTFORGE_GATEWAY_PORT="${AGENTFORGE_GATEWAY_PORT:-${MAIN_BACKEND_PORT:-${BACKEND_PORT:-3997}}}"
AGENTFORGE_FRONTEND_PORT="${AGENTFORGE_FRONTEND_PORT:-${FRONTEND_PORT:-8088}}"
AGENTFORGE_RNACOS_PORT="${AGENTFORGE_RNACOS_PORT:-${RNACOS_PORT:-8848}}"
CONVERSATION_SERVICE_PORT="${CONVERSATION_SERVICE_PORT:-4101}"
MEMORY_ADAPTER_SERVICE_PORT="${MEMORY_ADAPTER_SERVICE_PORT:-4102}"
AGENT_SKILL_SERVICE_PORT="${AGENT_SKILL_SERVICE_PORT:-4103}"
PLATFORM_CONFIG_SERVICE_PORT="${PLATFORM_CONFIG_SERVICE_PORT:-4104}"
WORKSPACE_SERVICE_PORT="${WORKSPACE_SERVICE_PORT:-4105}"
EXECUTION_SERVICE_PORT="${EXECUTION_SERVICE_PORT:-4106}"

AGENTFORGE_SERVICE_DISCOVERY="${AGENTFORGE_SERVICE_DISCOVERY:-static}"
if [[ "${AGENTFORGE_MANAGE_RNACOS+x}" == "x" ]]; then
  AGENTFORGE_MANAGE_RNACOS="${AGENTFORGE_MANAGE_RNACOS}"
elif [[ "$AGENTFORGE_SERVICE_DISCOVERY" == "r-nacos" ]]; then
  AGENTFORGE_MANAGE_RNACOS=1
else
  AGENTFORGE_MANAGE_RNACOS=0
fi
AGENTFORGE_NACOS_ADDR="${AGENTFORGE_NACOS_ADDR:-${NACOS_ADDR:-http://127.0.0.1:${AGENTFORGE_RNACOS_PORT}}}"
AGENTFORGE_NACOS_NAMESPACE="${AGENTFORGE_NACOS_NAMESPACE:-${NACOS_NAMESPACE:-chatos-dev}}"
AGENTFORGE_NACOS_GROUP="${AGENTFORGE_NACOS_GROUP:-${NACOS_GROUP:-DEFAULT_GROUP}}"
AGENTFORGE_NACOS_HEARTBEAT_SECONDS="${AGENTFORGE_NACOS_HEARTBEAT_SECONDS:-${NACOS_HEARTBEAT_SECONDS:-5}}"
AGENTFORGE_INTERNAL_CALL_TIMEOUT_MS="${AGENTFORGE_INTERNAL_CALL_TIMEOUT_MS:-${INTERNAL_CALL_TIMEOUT_MS:-3000}}"
AGENTFORGE_SERVICE_REGISTER_HOST="${AGENTFORGE_SERVICE_REGISTER_HOST:-${SERVICE_REGISTER_HOST:-127.0.0.1}}"

AGENTFORGE_BUILD_LOG_FILE="$RUNTIME_DIR/agentforge-build.log"
CHAT_API_GATEWAY_PID_FILE="$RUNTIME_DIR/chat-api-gateway.pid"
CHAT_API_GATEWAY_LOG_FILE="$RUNTIME_DIR/chat-api-gateway.log"
CONVERSATION_SERVICE_PID_FILE="$RUNTIME_DIR/conversation-service.pid"
CONVERSATION_SERVICE_LOG_FILE="$RUNTIME_DIR/conversation-service.log"
MEMORY_ADAPTER_SERVICE_PID_FILE="$RUNTIME_DIR/memory-adapter-service.pid"
MEMORY_ADAPTER_SERVICE_LOG_FILE="$RUNTIME_DIR/memory-adapter-service.log"
AGENT_SKILL_SERVICE_PID_FILE="$RUNTIME_DIR/agent-skill-service.pid"
AGENT_SKILL_SERVICE_LOG_FILE="$RUNTIME_DIR/agent-skill-service.log"
PLATFORM_CONFIG_SERVICE_PID_FILE="$RUNTIME_DIR/platform-config-service.pid"
PLATFORM_CONFIG_SERVICE_LOG_FILE="$RUNTIME_DIR/platform-config-service.log"
WORKSPACE_SERVICE_PID_FILE="$RUNTIME_DIR/workspace-service.pid"
WORKSPACE_SERVICE_LOG_FILE="$RUNTIME_DIR/workspace-service.log"
EXECUTION_SERVICE_PID_FILE="$RUNTIME_DIR/execution-service.pid"
EXECUTION_SERVICE_LOG_FILE="$RUNTIME_DIR/execution-service.log"
FRONTEND_PID_FILE="$RUNTIME_DIR/chat-app.pid"
FRONTEND_LOG_FILE="$RUNTIME_DIR/chat-app.log"

CHAT_API_GATEWAY_BINARY="$BINARY_DIR/agentforge-chat-api-gateway"
CONVERSATION_SERVICE_BINARY="$BINARY_DIR/agentforge-conversation-service"
MEMORY_ADAPTER_SERVICE_BINARY="$BINARY_DIR/agentforge-memory-adapter-service"
AGENT_SKILL_SERVICE_BINARY="$BINARY_DIR/agentforge-agent-skill-service"
PLATFORM_CONFIG_SERVICE_BINARY="$BINARY_DIR/agentforge-platform-config-service"
WORKSPACE_SERVICE_BINARY="$BINARY_DIR/agentforge-workspace-service"
EXECUTION_SERVICE_BINARY="$BINARY_DIR/agentforge-execution-service"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

docker_compose() {
  (
    cd "$AGENTFORGE_DIR"
    docker compose "$@"
  )
}

stop_from_pid_file() {
  local name="$1"
  local pid_file="$2"
  if [[ ! -f "$pid_file" ]]; then
    return
  fi
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    echo "[INFO] 停止 $name (pid=$pid)"
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
  fi
  rm -f "$pid_file"
}

stop_from_port() {
  local name="$1"
  local port="$2"

  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      echo "[INFO] 停止占用端口 $port 的 $name 进程: $pids"
      kill $pids >/dev/null 2>&1 || true
      sleep 1
      local left
      left="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
      if [[ -n "$left" ]]; then
        kill -9 $left >/dev/null 2>&1 || true
      fi
    fi
  elif command -v fuser >/dev/null 2>&1; then
    if fuser -n tcp "$port" >/dev/null 2>&1; then
      echo "[INFO] 停止占用端口 $port 的 $name 进程"
      fuser -k -n tcp "$port" >/dev/null 2>&1 || true
    fi
  fi
}

stop_project_owned_port_processes() {
  local name="$1"
  local port="$2"

  if ! command -v lsof >/dev/null 2>&1; then
    return
  fi

  local pids
  pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -z "$pids" ]]; then
    return
  fi

  local pid cwd_path
  for pid in $pids; do
    cwd_path="$(lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1)"
    if [[ -z "$cwd_path" ]]; then
      continue
    fi
    if [[ "$cwd_path" == "$ROOT_DIR"* ]]; then
      echo "[INFO] 停止当前项目残留的 $name 进程 (pid=$pid, port=$port, cwd=$cwd_path)"
      kill "$pid" >/dev/null 2>&1 || true
      sleep 1
      if kill -0 "$pid" >/dev/null 2>&1; then
        kill -9 "$pid" >/dev/null 2>&1 || true
      fi
    fi
  done
}

is_port_listening() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -n "$pids" ]]
    return
  fi
  if command -v fuser >/dev/null 2>&1; then
    fuser -n tcp "$port" >/dev/null 2>&1
    return
  fi
  return 1
}

ensure_port_available() {
  local name="$1"
  local port="$2"
  if is_port_listening "$port"; then
    echo "[ERROR] $name 端口已被占用: $port"
    if command -v lsof >/dev/null 2>&1; then
      echo "[INFO] 当前占用详情："
      lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
    fi
    return 1
  fi
}

launch_service() {
  local name="$1"
  local port="$2"
  local pid_file="$3"
  local log_file="$4"
  local command="$5"

  ensure_port_available "$name" "$port" || return 1
  echo "[INFO] 启动 $name..."
  : >"$log_file"
  nohup bash -lc "$command" >"$log_file" 2>&1 &
  echo $! >"$pid_file"
}

check_alive() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "[ERROR] $name 启动失败，请检查日志: $log_file"
    tail -n 80 "$log_file" 2>/dev/null || true
    return 1
  fi
}

wait_http_ready() {
  local name="$1"
  local url="$2"
  local timeout_sec="${3:-30}"

  if ! command -v curl >/dev/null 2>&1; then
    echo "[WARN] 未找到 curl，跳过 $name 健康检查: $url"
    return 0
  fi

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while true; do
    if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
      echo "[INFO] $name 健康检查通过: $url"
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name 健康检查超时 (${timeout_sec}s): $url"
      return 1
    fi
    sleep 1
  done
}

wait_port_released() {
  local name="$1"
  local port="$2"
  local timeout_sec="${3:-15}"

  local start_ts now_ts elapsed
  start_ts="$(date +%s)"

  while is_port_listening "$port"; do
    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] $name 端口未在预期时间内释放: $port"
      if command -v lsof >/dev/null 2>&1; then
        lsof -nP -iTCP:"$port" -sTCP:LISTEN || true
      fi
      return 1
    fi
    sleep 1
  done
}

wait_nacos_registration() {
  local service_name="$1"
  local timeout_sec="${2:-30}"

  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" != "r-nacos" ]]; then
    return 0
  fi
  if ! command -v curl >/dev/null 2>&1; then
    echo "[WARN] 未找到 curl，跳过 r-nacos 注册检查: $service_name"
    return 0
  fi

  local url="${AGENTFORGE_NACOS_ADDR}/nacos/v1/ns/instance/list?serviceName=${service_name}&namespaceId=${AGENTFORGE_NACOS_NAMESPACE}&groupName=${AGENTFORGE_NACOS_GROUP}&healthyOnly=true"
  local start_ts now_ts elapsed body
  start_ts="$(date +%s)"

  while true; do
    body="$(curl -fsS --max-time 2 "$url" 2>/dev/null || true)"
    if [[ -n "$body" ]] && printf '%s' "$body" | grep -Eq '"healthy"[[:space:]]*:[[:space:]]*true'; then
      echo "[INFO] r-nacos 已注册服务: $service_name"
      return 0
    fi

    now_ts="$(date +%s)"
    elapsed="$((now_ts - start_ts))"
    if (( elapsed >= timeout_sec )); then
      echo "[ERROR] r-nacos 注册检查超时 (${timeout_sec}s): $service_name"
      [[ -n "$body" ]] && echo "$body"
      return 1
    fi
    sleep 1
  done
}

agentforge_common_env() {
  local service_name="$1"
  local port="$2"
  printf '%s' \
    "SERVICE_NAME=\"$service_name\" " \
    "SERVICE_HOST=\"0.0.0.0\" " \
    "SERVICE_PORT=\"$port\" " \
    "SERVICE_DISCOVERY=\"$AGENTFORGE_SERVICE_DISCOVERY\" " \
    "NACOS_ADDR=\"$AGENTFORGE_NACOS_ADDR\" " \
    "NACOS_NAMESPACE=\"$AGENTFORGE_NACOS_NAMESPACE\" " \
    "NACOS_GROUP=\"$AGENTFORGE_NACOS_GROUP\" " \
    "SERVICE_REGISTER_HOST=\"$AGENTFORGE_SERVICE_REGISTER_HOST\" " \
    "SERVICE_REGISTER_PORT=\"$port\" " \
    "NACOS_HEARTBEAT_SECONDS=\"$AGENTFORGE_NACOS_HEARTBEAT_SECONDS\" " \
    "INTERNAL_CALL_TIMEOUT_MS=\"$AGENTFORGE_INTERNAL_CALL_TIMEOUT_MS\" " \
    "STATIC_SERVICE_HOST=\"127.0.0.1\" " \
    "STATIC_SERVICE_CHAT_API_GATEWAY_URL=\"http://127.0.0.1:${AGENTFORGE_GATEWAY_PORT}\" " \
    "STATIC_SERVICE_CONVERSATION_SERVICE_URL=\"http://127.0.0.1:${CONVERSATION_SERVICE_PORT}\" " \
    "STATIC_SERVICE_MEMORY_ADAPTER_SERVICE_URL=\"http://127.0.0.1:${MEMORY_ADAPTER_SERVICE_PORT}\" " \
    "STATIC_SERVICE_AGENT_SKILL_SERVICE_URL=\"http://127.0.0.1:${AGENT_SKILL_SERVICE_PORT}\" " \
    "STATIC_SERVICE_PLATFORM_CONFIG_SERVICE_URL=\"http://127.0.0.1:${PLATFORM_CONFIG_SERVICE_PORT}\" " \
    "STATIC_SERVICE_WORKSPACE_SERVICE_URL=\"http://127.0.0.1:${WORKSPACE_SERVICE_PORT}\" " \
    "STATIC_SERVICE_EXECUTION_SERVICE_URL=\"http://127.0.0.1:${EXECUTION_SERVICE_PORT}\" "
}

build_agentforge_binaries() {
  echo "[INFO] 构建 AgentForge binaries..."
  : >"$AGENTFORGE_BUILD_LOG_FILE"
  if ! bash -lc "cd \"$AGENTFORGE_DIR\" && cargo build -p agentforge-chat-api-gateway -p agentforge-conversation-service -p agentforge-memory-adapter-service -p agentforge-agent-skill-service -p agentforge-platform-config-service -p agentforge-workspace-service -p agentforge-execution-service" >"$AGENTFORGE_BUILD_LOG_FILE" 2>&1; then
    echo "[ERROR] AgentForge 构建失败，请检查日志: $AGENTFORGE_BUILD_LOG_FILE"
    tail -n 120 "$AGENTFORGE_BUILD_LOG_FILE" 2>/dev/null || true
    return 1
  fi
}

start_rnacos() {
  local timeout_sec="${AGENTFORGE_RNACOS_HEALTHCHECK_TIMEOUT_SEC:-45}"
  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" != "r-nacos" ]]; then
    echo "[INFO] 跳过 r-nacos，当前使用静态服务发现: $AGENTFORGE_SERVICE_DISCOVERY"
    return 0
  fi
  if [[ "$AGENTFORGE_MANAGE_RNACOS" != "1" ]]; then
    echo "[INFO] 使用外部 r-nacos: $AGENTFORGE_NACOS_ADDR"
    wait_http_ready "外部 r-nacos" "${AGENTFORGE_NACOS_ADDR}/nacos/v1/ns/operator/metrics" "$timeout_sec"
    return
  fi

  need_cmd docker
  echo "[INFO] 启动 r-nacos..."
  docker_compose up -d r-nacos >/dev/null
  wait_http_ready "r-nacos" "${AGENTFORGE_NACOS_ADDR}/nacos/v1/ns/operator/metrics" "$timeout_sec"
}

stop_rnacos() {
  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" != "r-nacos" ]]; then
    return
  fi
  if [[ "$AGENTFORGE_MANAGE_RNACOS" != "1" ]]; then
    return
  fi
  if ! command -v docker >/dev/null 2>&1; then
    return
  fi
  echo "[INFO] 停止 r-nacos..."
  docker_compose stop r-nacos >/dev/null 2>&1 || true
}

start_memory_adapter_service() {
  launch_service \
    "memory-adapter-service" \
    "$MEMORY_ADAPTER_SERVICE_PORT" \
    "$MEMORY_ADAPTER_SERVICE_PID_FILE" \
    "$MEMORY_ADAPTER_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PORT") MEMORY_MAPPING_STORE_PATH=\"$DATA_DIR/memory_mappings.json\" \"$MEMORY_ADAPTER_SERVICE_BINARY\""
}

start_agent_skill_service() {
  launch_service \
    "agent-skill-service" \
    "$AGENT_SKILL_SERVICE_PORT" \
    "$AGENT_SKILL_SERVICE_PID_FILE" \
    "$AGENT_SKILL_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "agent-skill-service" "$AGENT_SKILL_SERVICE_PORT") AGENT_STORE_PATH=\"$DATA_DIR/agents.json\" SKILL_CATALOG_STORE_PATH=\"$DATA_DIR/skill_catalog.json\" AGENT_SKILL_STATE_ROOT=\"$DATA_DIR/skill_state\" AGENT_SKILL_GIT_CACHE_ROOT=\"$DATA_DIR/skill_git_cache\" \"$AGENT_SKILL_SERVICE_BINARY\""
}

start_platform_config_service() {
  launch_service \
    "platform-config-service" \
    "$PLATFORM_CONFIG_SERVICE_PORT" \
    "$PLATFORM_CONFIG_SERVICE_PID_FILE" \
    "$PLATFORM_CONFIG_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PORT") USER_SETTINGS_STORE_PATH=\"$DATA_DIR/user_settings.json\" PLATFORM_CATALOG_STORE_PATH=\"$DATA_DIR/platform_catalog.json\" AUTH_USERS_STORE_PATH=\"$DATA_DIR/auth_users.json\" \"$PLATFORM_CONFIG_SERVICE_BINARY\""
}

start_workspace_service() {
  launch_service \
    "workspace-service" \
    "$WORKSPACE_SERVICE_PORT" \
    "$WORKSPACE_SERVICE_PID_FILE" \
    "$WORKSPACE_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "workspace-service" "$WORKSPACE_SERVICE_PORT") PROJECT_STORE_PATH=\"$DATA_DIR/projects.json\" WORKSPACE_NOTEPAD_ROOT=\"$DATA_DIR/notepad\" \"$WORKSPACE_SERVICE_BINARY\""
}

start_execution_service() {
  launch_service \
    "execution-service" \
    "$EXECUTION_SERVICE_PORT" \
    "$EXECUTION_SERVICE_PID_FILE" \
    "$EXECUTION_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "execution-service" "$EXECUTION_SERVICE_PORT") EXECUTION_STORE_PATH=\"$DATA_DIR/execution.json\" \"$EXECUTION_SERVICE_BINARY\""
}

start_conversation_service() {
  launch_service \
    "conversation-service" \
    "$CONVERSATION_SERVICE_PORT" \
    "$CONVERSATION_SERVICE_PID_FILE" \
    "$CONVERSATION_SERVICE_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "conversation-service" "$CONVERSATION_SERVICE_PORT") CONVERSATION_STORE_PATH=\"$DATA_DIR/conversation.json\" \"$CONVERSATION_SERVICE_BINARY\""
}

start_chat_api_gateway() {
  launch_service \
    "chat-api-gateway" \
    "$AGENTFORGE_GATEWAY_PORT" \
    "$CHAT_API_GATEWAY_PID_FILE" \
    "$CHAT_API_GATEWAY_LOG_FILE" \
    "cd \"$AGENTFORGE_DIR\" && exec env $(agentforge_common_env "chat-api-gateway" "$AGENTFORGE_GATEWAY_PORT") \"$CHAT_API_GATEWAY_BINARY\""
}

start_frontend() {
  launch_service \
    "chat_app frontend" \
    "$AGENTFORGE_FRONTEND_PORT" \
    "$FRONTEND_PID_FILE" \
    "$FRONTEND_LOG_FILE" \
    "cd \"$FRONTEND_DIR\" && exec env VITE_API_BASE_URL=\"http://127.0.0.1:${AGENTFORGE_GATEWAY_PORT}/api\" npm run dev -- --host 0.0.0.0 --port \"$AGENTFORGE_FRONTEND_PORT\""
}

start_service_and_wait() {
  local launch_fn="$1"
  local name="$2"
  local pid_file="$3"
  local log_file="$4"
  local port="$5"
  local service_name="$6"
  local timeout_sec="$7"

  "$launch_fn" &&
    sleep 1 &&
    check_alive "$name" "$pid_file" "$log_file" &&
    wait_http_ready "$name" "http://127.0.0.1:$port/health" "$timeout_sec" &&
    wait_nacos_registration "$service_name" "$timeout_sec"
}

stop_managed_services() {
  stop_from_pid_file "chat-api-gateway" "$CHAT_API_GATEWAY_PID_FILE"
  stop_from_pid_file "conversation-service" "$CONVERSATION_SERVICE_PID_FILE"
  stop_from_pid_file "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PID_FILE"
  stop_from_pid_file "agent-skill-service" "$AGENT_SKILL_SERVICE_PID_FILE"
  stop_from_pid_file "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PID_FILE"
  stop_from_pid_file "workspace-service" "$WORKSPACE_SERVICE_PID_FILE"
  stop_from_pid_file "execution-service" "$EXECUTION_SERVICE_PID_FILE"
  stop_from_pid_file "chat_app frontend" "$FRONTEND_PID_FILE"
}

stop_project_services_by_ports() {
  local ports=(
    "$AGENTFORGE_GATEWAY_PORT"
    "$CONVERSATION_SERVICE_PORT"
    "$MEMORY_ADAPTER_SERVICE_PORT"
    "$AGENT_SKILL_SERVICE_PORT"
    "$PLATFORM_CONFIG_SERVICE_PORT"
    "$WORKSPACE_SERVICE_PORT"
    "$EXECUTION_SERVICE_PORT"
    "$AGENTFORGE_FRONTEND_PORT"
  )
  local labels=(
    "chat-api-gateway"
    "conversation-service"
    "memory-adapter-service"
    "agent-skill-service"
    "platform-config-service"
    "workspace-service"
    "execution-service"
    "chat_app frontend"
  )
  local i
  for ((i = 0; i < ${#ports[@]}; i++)); do
    if [[ "$STOP_BY_PORT" == "1" ]]; then
      stop_from_port "${labels[$i]}" "${ports[$i]}"
    fi
    stop_project_owned_port_processes "${labels[$i]}" "${ports[$i]}"
  done
}

wait_all_service_ports_released() {
  local timeout_sec="${AGENTFORGE_STOP_TIMEOUT_SEC:-20}"
  wait_port_released "chat-api-gateway" "$AGENTFORGE_GATEWAY_PORT" "$timeout_sec" || return 1
  wait_port_released "conversation-service" "$CONVERSATION_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "agent-skill-service" "$AGENT_SKILL_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "workspace-service" "$WORKSPACE_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "execution-service" "$EXECUTION_SERVICE_PORT" "$timeout_sec" || return 1
  wait_port_released "chat_app frontend" "$AGENTFORGE_FRONTEND_PORT" "$timeout_sec" || return 1
}

ensure_start_ports_available() {
  ensure_port_available "chat-api-gateway" "$AGENTFORGE_GATEWAY_PORT" || return 1
  ensure_port_available "conversation-service" "$CONVERSATION_SERVICE_PORT" || return 1
  ensure_port_available "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PORT" || return 1
  ensure_port_available "agent-skill-service" "$AGENT_SKILL_SERVICE_PORT" || return 1
  ensure_port_available "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PORT" || return 1
  ensure_port_available "workspace-service" "$WORKSPACE_SERVICE_PORT" || return 1
  ensure_port_available "execution-service" "$EXECUTION_SERVICE_PORT" || return 1
  ensure_port_available "chat_app frontend" "$AGENTFORGE_FRONTEND_PORT" || return 1
}

prepare() {
  need_cmd bash
  need_cmd npm
  need_cmd cargo

  if [[ ! -d "$AGENTFORGE_DIR" || ! -d "$FRONTEND_DIR" ]]; then
    echo "[ERROR] 项目目录不完整: $AGENTFORGE_DIR / $FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"
  mkdir -p "$DATA_DIR"
  mkdir -p "$DATA_DIR/skill_state"
  mkdir -p "$DATA_DIR/notepad"
}

do_stop() {
  stop_managed_services
  stop_project_services_by_ports
  wait_all_service_ports_released
  stop_rnacos
}

run_start_sequence() {
  local service_timeout="${AGENTFORGE_SERVICE_HEALTHCHECK_TIMEOUT_SEC:-90}"
  local frontend_timeout="${AGENTFORGE_FRONTEND_HEALTHCHECK_TIMEOUT_SEC:-45}"

  ensure_start_ports_available &&
    start_rnacos &&
    start_service_and_wait start_memory_adapter_service "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PID_FILE" "$MEMORY_ADAPTER_SERVICE_LOG_FILE" "$MEMORY_ADAPTER_SERVICE_PORT" "memory-adapter-service" "$service_timeout" &&
    start_service_and_wait start_agent_skill_service "agent-skill-service" "$AGENT_SKILL_SERVICE_PID_FILE" "$AGENT_SKILL_SERVICE_LOG_FILE" "$AGENT_SKILL_SERVICE_PORT" "agent-skill-service" "$service_timeout" &&
    start_service_and_wait start_platform_config_service "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PID_FILE" "$PLATFORM_CONFIG_SERVICE_LOG_FILE" "$PLATFORM_CONFIG_SERVICE_PORT" "platform-config-service" "$service_timeout" &&
    start_service_and_wait start_workspace_service "workspace-service" "$WORKSPACE_SERVICE_PID_FILE" "$WORKSPACE_SERVICE_LOG_FILE" "$WORKSPACE_SERVICE_PORT" "workspace-service" "$service_timeout" &&
    start_service_and_wait start_execution_service "execution-service" "$EXECUTION_SERVICE_PID_FILE" "$EXECUTION_SERVICE_LOG_FILE" "$EXECUTION_SERVICE_PORT" "execution-service" "$service_timeout" &&
    start_service_and_wait start_conversation_service "conversation-service" "$CONVERSATION_SERVICE_PID_FILE" "$CONVERSATION_SERVICE_LOG_FILE" "$CONVERSATION_SERVICE_PORT" "conversation-service" "$service_timeout" &&
    start_service_and_wait start_chat_api_gateway "chat-api-gateway" "$CHAT_API_GATEWAY_PID_FILE" "$CHAT_API_GATEWAY_LOG_FILE" "$AGENTFORGE_GATEWAY_PORT" "chat-api-gateway" "$service_timeout" &&
    start_frontend &&
    sleep 2 &&
    check_alive "chat_app frontend" "$FRONTEND_PID_FILE" "$FRONTEND_LOG_FILE" &&
    wait_http_ready "chat_app frontend" "http://127.0.0.1:$AGENTFORGE_FRONTEND_PORT" "$frontend_timeout"
}

service_status_line() {
  local name="$1"
  local pid_file="$2"
  local port="$3"
  local log_file="$4"
  local pid state

  pid="$(cat "$pid_file" 2>/dev/null || true)"
  state="stopped"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    state="running"
  fi

  echo "  $name:"
  echo "    pid: ${pid:-N/A}"
  echo "    port: $port"
  echo "    state: $state"
  echo "    log: $log_file"
}

print_runtime_info() {
  echo "[OK] AgentForge 本地链路已在后台运行"
  echo "  runtime dir: $RUNTIME_DIR"
  echo "  data dir: $DATA_DIR"
  echo "  discovery: $AGENTFORGE_SERVICE_DISCOVERY"
  echo
  echo "  gateway url: http://localhost:$AGENTFORGE_GATEWAY_PORT"
  echo "  frontend url: http://localhost:$AGENTFORGE_FRONTEND_PORT"
  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" == "r-nacos" ]]; then
    echo "  r-nacos url: $AGENTFORGE_NACOS_ADDR"
  fi
  echo
  echo "  build log: $AGENTFORGE_BUILD_LOG_FILE"
}

status() {
  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "[INFO] data dir: $DATA_DIR"
  echo "[INFO] discovery: $AGENTFORGE_SERVICE_DISCOVERY"
  echo "[INFO] gateway url: http://localhost:$AGENTFORGE_GATEWAY_PORT"
  echo "[INFO] frontend url: http://localhost:$AGENTFORGE_FRONTEND_PORT"
  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" == "r-nacos" ]]; then
    echo "[INFO] r-nacos url: $AGENTFORGE_NACOS_ADDR"
    echo
  fi
  if [[ "$AGENTFORGE_SERVICE_DISCOVERY" == "r-nacos" ]] && command -v curl >/dev/null 2>&1; then
    if curl -fsS --max-time 2 "${AGENTFORGE_NACOS_ADDR}/nacos/v1/ns/operator/metrics" >/dev/null 2>&1; then
      echo "  r-nacos: healthy"
    else
      echo "  r-nacos: unavailable"
    fi
  fi
  echo
  service_status_line "chat-api-gateway" "$CHAT_API_GATEWAY_PID_FILE" "$AGENTFORGE_GATEWAY_PORT" "$CHAT_API_GATEWAY_LOG_FILE"
  service_status_line "conversation-service" "$CONVERSATION_SERVICE_PID_FILE" "$CONVERSATION_SERVICE_PORT" "$CONVERSATION_SERVICE_LOG_FILE"
  service_status_line "memory-adapter-service" "$MEMORY_ADAPTER_SERVICE_PID_FILE" "$MEMORY_ADAPTER_SERVICE_PORT" "$MEMORY_ADAPTER_SERVICE_LOG_FILE"
  service_status_line "agent-skill-service" "$AGENT_SKILL_SERVICE_PID_FILE" "$AGENT_SKILL_SERVICE_PORT" "$AGENT_SKILL_SERVICE_LOG_FILE"
  service_status_line "platform-config-service" "$PLATFORM_CONFIG_SERVICE_PID_FILE" "$PLATFORM_CONFIG_SERVICE_PORT" "$PLATFORM_CONFIG_SERVICE_LOG_FILE"
  service_status_line "workspace-service" "$WORKSPACE_SERVICE_PID_FILE" "$WORKSPACE_SERVICE_PORT" "$WORKSPACE_SERVICE_LOG_FILE"
  service_status_line "execution-service" "$EXECUTION_SERVICE_PID_FILE" "$EXECUTION_SERVICE_PORT" "$EXECUTION_SERVICE_LOG_FILE"
  service_status_line "chat_app frontend" "$FRONTEND_PID_FILE" "$AGENTFORGE_FRONTEND_PORT" "$FRONTEND_LOG_FILE"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart)
    if build_agentforge_binaries; then
      do_stop
      if run_start_sequence; then
        print_runtime_info
      else
        echo "[WARN] 启动失败，正在回滚已启动的服务..."
        do_stop || true
        exit 1
      fi
    else
      exit 1
    fi
    ;;
  start)
    if run_start_sequence; then
      print_runtime_info
    else
      echo "[WARN] 启动失败，正在回滚已启动的服务..."
      do_stop || true
      exit 1
    fi
    ;;
  stop)
    do_stop
    echo "[OK] AgentForge 本地链路已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
