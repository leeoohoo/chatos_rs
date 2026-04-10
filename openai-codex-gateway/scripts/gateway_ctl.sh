#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATEWAY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SERVER_FILE="$GATEWAY_DIR/server.py"
REQUIREMENTS_FILE="$GATEWAY_DIR/requirements.txt"
RUNTIME_DEP_PYDANTIC_SPEC="${CODEX_GATEWAY_RUNTIME_DEP_PYDANTIC_SPEC:-pydantic>=2.7,<3}"

RUNTIME_DIR="${CODEX_GATEWAY_RUNTIME_DIR:-/tmp/chatos_rs_dev}"
PID_FILE="${CODEX_GATEWAY_PID_FILE:-$RUNTIME_DIR/codex_gateway.pid}"
LOG_FILE="${CODEX_GATEWAY_LOG_FILE:-$RUNTIME_DIR/codex_gateway.log}"
AUTO_INSTALL_DEPS="${CODEX_GATEWAY_AUTO_INSTALL_DEPS:-1}"

HOST="${CODEX_GATEWAY_HOST:-127.0.0.1}"
PORT="${CODEX_GATEWAY_PORT:-8089}"

PYTHON_BIN="${PYTHON_BIN:-}"
if [[ -z "$PYTHON_BIN" ]]; then
  if command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN="python3"
  elif command -v python >/dev/null 2>&1; then
    PYTHON_BIN="python"
  else
    echo "[ERROR] python3/python not found"
    exit 1
  fi
fi

ACTION="${1:-start}"

mkdir -p "$RUNTIME_DIR"

is_pid_running() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1
}

read_pid() {
  if [[ -f "$PID_FILE" ]]; then
    cat "$PID_FILE" 2>/dev/null || true
  fi
}

python_has_module() {
  local module_name="$1"
  "$PYTHON_BIN" -c "import importlib.util; raise SystemExit(0 if importlib.util.find_spec('$module_name') else 1)" >/dev/null 2>&1
}

ensure_runtime_deps() {
  if python_has_module "pydantic"; then
    return 0
  fi

  if [[ "$AUTO_INSTALL_DEPS" != "1" ]]; then
    echo "[ERROR] missing Python dependency: pydantic"
    echo "[ERROR] install with: $PYTHON_BIN -m pip install -r \"$REQUIREMENTS_FILE\""
    echo "[ERROR] or set CODEX_GATEWAY_AUTO_INSTALL_DEPS=1 to enable automatic install."
    exit 1
  fi

  if [[ ! -f "$REQUIREMENTS_FILE" ]]; then
    echo "[ERROR] missing requirements file: $REQUIREMENTS_FILE"
    exit 1
  fi

  if ! "$PYTHON_BIN" -m pip --version >/dev/null 2>&1; then
    echo "[ERROR] pip is unavailable for $PYTHON_BIN"
    exit 1
  fi

  echo "[WARN] missing Python dependency 'pydantic'; installing gateway dependencies..."
  local -a runtime_dep_specs=("$RUNTIME_DEP_PYDANTIC_SPEC")
  local install_ok=0

  echo "[INFO] pip install attempt #1: standard environment"
  if "$PYTHON_BIN" -m pip install "${runtime_dep_specs[@]}"; then
    install_ok=1
  else
    echo "[WARN] pip install attempt #1 failed."
  fi

  if [[ "$install_ok" -eq 0 ]]; then
    echo "[INFO] pip install attempt #2: user site-packages (--user)"
    if "$PYTHON_BIN" -m pip install --user "${runtime_dep_specs[@]}"; then
      install_ok=1
    else
      echo "[WARN] pip install attempt #2 failed."
    fi
  fi

  if [[ "$install_ok" -eq 0 ]]; then
    echo "[INFO] pip install attempt #3: break system packages (--break-system-packages)"
    if "$PYTHON_BIN" -m pip install --break-system-packages "${runtime_dep_specs[@]}"; then
      install_ok=1
    else
      echo "[WARN] pip install attempt #3 failed."
    fi
  fi

  if [[ "$install_ok" -eq 0 ]]; then
    echo "[ERROR] failed to install runtime dependency: ${runtime_dep_specs[*]}"
    echo "[ERROR] try manual install (choose one):"
    echo "[ERROR]   $PYTHON_BIN -m pip install --user ${runtime_dep_specs[*]}"
    echo "[ERROR]   $PYTHON_BIN -m pip install --break-system-packages ${runtime_dep_specs[*]}"
    if [[ -f "$REQUIREMENTS_FILE" ]]; then
      echo "[ERROR] full dependencies file: $REQUIREMENTS_FILE"
    fi
    exit 1
  fi

  if ! python_has_module "pydantic"; then
    echo "[ERROR] dependency installation completed but 'pydantic' is still unavailable."
    echo "[ERROR] please check Python environment and retry."
    exit 1
  fi
}

