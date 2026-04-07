#!/usr/bin/env bash
if [[ -z "${CHATOS_RS_SHELL_SANITIZED-}" ]]; then export CHATOS_RS_SHELL_SANITIZED=1; export CHATOS_RS_SCRIPT_PATH="$0"; exec bash <(tr -d '\r' < "$0") "$@"; fi # CRLF-safe bootstrap for `bash restart_services.sh` #

set -euo pipefail

SCRIPT_PATH="${CHATOS_RS_SCRIPT_PATH:-${BASH_SOURCE[0]}}"
ROOT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
MAIN_BACKEND_DIR="$ROOT_DIR/chat_app_server_rs"
MAIN_FRONTEND_DIR="$ROOT_DIR/chat_app"
MAIN_BACKEND_ENV_FILE="$MAIN_BACKEND_DIR/.env"
MEMORY_ROOT_DIR="$ROOT_DIR/memory_server"
MEMORY_BACKEND_DIR="$MEMORY_ROOT_DIR/backend"
MEMORY_FRONTEND_DIR="$MEMORY_ROOT_DIR/frontend"
MEMORY_BACKEND_ENV_FILE="$MEMORY_BACKEND_DIR/.env"
IM_ROOT_DIR="$ROOT_DIR/im_service"
IM_BACKEND_DIR="$IM_ROOT_DIR/backend"
IM_BACKEND_ENV_FILE="$IM_BACKEND_DIR/.env"
TASK_ROOT_DIR="$ROOT_DIR/contact_task_service"
TASK_BACKEND_DIR="$TASK_ROOT_DIR/backend"
TASK_FRONTEND_DIR="$TASK_ROOT_DIR/frontend"
TASK_BACKEND_ENV_FILE="$TASK_BACKEND_DIR/.env"

MAIN_BACKEND_PORT="${BACKEND_PORT:-3001}"
MAIN_FRONTEND_PORT="${FRONTEND_PORT:-8088}"
MEMORY_BACKEND_PORT="${MEMORY_SERVER_BACKEND_PORT:-}"
MEMORY_FRONTEND_PORT="${MEMORY_SERVER_FRONTEND_PORT:-5176}"
MEMORY_FRONTEND_HOST="${MEMORY_SERVER_FRONTEND_HOST:-0.0.0.0}"
IM_BACKEND_PORT="${IM_SERVICE_PORT:-}"
TASK_BACKEND_PORT="${CONTACT_TASK_SERVICE_PORT:-}"
TASK_FRONTEND_PORT="${CONTACT_TASK_SERVICE_FRONTEND_PORT:-5177}"
TASK_FRONTEND_HOST="${CONTACT_TASK_SERVICE_FRONTEND_HOST:-0.0.0.0}"

RUNTIME_DIR="${RUNTIME_DIR:-$ROOT_DIR/logs}"
MAIN_BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
MAIN_FRONTEND_PID_FILE="$RUNTIME_DIR/frontend.pid"
MEMORY_BACKEND_PID_FILE="$RUNTIME_DIR/memory_backend.pid"
MEMORY_FRONTEND_PID_FILE="$RUNTIME_DIR/memory_frontend.pid"
IM_BACKEND_PID_FILE="$RUNTIME_DIR/im_backend.pid"
TASK_BACKEND_PID_FILE="$RUNTIME_DIR/task_backend.pid"
TASK_FRONTEND_PID_FILE="$RUNTIME_DIR/task_frontend.pid"
MAIN_BACKEND_LOG_FILE="$RUNTIME_DIR/backend.log"
MAIN_FRONTEND_LOG_FILE="$RUNTIME_DIR/frontend.log"
MEMORY_BACKEND_LOG_FILE="$RUNTIME_DIR/memory_backend.log"
MEMORY_FRONTEND_LOG_FILE="$RUNTIME_DIR/memory_frontend.log"
IM_BACKEND_LOG_FILE="$RUNTIME_DIR/im_backend.log"
TASK_BACKEND_LOG_FILE="$RUNTIME_DIR/task_backend.log"
TASK_FRONTEND_LOG_FILE="$RUNTIME_DIR/task_frontend.log"

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[ERROR] 缺少命令: $cmd"
    exit 1
  fi
}

