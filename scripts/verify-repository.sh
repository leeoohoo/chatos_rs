#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN_VERSION="${CHATOS_RUST_TOOLCHAIN:-1.94.0}"
CARGO=(cargo "+${RUST_TOOLCHAIN_VERSION}")

FRONTEND_DIRS=(
  "admin_console"
  "official_website_service/frontend"
)

usage() {
  cat <<'EOF'
Usage: scripts/verify-repository.sh <mode> [frontend-directory]

Modes:
  quality       Run repository code-quality policies.
  rust-build    Check every Rust binary shipped by the production Compose stack.
  rust-lint     Run Rust formatting and Clippy for all workspaces.
  rust-test     Run tests for the root, Memory Engine, and User Service workspaces.
  native-platform Build and test the native client and Computer Use plugin for this OS.
  plugins         Test the Browser and Document plugin workspaces.
  frontend DIR  Type-check, test when available, build, and lint when available.
  frontends     Verify all production frontends (dependencies must already be installed).
  fast          Run quality and Rust lint checks.
  full          Run quality, Rust lint/tests, and all frontend checks.
EOF
}

has_npm_script() {
  local directory="$1"
  local script_name="$2"
  node -e '
    const packageJson = require(process.argv[1]);
    process.exit(packageJson.scripts?.[process.argv[2]] ? 0 : 1);
  ' "$ROOT_DIR/$directory/package.json" "$script_name"
}

validate_frontend_directory() {
  local requested="$1"
  local directory
  for directory in "${FRONTEND_DIRS[@]}"; do
    if [[ "$directory" == "$requested" ]]; then
      return 0
    fi
  done
  echo "Unsupported production frontend: $requested" >&2
  return 2
}

run_quality() {
  cd "$ROOT_DIR"
  python3 -m unittest discover -s scripts/tests -p "test_code_quality_*.py"
  python3 scripts/check_source_size_policy.py
  python3 scripts/check_new_code_clones.py --min-lines 25
  python3 scripts/check-agent-tool-plane-boundaries.py
  python3 scripts/check-non-test-unwrap-expect.py
  bash scripts/check-request-path-panics.sh
  bash scripts/check-hotspot-line-budgets.sh
  python3 scripts/check-rust-dependency-drift.py
}

run_rust_build() {
  cd "$ROOT_DIR"
  "${CARGO[@]}" check \
    -p config_center_service_backend \
    -p project_management_service_backend \
    -p plugin_management_service_backend \
    -p local_connector_service_backend \
    -p mcp_management_service_backend \
    -p task_runner_service_backend \
    -p chat_app_server_rs \
    -p official_website_service_backend
  "${CARGO[@]}" check \
    --manifest-path memory_engine/backend/Cargo.toml \
    --bin memory_engine
  "${CARGO[@]}" check \
    --manifest-path user_service/backend/Cargo.toml \
    --bin user_service_backend
}

run_rust_lint() {
  cd "$ROOT_DIR"
  "${CARGO[@]}" fmt --all -- --check
  "${CARGO[@]}" clippy --workspace --all-targets -- -D warnings
  (
    cd memory_engine/backend
    "${CARGO[@]}" clippy --all-targets -- -D warnings
  )
  (
    cd user_service/backend
    "${CARGO[@]}" clippy --all-targets -- -D warnings
  )
  "${CARGO[@]}" fmt --manifest-path plugins/browser/Cargo.toml --all -- --check
  "${CARGO[@]}" clippy --manifest-path plugins/browser/Cargo.toml --workspace --all-targets -- -D warnings
}

run_rust_tests() {
  cd "$ROOT_DIR"
  "${CARGO[@]}" test --workspace --no-fail-fast
  (
    cd memory_engine/backend
    "${CARGO[@]}" test --no-fail-fast
  )
  (
    cd user_service/backend
    "${CARGO[@]}" test --no-fail-fast
  )
  "${CARGO[@]}" test --manifest-path plugins/browser/Cargo.toml --workspace --no-fail-fast
}

run_native_platform() {
  case "$(uname -s)" in
    Darwin)
      swift build --package-path "$ROOT_DIR/clients/macos"
      swift test --package-path "$ROOT_DIR/clients/macos"
      swift build --package-path "$ROOT_DIR/plugins/computer-use"
      swift test --package-path "$ROOT_DIR/plugins/computer-use"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      dotnet build "$ROOT_DIR/clients/windows/ChatOS.Win.sln" --configuration Release
      dotnet test "$ROOT_DIR/clients/windows/ChatOS.Win.sln" --configuration Release
      dotnet build "$ROOT_DIR/plugins/computer-use/windows/VisualComputerUse.Windows/VisualComputerUse.Windows.csproj" --configuration Release
      ;;
    *)
      echo "Native platform verification supports only macOS and Windows" >&2
      return 2
      ;;
  esac
}

run_plugins() {
  cd "$ROOT_DIR"
  "${CARGO[@]}" test --manifest-path plugins/browser/Cargo.toml --workspace --no-fail-fast
  npm --prefix plugins/document run vendor:fetch:current
  npm --prefix plugins/document test
}

run_frontend() {
  local directory="$1"
  validate_frontend_directory "$directory"

  npm --prefix "$ROOT_DIR/$directory" run type-check

  if has_npm_script "$directory" "test"; then
    npm --prefix "$ROOT_DIR/$directory" run test
  fi

  if has_npm_script "$directory" "test:electron"; then
    npm --prefix "$ROOT_DIR/$directory" run test:electron
  fi

  npm --prefix "$ROOT_DIR/$directory" run build

  if has_npm_script "$directory" "lint"; then
    npm --prefix "$ROOT_DIR/$directory" run lint
  fi
}

run_frontends() {
  local directory
  for directory in "${FRONTEND_DIRS[@]}"; do
    echo "Verifying frontend: $directory"
    run_frontend "$directory"
  done
}

mode="${1:-}"
case "$mode" in
  quality)
    run_quality
    ;;
  rust-build)
    run_rust_build
    ;;
  rust-lint)
    run_rust_lint
    ;;
  rust-test)
    run_rust_tests
    ;;
  native-platform)
    run_native_platform
    ;;
  plugins)
    run_plugins
    ;;
  frontend)
    [[ "$#" -eq 2 ]] || {
      usage >&2
      exit 2
    }
    run_frontend "$2"
    ;;
  frontends)
    run_frontends
    ;;
  fast)
    run_quality
    run_rust_build
    run_rust_lint
    ;;
  full)
    run_quality
    run_rust_build
    run_rust_lint
    run_rust_tests
    run_frontends
    ;;
  -h|--help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