print_status() {
  local pid
  pid="$(read_pid)"
  if is_pid_running "$pid"; then
    echo "[INFO] codex gateway is running (pid=$pid)"
  else
    echo "[INFO] codex gateway is not running"
  fi
  echo "[INFO] host=$HOST port=$PORT"
  echo "[INFO] pid_file=$PID_FILE"
  echo "[INFO] log_file=$LOG_FILE"
}

start_gateway() {
  ensure_runtime_deps

  local pid
  pid="$(read_pid)"
  if is_pid_running "$pid"; then
    echo "[INFO] codex gateway already running (pid=$pid)"
    echo "[INFO] log_file=$LOG_FILE"
    exit 0
  fi

  rm -f "$PID_FILE"
  touch "$LOG_FILE"

  local -a cmd=(
    "$PYTHON_BIN" -u "$SERVER_FILE"
    --host "$HOST"
    --port "$PORT"
  )

  if [[ -n "${CODEX_GATEWAY_CODEX_BIN:-}" ]]; then
    cmd+=(--codex-bin "$CODEX_GATEWAY_CODEX_BIN")
  fi
  if [[ -n "${CODEX_GATEWAY_STATE_DB:-}" ]]; then
    cmd+=(--state-db "$CODEX_GATEWAY_STATE_DB")
  fi
  if [[ -n "${CODEX_GATEWAY_CWD:-}" ]]; then
    cmd+=(--cwd "$CODEX_GATEWAY_CWD")
  fi
  if [[ -n "${CODEX_GATEWAY_SANDBOX:-}" ]]; then
    cmd+=(--sandbox "$CODEX_GATEWAY_SANDBOX")
  fi

  echo "[INFO] starting codex gateway..."
  nohup "${cmd[@]}" >>"$LOG_FILE" 2>&1 &
  local new_pid="$!"
  echo "$new_pid" >"$PID_FILE"

  sleep 1
  if is_pid_running "$new_pid"; then
    echo "[INFO] codex gateway started (pid=$new_pid)"
    echo "[INFO] log_file=$LOG_FILE"
  else
    echo "[ERROR] codex gateway failed to start, check log:"
    echo "  $LOG_FILE"
    rm -f "$PID_FILE"
    exit 1
  fi
}

stop_gateway() {
  local pid
  pid="$(read_pid)"
  if ! is_pid_running "$pid"; then
    echo "[INFO] codex gateway is not running"
    rm -f "$PID_FILE"
    exit 0
  fi

  echo "[INFO] stopping codex gateway (pid=$pid)..."
  kill "$pid" >/dev/null 2>&1 || true
  sleep 1
  if is_pid_running "$pid"; then
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi
  rm -f "$PID_FILE"
  echo "[INFO] codex gateway stopped"
}

tail_logs() {
  touch "$LOG_FILE"
  echo "[INFO] tailing $LOG_FILE"
  tail -n 200 -f "$LOG_FILE"
}

case "$ACTION" in
  start)
    start_gateway
    ;;
  stop)
    stop_gateway
    ;;
  restart)
    stop_gateway || true
    start_gateway
    ;;
  status)
    print_status
    ;;
  tail)
    tail_logs
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status|tail}"
    exit 1
    ;;
esac