ensure_vite_dependencies() {
  local name="$1"
  local dir="$2"
  if [[ -x "$dir/node_modules/.bin/vite" ]]; then
    return
  fi

  echo "[INFO] $name 依赖未安装，执行 npm install..."
  (
    cd "$dir"
    npm install
  )
}

read_memory_port_from_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    return
  fi
  local port
  port="$(grep -E '^[[:space:]]*MEMORY_SERVER_PORT=' "$env_file" | tail -n 1 | cut -d '=' -f 2- | tr -d '"' | tr -d "'" | tr -d '[:space:]' || true)"
  if [[ -n "$port" ]]; then
    MEMORY_BACKEND_PORT="$port"
  fi
}

read_task_port_from_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    return
  fi
  local port
  port="$(grep -E '^[[:space:]]*CONTACT_TASK_SERVICE_PORT=' "$env_file" | tail -n 1 | cut -d '=' -f 2- | tr -d '"' | tr -d "'" | tr -d '[:space:]' || true)"
  if [[ -n "$port" ]]; then
    TASK_BACKEND_PORT="$port"
  fi
}

read_im_port_from_env_file() {
  local env_file="$1"
  if [[ ! -f "$env_file" ]]; then
    return
  fi
  local port
  port="$(grep -E '^[[:space:]]*IM_SERVICE_PORT=' "$env_file" | tail -n 1 | cut -d '=' -f 2- | tr -d '"' | tr -d "'" | tr -d '[:space:]' || true)"
  if [[ -n "$port" ]]; then
    IM_BACKEND_PORT="$port"
  fi
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

prepare() {
  need_cmd bash
  need_cmd npm
  need_cmd cargo

  if [[ ! -d "$MAIN_BACKEND_DIR" || ! -d "$MAIN_FRONTEND_DIR" ]]; then
    echo "[ERROR] 原项目目录不完整: $MAIN_BACKEND_DIR / $MAIN_FRONTEND_DIR"
    exit 1
  fi

  if [[ ! -d "$MEMORY_BACKEND_DIR" || ! -d "$MEMORY_FRONTEND_DIR" ]]; then
    echo "[ERROR] memory_server 目录不完整: $MEMORY_BACKEND_DIR / $MEMORY_FRONTEND_DIR"
    exit 1
  fi

  if [[ ! -d "$IM_BACKEND_DIR" ]]; then
    echo "[ERROR] im_service backend 目录不完整: $IM_BACKEND_DIR"
    exit 1
  fi

  if [[ ! -d "$TASK_BACKEND_DIR" || ! -d "$TASK_FRONTEND_DIR" ]]; then
    echo "[ERROR] contact_task_service 目录不完整: $TASK_BACKEND_DIR / $TASK_FRONTEND_DIR"
    exit 1
  fi

  mkdir -p "$RUNTIME_DIR"

  if [[ ! -f "$MEMORY_BACKEND_ENV_FILE" && -f "$MEMORY_BACKEND_DIR/.env.example" ]]; then
    echo "[INFO] memory_server backend/.env 不存在，自动从 .env.example 复制"
    cp "$MEMORY_BACKEND_DIR/.env.example" "$MEMORY_BACKEND_ENV_FILE"
  fi

  if [[ -z "$MEMORY_BACKEND_PORT" ]]; then
    read_memory_port_from_env_file "$MEMORY_BACKEND_ENV_FILE"
  fi
  MEMORY_BACKEND_PORT="${MEMORY_BACKEND_PORT:-7080}"

  if [[ -z "$IM_BACKEND_PORT" ]]; then
    read_im_port_from_env_file "$IM_BACKEND_ENV_FILE"
  fi
  IM_BACKEND_PORT="${IM_BACKEND_PORT:-7090}"

  if [[ -z "$TASK_BACKEND_PORT" ]]; then
    read_task_port_from_env_file "$TASK_BACKEND_ENV_FILE"
  fi
  TASK_BACKEND_PORT="${TASK_BACKEND_PORT:-8096}"
}

start_main_backend() {
  echo "[INFO] 启动原项目 backend..."
  nohup bash -lc "cd \"$MAIN_BACKEND_DIR\" && if [[ -f \"$MAIN_BACKEND_ENV_FILE\" ]]; then set -a; source \"$MAIN_BACKEND_ENV_FILE\"; set +a; fi; export TASK_SERVICE_BASE_URL=\"\${TASK_SERVICE_BASE_URL:-http://127.0.0.1:8096/api/task-service/v1}\"; export TASK_SERVICE_SERVICE_TOKEN=\"\${TASK_SERVICE_SERVICE_TOKEN:-\${CONTACT_TASK_SERVICE_SERVICE_TOKEN:-\${MEMORY_SERVER_SERVICE_TOKEN:-}}}\"; export IM_SERVICE_BASE_URL=\"\${IM_SERVICE_BASE_URL:-http://127.0.0.1:$IM_BACKEND_PORT/api/im/v1}\"; export IM_SERVICE_SERVICE_TOKEN=\"\${IM_SERVICE_SERVICE_TOKEN:-\${MEMORY_SERVER_SERVICE_TOKEN:-}}\"; cargo run --bin chat_app_server_rs" >"$MAIN_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$MAIN_BACKEND_PID_FILE"
}

start_main_frontend() {
  echo "[INFO] 启动原项目 frontend..."
  ensure_vite_dependencies "原项目 frontend" "$MAIN_FRONTEND_DIR"
  nohup bash -lc "cd \"$MAIN_FRONTEND_DIR\" && npm run dev -- --host 0.0.0.0 --port \"$MAIN_FRONTEND_PORT\"" >"$MAIN_FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$MAIN_FRONTEND_PID_FILE"
}

start_memory_backend() {
  echo "[INFO] 启动 memory backend..."
  nohup bash -lc "cd \"$MEMORY_BACKEND_DIR\" && if [[ -f .env ]]; then set -a; source .env; set +a; fi; export IM_SERVICE_AUTH_SECRET=\"\${IM_SERVICE_AUTH_SECRET:-\${MEMORY_SERVER_TRUSTED_IM_AUTH_SECRET:-\${MEMORY_SERVER_AUTH_SECRET:-memory_server_dev_change_me}}}\"; export MEMORY_SERVER_TRUSTED_IM_AUTH_SECRET=\"\${MEMORY_SERVER_TRUSTED_IM_AUTH_SECRET:-\$IM_SERVICE_AUTH_SECRET}\"; cargo run --bin memory_server" >"$MEMORY_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$MEMORY_BACKEND_PID_FILE"
}

start_memory_frontend() {
  echo "[INFO] 启动 memory frontend..."
  ensure_vite_dependencies "memory frontend" "$MEMORY_FRONTEND_DIR"
  nohup bash -lc "cd \"$MEMORY_FRONTEND_DIR\" && npm run dev -- --host \"$MEMORY_FRONTEND_HOST\" --port \"$MEMORY_FRONTEND_PORT\"" >"$MEMORY_FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$MEMORY_FRONTEND_PID_FILE"
}

start_im_backend() {
  echo "[INFO] 启动 IM backend..."
  nohup bash -lc "cd \"$IM_BACKEND_DIR\" && if [[ -f \"$MEMORY_BACKEND_ENV_FILE\" ]]; then set -a; source \"$MEMORY_BACKEND_ENV_FILE\"; set +a; fi; if [[ -f \"$IM_BACKEND_ENV_FILE\" ]]; then set -a; source \"$IM_BACKEND_ENV_FILE\"; set +a; fi; export IM_SERVICE_PORT=\"\${IM_SERVICE_PORT:-$IM_BACKEND_PORT}\"; export IM_SERVICE_MONGODB_URI=\"\${IM_SERVICE_MONGODB_URI:-\${MONGO_URL:-\${MEMORY_SERVER_MONGODB_URI:-\${MEMORY_SERVER_DATABASE_URL:-mongodb://127.0.0.1:27017}}}}\"; export IM_SERVICE_AUTH_SECRET=\"\${IM_SERVICE_AUTH_SECRET:-\${MEMORY_SERVER_TRUSTED_IM_AUTH_SECRET:-\${MEMORY_SERVER_AUTH_SECRET:-memory_server_dev_change_me}}}\"; export IM_SERVICE_SERVICE_TOKEN=\"\${IM_SERVICE_SERVICE_TOKEN:-\${MEMORY_SERVER_SERVICE_TOKEN:-}}\"; cargo run" >"$IM_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$IM_BACKEND_PID_FILE"
}

start_task_backend() {
  echo "[INFO] 启动 task backend..."
  nohup bash -lc "cd \"$TASK_BACKEND_DIR\" && if [[ -f \"$MEMORY_BACKEND_ENV_FILE\" ]]; then set -a; source \"$MEMORY_BACKEND_ENV_FILE\"; set +a; fi; if [[ -f \"$TASK_BACKEND_ENV_FILE\" ]]; then set -a; source \"$TASK_BACKEND_ENV_FILE\"; set +a; fi; export CONTACT_TASK_SERVICE_MONGO_URL=\"\${CONTACT_TASK_SERVICE_MONGO_URL:-\${MONGO_URL:-\${MEMORY_SERVER_MONGODB_URI:-\${MEMORY_SERVER_DATABASE_URL:-}}}}\"; export CONTACT_TASK_SERVICE_AUTH_SECRET=\"\${CONTACT_TASK_SERVICE_AUTH_SECRET:-\${MEMORY_SERVER_AUTH_SECRET:-}}\"; export CONTACT_TASK_SERVICE_SERVICE_TOKEN=\"\${CONTACT_TASK_SERVICE_SERVICE_TOKEN:-\${TASK_SERVICE_SERVICE_TOKEN:-\${MEMORY_SERVER_SERVICE_TOKEN:-}}}\"; cargo run" >"$TASK_BACKEND_LOG_FILE" 2>&1 &
  echo $! >"$TASK_BACKEND_PID_FILE"
}

start_task_frontend() {
  echo "[INFO] 启动 task frontend..."
  ensure_vite_dependencies "task frontend" "$TASK_FRONTEND_DIR"
  nohup bash -lc "cd \"$TASK_FRONTEND_DIR\" && npm run dev -- --host \"$TASK_FRONTEND_HOST\" --port \"$TASK_FRONTEND_PORT\"" >"$TASK_FRONTEND_LOG_FILE" 2>&1 &
  echo $! >"$TASK_FRONTEND_PID_FILE"
}

check_alive() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    echo "[ERROR] $name 启动失败，请检查日志: $log_file"
    tail -n 60 "$log_file" 2>/dev/null || true
    exit 1
  fi
}

