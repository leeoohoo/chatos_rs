#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BUDGETS=(
  "chat_app/src/components/ToolCallRenderer.tsx:340"
  "chat_app/src/components/ChatInterface.tsx:180"
  "chat_app/src/components/ProjectExplorer.tsx:180"
  "chat_app/src/components/projectExplorer/TreePane.tsx:260"
  "chat_app/src/components/chatInterface/useSessionWorkbarPanels.ts:320"
  "chat_app/src/components/projectExplorer/useProjectExplorerWorkspaceView.ts:228"
  "chat_app/src/lib/api/client/types.ts:40"
  "chat_app/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts:560"
  "chat_app/src/lib/store/actions/remoteConnections.ts:580"
  "chat_app/src/components/chatInterface/useChatStreamRealtimeBridge.ts:360"
  "chat_app/src/components/terminal/useTerminalInstanceLifecycle.ts:420"
  "chat_app_server_rs/src/builtin/browser_tools/actions.rs:220"
  "chat_app_server_rs/src/builtin/web_tools/provider.rs:260"
  "chat_app_server_rs/src/services/git/mod.rs:40"
  "chat_app_server_rs/src/services/chatos_skills.rs:700"
  "chat_app_server_rs/src/services/chatos_memory_engine/mod.rs:120"
  "chat_app_server_rs/src/services/code_nav/languages/java/mod.rs:650"
  "chat_app_server_rs/src/services/code_nav/languages/go/mod.rs:520"
  "chat_app_server_rs/src/services/code_nav/languages/python/mod.rs:520"
  "chat_app_server_rs/src/services/v2/mcp_tool_execute.rs:160"
  "chat_app_server_rs/src/services/v3/mcp_tool_execute.rs:240"
  "chat_app_server_rs/src/core/chat_runtime.rs:260"
  "openai-codex-gateway/gateway_request/input_items.py:320"
  "openai-codex-gateway/gateway_request/function_tools.py:160"
  "openai-codex-gateway/gateway_request/request_options.py:220"
  "openai-codex-gateway/gateway_runtime/bridge.py:320"
  "openai-codex-gateway/gateway_runtime/thread_session.py:119"
  "openai-codex-gateway/gateway_runtime/turn_loop.py:80"
  "openai-codex-gateway/gateway_runtime/turn_event_processing.py:220"
  "openai-codex-gateway/gateway_runtime/approval_handler.py:160"
  "db_connection_hub/backend/src/drivers/sqlserver/metadata/detail.rs:720"
  "db_connection_hub/frontend/src/App.tsx:440"
)

failures=0

for item in "${BUDGETS[@]}"; do
  relative_path="${item%%:*}"
  max_lines="${item##*:}"
  absolute_path="$ROOT_DIR/$relative_path"

  if [[ ! -f "$absolute_path" ]]; then
    echo "Missing hotspot file: $relative_path"
    failures=1
    continue
  fi

  line_count="$(wc -l < "$absolute_path" | tr -d ' ')"
  if (( line_count > max_lines )); then
    echo "Hotspot line budget exceeded: $relative_path has $line_count lines (max $max_lines)"
    failures=1
  fi
done

if (( failures > 0 )); then
  exit 1
fi

echo "Hotspot line budgets are within limits."
