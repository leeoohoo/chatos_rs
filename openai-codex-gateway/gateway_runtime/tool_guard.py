from __future__ import annotations

from typing import Any

from gateway_runtime.sdk_types import (
    CommandExecutionThreadItem,
    DynamicToolCallThreadItem,
    FileChangeThreadItem,
    ImageViewThreadItem,
    McpToolCallThreadItem,
    WebSearchThreadItem,
)


def describe_disallowed_thread_item(
    item: Any,
    *,
    allowed_function_tool_names: set[str],
    allowed_mcp_server_labels: set[str],
) -> str | None:
    if isinstance(item, CommandExecutionThreadItem):
        return "Codex 内置 shell/commandExecution 工具已被 gateway 禁用"

    if isinstance(item, FileChangeThreadItem):
        return "Codex 内置 fileChange/apply_patch 工具已被 gateway 禁用"

    if isinstance(item, ImageViewThreadItem):
        return "Codex 内置 view_image 工具已被 gateway 禁用"

    if isinstance(item, WebSearchThreadItem):
        return "Codex 内置 web_search 工具已被 gateway 禁用"

    if isinstance(item, DynamicToolCallThreadItem):
        tool_name = item.tool.strip()
        if tool_name not in allowed_function_tool_names:
            return (
                "Codex 尝试调用未在本次请求中声明的动态工具："
                f"{tool_name or 'unknown'}"
            )
        return None

    if isinstance(item, McpToolCallThreadItem):
        server_label = item.server.strip()
        if server_label not in allowed_mcp_server_labels:
            return (
                "Codex 尝试调用未在本次请求中声明的 MCP 服务："
                f"{server_label or 'unknown'}"
            )
        return None

    return None