check_port_listening() {
  local name="$1"
  local port="$2"
  local log_file="$3"
  local retries="${4:-20}"

  while (( retries > 0 )); do
    if command -v lsof >/dev/null 2>&1; then
      if lsof -iTCP:"$port" -sTCP:LISTEN -n -P >/dev/null 2>&1; then
        return
      fi
    elif command -v nc >/dev/null 2>&1; then
      if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
        return
      fi
    fi
    sleep 1
    retries=$((retries - 1))
  done

  echo "[ERROR] $name 未在端口 $port 上成功监听，请检查日志: $log_file"
  tail -n 60 "$log_file" 2>/dev/null || true
  exit 1
}

do_stop() {
  stop_from_pid_file "原项目 backend" "$MAIN_BACKEND_PID_FILE"
  stop_from_pid_file "原项目 frontend" "$MAIN_FRONTEND_PID_FILE"
  stop_from_pid_file "memory backend" "$MEMORY_BACKEND_PID_FILE"
  stop_from_pid_file "memory frontend" "$MEMORY_FRONTEND_PID_FILE"
  stop_from_pid_file "IM backend" "$IM_BACKEND_PID_FILE"
  stop_from_pid_file "task backend" "$TASK_BACKEND_PID_FILE"
  stop_from_pid_file "task frontend" "$TASK_FRONTEND_PID_FILE"

  stop_from_port "原项目 backend" "$MAIN_BACKEND_PORT"
  stop_from_port "原项目 frontend" "$MAIN_FRONTEND_PORT"
  stop_from_port "memory backend" "$MEMORY_BACKEND_PORT"
  stop_from_port "memory frontend" "$MEMORY_FRONTEND_PORT"
  stop_from_port "IM backend" "$IM_BACKEND_PORT"
  stop_from_port "task backend" "$TASK_BACKEND_PORT"
  stop_from_port "task frontend" "$TASK_FRONTEND_PORT"
}

