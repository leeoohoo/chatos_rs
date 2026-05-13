#!/usr/bin/env python3
from __future__ import annotations

from gateway_base.policy import (
    deny_approval,
    extract_allowed_function_tool_names,
    extract_allowed_mcp_server_labels,
    gateway_developer_instructions,
    is_allowed_tool_call_name,
)
from gateway_request.payload import (
    extract_bearer_token,
    extract_request_config_overrides,
)
from gateway_http.handler import GatewayHandler, GatewayServer
from gateway_runtime.bridge import CodexBridge
from gateway_runtime.entrypoint import main
from gateway_runtime.sdk_types import (
    SDK_IMPORT_SOURCE,
    AgentMessageDeltaNotification,
    AgentMessageThreadItem,
    AppServerClient,
    AppServerConfig,
    CommandExecutionThreadItem,
    DynamicToolCallThreadItem,
    FileChangeThreadItem,
    ImageViewThreadItem,
    ItemCompletedNotification,
    ItemStartedNotification,
    McpToolCallThreadItem,
    ModelListResponse,
    ReasoningSummaryTextDeltaNotification,
    ReasoningTextDeltaNotification,
    ReasoningThreadItem,
    ThreadTokenUsageUpdatedNotification,
    TurnCompletedNotification,
    WebSearchThreadItem,
)
from gateway_runtime.tool_guard import describe_disallowed_thread_item
from gateway_runtime.turn_state import TurnRuntimeState

_deny_approval = deny_approval


if __name__ == "__main__":
    main()
