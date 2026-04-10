#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PYTHON_BIN="${PYTHON_BIN:-python}"
TARGET="${1:-all}"

usage() {
  cat <<'EOF'
Usage:
  bash openai-codex-gateway/tests/run_by_case.sh <target>

Targets:
  all               Run gateway unit regression suite (test_gateway_*.py)
  stream            Run tests/case_stream
  http              Run tests/case_http
  request           Run tests/case_request
  create-response   Run tests/case_create_response
  response          Run tests/case_response
  base              Run tests/case_base
  core              Run tests/case_core
  integration       Run tests/case_integration (requires running gateway / env setup)

  mcp-request       Run integration script: test_mcp_tools_request.py
  mcp-stream        Run integration script: test_mcp_tools_stream.py
  mcp-full          Run integration script: test_mcp_tools_full_session.py
  function-single   Run integration script: test_function_tools_single.py
  function-multi    Run integration script: test_function_tools_multi_call.py
  function-stream   Run integration script: test_function_tools_stream.py
EOF
}

run_discover() {
  local start_dir="$1"
  local pattern="$2"
  "$PYTHON_BIN" -m unittest discover -s "$start_dir" -p "$pattern"
}

run_script() {
  local script_path="$1"
  "$PYTHON_BIN" "$script_path"
}

case "$TARGET" in
  all)
    run_discover "$TEST_ROOT" "test_gateway_*.py"
    ;;
  stream)
    run_discover "$TEST_ROOT/case_stream" "test_gateway_*.py"
    ;;
  http)
    run_discover "$TEST_ROOT/case_http" "test_gateway_*.py"
    ;;
  request)
    run_discover "$TEST_ROOT/case_request" "test_gateway_*.py"
    ;;
  create-response)
    run_discover "$TEST_ROOT/case_create_response" "test_gateway_*.py"
    ;;
  response)
    run_discover "$TEST_ROOT/case_response" "test_gateway_*.py"
    ;;
  base)
    run_discover "$TEST_ROOT/case_base" "test_gateway_*.py"
    ;;
  core)
    run_discover "$TEST_ROOT/case_core" "test_gateway_*.py"
    ;;
  integration)
    run_discover "$TEST_ROOT/case_integration" "test_*.py"
    ;;
  mcp-request)
    run_script "$TEST_ROOT/case_integration/test_mcp_tools_request.py"
    ;;
  mcp-stream)
    run_script "$TEST_ROOT/case_integration/test_mcp_tools_stream.py"
    ;;
  mcp-full)
    run_script "$TEST_ROOT/case_integration/test_mcp_tools_full_session.py"
    ;;
  function-single)
    run_script "$TEST_ROOT/case_integration/test_function_tools_single.py"
    ;;
  function-multi)
    run_script "$TEST_ROOT/case_integration/test_function_tools_multi_call.py"
    ;;
  function-stream)
    run_script "$TEST_ROOT/case_integration/test_function_tools_stream.py"
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    echo "Unknown target: $TARGET" >&2
    usage
    exit 1
    ;;
esac
