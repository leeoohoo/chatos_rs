from __future__ import annotations

from typing import Any

from gateway_base.logging import debug_log, state_log
from gateway_base.types import ToolCallRecord
from gateway_base.utils import make_id
from gateway_base.policy import is_allowed_tool_call_name
from gateway_runtime.turn_state import TurnRuntimeState


def handle_server_request(
    *,
    method: str,
    params: dict[str, Any] | None,
    state: TurnRuntimeState,
    allowed_function_tool_names: set[str],
    allowed_mcp_server_labels: set[str],
    tool_calls: list[ToolCallRecord],
    seen_call_ids: set[str],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> dict[str, Any]:
    payload = params or {}
    if method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
        state_log("run_turn.builtin_request_declined", f"method={method}")
        return {"decision": "decline"}

    if method == "item/permissions/requestApproval":
        if state.disallowed_tool_error is None:
            state.disallowed_tool_error = "Codex 内置 request_permissions 工具已被 gateway 禁用"
        state_log("run_turn.builtin_request_declined", f"method={method}")
        return {"permissions": {}}

    if method == "mcpServer/elicitation/request":
        server_name_raw = payload.get("serverName")
        server_name = server_name_raw.strip() if isinstance(server_name_raw, str) else ""
        if server_name in allowed_mcp_server_labels:
            state_log(
                "run_turn.mcp_elicitation_accepted",
                f"server={server_name}",
            )
            return {
                "action": "accept",
                "content": {},
            }
        if state.disallowed_tool_error is None:
            state.disallowed_tool_error = (
                "Codex 尝试为未声明的 MCP 服务申请调用权限："
                f"{server_name or 'unknown'}"
            )
        state_log(
            "run_turn.mcp_elicitation_declined",
            f"server={server_name or 'unknown'}",
        )
        return {
            "action": "decline",
            "content": None,
        }

    if method != "item/tool/call":
        return {}

    call_id_raw = payload.get("callId")
    tool_name_raw = payload.get("tool")
    arguments = payload.get("arguments")

    call_id = call_id_raw if isinstance(call_id_raw, str) and call_id_raw else make_id("call")
    tool_name = tool_name_raw if isinstance(tool_name_raw, str) and tool_name_raw else "unknown_tool"

    if not is_allowed_tool_call_name(
        tool_name,
        allowed_function_tool_names=allowed_function_tool_names,
        allowed_mcp_server_labels=allowed_mcp_server_labels,
    ):
        if state.disallowed_tool_error is None:
            state.disallowed_tool_error = (
                "Codex 尝试调用未在本次请求中声明的动态工具："
                f"{tool_name}"
            )
        state_log(
            "run_turn.disallowed_dynamic_tool",
            f"name={tool_name}",
            f"call_id={call_id}",
        )
        return {
            "contentItems": [
                {
                    "type": "inputText",
                    "text": "DISALLOWED_TOOL_CALL",
                }
            ],
            "success": False,
        }

    if call_id not in seen_call_ids:
        seen_call_ids.add(call_id)
        tool_calls.append(
            ToolCallRecord(
                call_id=call_id,
                name=tool_name,
                arguments=arguments,
            )
        )

    content_items = provided_tool_outputs.get(call_id)
    debug_log(
        "run_turn.tool_call",
        f"name={tool_name}",
        f"call_id={call_id}",
        f"has_output={'yes' if content_items is not None else 'no'}",
    )
    if content_items is not None:
        return {
            "contentItems": content_items,
            "success": True,
        }

    state.missing_tool_output_detected = True
    return {
        "contentItems": [
            {
                "type": "inputText",
                "text": f"TOOL_OUTPUT_DEFERRED call_id={call_id}",
            }
        ],
        "success": True,
    }
