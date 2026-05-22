from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gateway_request.payload import extract_reasoning_options, extract_request_instructions


@dataclass
class StreamRequestContext:
    instructions: str | None
    model_raw: Any
    model_name: str
    response_tools: list[dict[str, Any]]
    reasoning_effort: str | None
    reasoning_summary: str | None


def parse_stream_request_context(payload: dict[str, Any]) -> StreamRequestContext:
    instructions = extract_request_instructions(payload)
    model_raw = payload.get("model")
    model_name = model_raw if isinstance(model_raw, str) and model_raw else "codex-default"
    response_tools_raw = payload.get("tools")
    response_tools = response_tools_raw if isinstance(response_tools_raw, list) else []
    reasoning_effort, reasoning_summary = extract_reasoning_options(payload)

    return StreamRequestContext(
        instructions=instructions,
        model_raw=model_raw,
        model_name=model_name,
        response_tools=response_tools,
        reasoning_effort=reasoning_effort,
        reasoning_summary=reasoning_summary,
    )
