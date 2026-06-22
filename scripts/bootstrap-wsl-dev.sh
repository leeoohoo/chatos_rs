#!/usr/bin/env bash
set -euo pipefail

if ! command -v apt-get >/dev/null 2>&1; then
  echo "[ERROR] bootstrap-wsl-dev.sh currently supports Debian/Ubuntu apt-based distros only."
  exit 1
fi

echo "[INFO] installing base system dependencies..."
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  sqlite3 \
  libsqlite3-dev \
  curl \
  ca-certificates \
  git \
  make \
  unzip \
  zip \
  python3 \
  python3-pip \
  file \
  lsof \
  net-tools \
  nodejs \
  npm

if ! command -v rustup >/dev/null 2>&1; then
  echo "[INFO] installing rustup + stable toolchain..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup default stable

if command -v cargo >/dev/null 2>&1; then
  cargo --version
fi

if command -v node >/dev/null 2>&1; then
  echo "[INFO] detected Node.js $(node --version)"
  node_major="$(node -p "process.versions.node.split('.')[0]" 2>/dev/null || echo 0)"
  if [[ "${node_major:-0}" -lt 18 ]]; then
    echo "[WARN] Node.js >= 18 is recommended for Vite/React development."
    echo "[WARN] Current apt package is older; consider replacing it with nvm + Node LTS."
  fi
fi

if command -v npm >/dev/null 2>&1; then
  npm --version
fi

cat <<'EOF'

[OK] WSL bootstrap finished.

Suggested next steps from Windows:
  make restart-wsl
  make restart-user-service-wsl
  make restart-memory-engine-wsl
  make restart-task-runner-wsl
  make restart-all-wsl

Or directly inside WSL:
  ./restart_services.sh restart
  ./user_service/restart_services.sh restart
  ./memory_engine/restart_services.sh restart
  ./restart_task_runner_service.sh restart
  ./restart_all_services.sh restart
EOF
