from __future__ import annotations

from typing import Any


def deny_approval(method: str, _params: dict[str, Any] | None) -> dict[str, Any]:
    # Do not auto-approve command/file-change requests for public HTTP callers.
    if method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
        return {"decision": "decline"}
    return {}


def gateway_developer_instructions() -> str:
    return (
        "Gateway policy: only use caller-provided tools for this request "
        "(dynamic function tools and configured MCP servers). "
        "Do not use Codex built-in environment tools such as shell/command execution, "
        "file editing/apply_patch, request_permissions, or web_search. "
        "If the required caller-provided tool is unavailable, explain that limitation "
        "and ask the user to provide/enable the tool."
    )


def extract_allowed_function_tool_names(function_tools: list[dict[str, Any]]) -> set[str]:
    out: set[str] = set()
    for tool in function_tools:
        name = tool.get("name")
        if isinstance(name, str) and name:
            out.add(name)
    return out


def extract_allowed_mcp_server_labels(config_overrides: dict[str, Any] | None) -> set[str]:
    if not isinstance(config_overrides, dict):
        return set()
    raw_servers = config_overrides.get("mcp_servers")
    if not isinstance(raw_servers, dict):
        return set()
    return {
        label
        for label in raw_servers.keys()
        if isinstance(label, str) and label.strip()
    }


def is_allowed_tool_call_name(
    tool_name: str,
    *,
    allowed_function_tool_names: set[str],
    allowed_mcp_server_labels: set[str],
) -> bool:
    normalized = tool_name.strip()
    if not normalized:
        return False
    if normalized in allowed_function_tool_names:
        return True
    return any(
        normalized.startswith(f"mcp__{label}__")
        for label in allowed_mcp_server_labels
    )
