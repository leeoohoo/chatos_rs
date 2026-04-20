from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gateway_request.payload import (
    extract_bearer_token,
    extract_function_call_outputs,
    extract_function_tools,
    extract_reasoning_options,
    extract_request_config_overrides,
    extract_request_cwd,
)


@dataclass
class ResponsesRequestContext:
    request_cwd: str | None
    request_config_overrides: dict[str, Any] | None
    function_tools: list[dict[str, Any]]
    provided_tool_outputs: dict[str, list[dict[str, Any]]]
    stream: bool
    api_key: str | None
    requested_tools_count: int
    reasoning_effort: str | None
    reasoning_summary: str | None
    reasoning_raw: Any


def parse_responses_request(
    payload: dict[str, Any],
    *,
    authorization_header: str | None,
) -> ResponsesRequestContext:
    request_cwd = extract_request_cwd(payload)
    request_config_overrides = extract_request_config_overrides(payload)
    function_tools = extract_function_tools(payload)
    provided_tool_outputs = extract_function_call_outputs(payload)
    stream = bool(payload.get("stream", False))
    api_key = extract_bearer_token(authorization_header)
    raw_tools = payload.get("tools")
    requested_tools_count = len(raw_tools) if isinstance(raw_tools, list) else 0
    reasoning_effort, reasoning_summary = extract_reasoning_options(payload)
    reasoning_raw = payload.get("reasoning")

    return ResponsesRequestContext(
        request_cwd=request_cwd,
        request_config_overrides=request_config_overrides,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        stream=stream,
        api_key=api_key,
        requested_tools_count=requested_tools_count,
        reasoning_effort=reasoning_effort,
        reasoning_summary=reasoning_summary,
        reasoning_raw=reasoning_raw,
    )
