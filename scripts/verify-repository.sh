#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN_VERSION="${CHATOS_RUST_TOOLCHAIN:-1.94.0}"
CARGO=(cargo "+${RUST_TOOLCHAIN_VERSION}")

FRONTEND_DIRS=(
  "chatos/frontend"
  "config_center_service/frontend"
  "local_connector_client/frontend"
  "memory_engine/frontend"
  "official_website_service/frontend"
  "plugin_management_service/frontend"
  "project_management_service/frontend"
  "task_runner_service/frontend"
  "user_service/frontend"
)

usage() {
  cat <<'EOF'
Usage: scripts/verify-repository.sh <mode> [frontend-directory]

Modes:
  quality       Run repository code-quality policies.
  rust-lint     Run Rust formatting and Clippy for all workspaces.
  rust-test     Run tests for the root, Memory Engine, and User Service workspaces.
  native-platform
                  Check, lint, build, and test native desktop platform contracts.
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
}

run_native_platform() {
  local platform
  local executable_suffix
  case "$(uname -s)" in
    Darwin)
      platform="macos"
      executable_suffix=""
      ;;
    MINGW*|MSYS*|CYGWIN*)
      platform="windows"
      executable_suffix=".exe"
      ;;
    *)
      echo "Native platform verification supports only macOS and Windows" >&2
      return 2
      ;;
  esac

  cd "$ROOT_DIR"
  "${CARGO[@]}" check \
    -p local_connector_client_core \
    --all-targets
  "${CARGO[@]}" clippy \
    -p local_connector_client_core \
    --all-targets \
    -- \
    -D warnings
  "${CARGO[@]}" build \
    -p local_connector_client_core \
    --bins
  "${CARGO[@]}" test \
    -p local_connector_client_core \
    --lib \
    skills::native::computer_use:: \
    -- \
    --nocapture
  "${CARGO[@]}" test \
    -p local_connector_client_core \
    --lib \
    chrome_integration::tests:: \
    -- \
    --nocapture
  if [[ "$platform" == "macos" ]]; then
    "${CARGO[@]}" test \
      -p local_connector_client_core \
      --lib \
      embedded_excel_jxa_bridges_compile_without_launching_excel \
      -- \
      --nocapture
  fi
  local core_binary="$ROOT_DIR/target-shared/debug/local_connector_client_core${executable_suffix}"
  if [[ ! -f "$core_binary" ]]; then
    echo "Native Local Connector Core binary is missing: $core_binary" >&2
    return 1
  fi
  CHATOS_TEST_LOCAL_CONNECTOR_BINARY="$core_binary" node --test \
    "$ROOT_DIR"/local_connector_client/tests/*.test.mjs \
    "$ROOT_DIR"/local_connector_client/verify-installed-package.test.mjs
  npm --prefix "$ROOT_DIR/local_connector_client/frontend" run test:electron
}

run_frontend() {
  local directory="$1"
  validate_frontend_directory "$directory"

  npm --prefix "$ROOT_DIR/$directory" run type-check

  if has_npm_script "$directory" "test"; then
    if [[ "$directory" == "chatos/frontend" ]]; then
      npm --prefix "$ROOT_DIR/$directory" run test -- --run
    else
      npm --prefix "$ROOT_DIR/$directory" run test
    fi
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
  rust-lint)
    run_rust_lint
    ;;
  rust-test)
    run_rust_tests
    ;;
  native-platform)
    run_native_platform
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
    run_rust_lint
    ;;
  full)
    run_quality
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