print_runtime_info() {
  echo "[OK] 全部服务已在后台运行"
  echo "  原项目 backend pid: $(cat "$MAIN_BACKEND_PID_FILE")"
  echo "  原项目 frontend pid: $(cat "$MAIN_FRONTEND_PID_FILE")"
  echo "  memory backend pid: $(cat "$MEMORY_BACKEND_PID_FILE")"
  echo "  memory frontend pid: $(cat "$MEMORY_FRONTEND_PID_FILE")"
  echo "  IM backend pid: $(cat "$IM_BACKEND_PID_FILE")"
  echo "  task backend pid: $(cat "$TASK_BACKEND_PID_FILE")"
  echo "  task frontend pid: $(cat "$TASK_FRONTEND_PID_FILE")"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  memory backend log: $MEMORY_BACKEND_LOG_FILE"
  echo "  memory frontend log: $MEMORY_FRONTEND_LOG_FILE"
  echo "  IM backend log: $IM_BACKEND_LOG_FILE"
  echo "  task backend log: $TASK_BACKEND_LOG_FILE"
  echo "  task frontend log: $TASK_FRONTEND_LOG_FILE"
  echo
  echo "  原项目 frontend url: http://localhost:$MAIN_FRONTEND_PORT"
  echo "  原项目 backend url: http://localhost:$MAIN_BACKEND_PORT"
  echo "  memory frontend url: http://localhost:$MEMORY_FRONTEND_PORT"
  echo "  memory backend url: http://localhost:$MEMORY_BACKEND_PORT"
  echo "  IM backend url: http://localhost:$IM_BACKEND_PORT"
  echo "  task frontend url: http://localhost:$TASK_FRONTEND_PORT"
  echo "  task backend url: http://localhost:$TASK_BACKEND_PORT"
}

