from __future__ import annotations

from typing import Any

from gateway_request.payload import (
    ensure_non_empty_turn_input,
    extract_turn_input_items,
    merge_input_items_with_tool_outputs,
)


def prepare_stream_input_items(
    payload: dict[str, Any],
    *,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    input_items = extract_turn_input_items(payload)
    input_items = merge_input_items_with_tool_outputs(input_items, provided_tool_outputs)
    input_items = ensure_non_empty_turn_input(input_items)
    if not input_items:
        raise ValueError("request input is empty; provide `input` text/image/file")
    return input_items
