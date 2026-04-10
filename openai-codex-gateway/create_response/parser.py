from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gateway_request.payload import (
    ensure_non_empty_turn_input,
    extract_reasoning_options,
    extract_turn_input_items,
    merge_input_items_with_tool_outputs,
)


@dataclass
class CreateResponseContext:
    input_items: list[dict[str, Any]]
    model: Any
    model_name: str
    previous_response_id: str | None
    reasoning_effort: str | None
    reasoning_summary: str | None
    response_tools: list[dict[str, Any]]


def parse_create_response_context(
    payload: dict[str, Any],
    *,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> CreateResponseContext:
    input_items = extract_turn_input_items(payload)
    input_items = merge_input_items_with_tool_outputs(input_items, provided_tool_outputs)
    input_items = ensure_non_empty_turn_input(input_items)
    if not input_items:
        raise ValueError("request input is empty; provide `input` text/image/file")

    model = payload.get("model")
    model_name = model if isinstance(model, str) and model else "codex-default"

    previous_response_id_raw = payload.get("previous_response_id")
    previous_response_id = (
        previous_response_id_raw
        if isinstance(previous_response_id_raw, str) and previous_response_id_raw
        else None
    )
    reasoning_effort, reasoning_summary = extract_reasoning_options(payload)
    response_tools_raw = payload.get("tools")
    response_tools = response_tools_raw if isinstance(response_tools_raw, list) else []

    return CreateResponseContext(
        input_items=input_items,
        model=model,
        model_name=model_name,
        previous_response_id=previous_response_id,
        reasoning_effort=reasoning_effort,
        reasoning_summary=reasoning_summary,
        response_tools=response_tools,
    )