status() {
  local main_backend_pid main_frontend_pid memory_backend_pid memory_frontend_pid im_backend_pid task_backend_pid task_frontend_pid
  main_backend_pid="$(cat "$MAIN_BACKEND_PID_FILE" 2>/dev/null || true)"
  main_frontend_pid="$(cat "$MAIN_FRONTEND_PID_FILE" 2>/dev/null || true)"
  memory_backend_pid="$(cat "$MEMORY_BACKEND_PID_FILE" 2>/dev/null || true)"
  memory_frontend_pid="$(cat "$MEMORY_FRONTEND_PID_FILE" 2>/dev/null || true)"
  im_backend_pid="$(cat "$IM_BACKEND_PID_FILE" 2>/dev/null || true)"
  task_backend_pid="$(cat "$TASK_BACKEND_PID_FILE" 2>/dev/null || true)"
  task_frontend_pid="$(cat "$TASK_FRONTEND_PID_FILE" 2>/dev/null || true)"

  echo "[INFO] runtime dir: $RUNTIME_DIR"
  echo "  原项目 backend pid: ${main_backend_pid:-N/A}"
  echo "  原项目 frontend pid: ${main_frontend_pid:-N/A}"
  echo "  memory backend pid: ${memory_backend_pid:-N/A}"
  echo "  memory frontend pid: ${memory_frontend_pid:-N/A}"
  echo "  IM backend pid: ${im_backend_pid:-N/A}"
  echo "  task backend pid: ${task_backend_pid:-N/A}"
  echo "  task frontend pid: ${task_frontend_pid:-N/A}"
  echo
  echo "  原项目 backend log: $MAIN_BACKEND_LOG_FILE"
  echo "  原项目 frontend log: $MAIN_FRONTEND_LOG_FILE"
  echo "  memory backend log: $MEMORY_BACKEND_LOG_FILE"
  echo "  memory frontend log: $MEMORY_FRONTEND_LOG_FILE"
  echo "  IM backend log: $IM_BACKEND_LOG_FILE"
  echo "  task backend log: $TASK_BACKEND_LOG_FILE"
  echo "  task frontend log: $TASK_FRONTEND_LOG_FILE"
}

