from __future__ import annotations

from typing import Any

from gateway_request.payload import (
    ensure_non_empty_turn_input,
    extract_request_instructions,
    extract_turn_input_items,
    merge_input_items_with_tool_outputs,
)


def prepare_stream_input_items(
    payload: dict[str, Any],
    *,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    input_items = extract_turn_input_items(payload)
    if not input_items:
        instructions = extract_request_instructions(payload)
        if instructions:
            input_items = [{"type": "text", "text": instructions}]
    input_items = merge_input_items_with_tool_outputs(input_items, provided_tool_outputs)
    input_items = ensure_non_empty_turn_input(input_items)
    if not input_items:
        raise ValueError("request input is empty; provide `input` text/image/file")
    return input_items
