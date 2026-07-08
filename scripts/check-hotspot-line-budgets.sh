#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team


set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WARN_ONLY=0
INCLUDE_PLANNED=0

usage() {
  cat <<'EOF'
Usage: scripts/check-hotspot-line-budgets.sh [--warn] [--warn-planned]

Check line-count budgets for known hotspot files.

Modes:
  default        Enforce existing budgets and exit non-zero on violations.
  --warn        Print violations as warnings and always exit zero.
  --warn-planned Include planned refactor hotspots as warning-only budgets.
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --warn)
      WARN_ONLY=1
      ;;
    --warn-planned)
      WARN_ONLY=1
      INCLUDE_PLANNED=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

BUDGETS=(
  "chatos/frontend/src/components/ToolCallRenderer.tsx:340"
  "chatos/frontend/src/components/ChatInterface.tsx:180"
  "chatos/frontend/src/components/ProjectExplorer.tsx:180"
  "chatos/frontend/src/components/projectExplorer/TreePane.tsx:260"
  "chatos/frontend/src/components/projectExplorer/useProjectExplorerWorkspaceView.ts:236"
  "chatos/frontend/src/lib/api/client/types.ts:40"
  "chatos/frontend/src/components/projectExplorer/runState/useProjectRunnerCatalogState.ts:560"
  "chatos/frontend/src/lib/store/actions/remoteConnections.ts:580"
  "chatos/frontend/src/components/chatInterface/useChatStreamRealtimeBridge.ts:412"
  "chatos/frontend/src/components/terminal/useTerminalInstanceLifecycle.ts:420"
  "chatos/backend/src/services/git/mod.rs:40"
  "chatos/backend/src/services/chatos_skills.rs:700"
  "chatos/backend/src/services/chatos_memory_engine/mod.rs:120"
  "chatos/backend/src/services/code_nav/languages/java/mod.rs:650"
  "chatos/backend/src/services/code_nav/languages/go/mod.rs:520"
  "chatos/backend/src/services/code_nav/languages/python/mod.rs:520"
  "chatos/backend/src/core/chat_runtime.rs:300"
  "db_connection_hub/backend/src/drivers/sqlserver/metadata/detail.rs:720"
  "db_connection_hub/frontend/src/App.tsx:440"
)

PLANNED_BUDGETS=(
  "project_management_service/backend/src/store/sqlite.rs:700"
  "project_management_service/backend/src/api/router.rs:700"
  "project_management_service/backend/src/store/mongo.rs:700"
  "project_management_service/backend/src/mcp_server.rs:700"
  "chatos/backend/src/api/projects/requirement_execution_handlers.rs:700"
  "user_service/backend/src/api/models.rs:700"
  "chatos/backend/src/api/configs/ai_model.rs:700"
  "project_management_service/frontend/src/pages/ProjectDetailPage.tsx:500"
  "chatos/frontend/src/components/projectExplorer/ProjectPlanPane.tsx:500"
  "chatos/frontend/src/components/projectExplorer/ProjectRunSettingsPanel.tsx:500"
  "chatos/frontend/src/i18n/messages/enUS.ts:1200"
  "chatos/frontend/src/i18n/messages/zhCN.ts:1200"
  "task_runner_service/frontend/src/i18n/messages/enUS.ts:1200"
  "task_runner_service/frontend/src/i18n/messages/zhCN.ts:1200"
)

failures=0
warnings=0

check_budget() {
  local item="$1"
  local severity="$2"
  relative_path="${item%%:*}"
  max_lines="${item##*:}"
  absolute_path="$ROOT_DIR/$relative_path"

  if [[ ! -f "$absolute_path" ]]; then
    echo "Missing hotspot file: $relative_path"
    if [[ "$severity" == "warning" ]]; then
      warnings=$((warnings + 1))
    else
      failures=1
    fi
    return
  fi

  line_count="$(wc -l < "$absolute_path" | tr -d ' ')"
  if (( line_count > max_lines )); then
    if [[ "$severity" == "warning" ]]; then
      echo "Warning: planned hotspot exceeds target: $relative_path has $line_count lines (target $max_lines)"
      warnings=$((warnings + 1))
    else
      echo "Hotspot line budget exceeded: $relative_path has $line_count lines (max $max_lines)"
      failures=1
    fi
  fi
}

for item in "${BUDGETS[@]}"; do
  if (( WARN_ONLY == 1 )); then
    check_budget "$item" "warning"
  else
    check_budget "$item" "error"
  fi
done

if (( INCLUDE_PLANNED == 1 )); then
  for item in "${PLANNED_BUDGETS[@]}"; do
    check_budget "$item" "warning"
  done
fi

if (( failures > 0 && WARN_ONLY == 0 )); then
  exit 1
fi

if (( warnings > 0 )); then
  echo "Hotspot line budget warnings: $warnings"
else
  echo "Hotspot line budgets are within limits."
fi