CMD="${1:-restart}"
prepare

case "$CMD" in
  restart|start)
    do_stop
    start_memory_backend
    start_im_backend
    start_task_backend
    start_main_backend
    sleep 2
    check_alive "原项目 backend" "$MAIN_BACKEND_PID_FILE" "$MAIN_BACKEND_LOG_FILE"
    check_alive "memory backend" "$MEMORY_BACKEND_PID_FILE" "$MEMORY_BACKEND_LOG_FILE"
    check_alive "IM backend" "$IM_BACKEND_PID_FILE" "$IM_BACKEND_LOG_FILE"
    check_alive "task backend" "$TASK_BACKEND_PID_FILE" "$TASK_BACKEND_LOG_FILE"
    check_port_listening "原项目 backend" "$MAIN_BACKEND_PORT" "$MAIN_BACKEND_LOG_FILE" 90
    check_port_listening "memory backend" "$MEMORY_BACKEND_PORT" "$MEMORY_BACKEND_LOG_FILE" 60
    check_port_listening "IM backend" "$IM_BACKEND_PORT" "$IM_BACKEND_LOG_FILE" 60
    check_port_listening "task backend" "$TASK_BACKEND_PORT" "$TASK_BACKEND_LOG_FILE" 60
    start_main_frontend
    start_memory_frontend
    start_task_frontend
    sleep 2
    check_alive "原项目 frontend" "$MAIN_FRONTEND_PID_FILE" "$MAIN_FRONTEND_LOG_FILE"
    check_alive "memory frontend" "$MEMORY_FRONTEND_PID_FILE" "$MEMORY_FRONTEND_LOG_FILE"
    check_alive "task frontend" "$TASK_FRONTEND_PID_FILE" "$TASK_FRONTEND_LOG_FILE"
    check_port_listening "原项目 frontend" "$MAIN_FRONTEND_PORT" "$MAIN_FRONTEND_LOG_FILE" 30
    check_port_listening "memory frontend" "$MEMORY_FRONTEND_PORT" "$MEMORY_FRONTEND_LOG_FILE" 30
    check_port_listening "task frontend" "$TASK_FRONTEND_PORT" "$TASK_FRONTEND_LOG_FILE" 30
    print_runtime_info
    ;;
  stop)
    do_stop
    echo "[OK] 全部服务已停止"
    ;;
  status)
    status
    ;;
  *)
    echo "用法: $0 [restart|start|stop|status]"
    exit 1
    ;;
esac
