from __future__ import annotations

from typing import Any

from gateway_request.payload import encode_tool_arguments
from gateway_response.builder import build_stream_message_item
from gateway_base.types import ToolCallRecord


def build_stream_message_start_events(
    message_id: str,
    *,
    output_index: int = 0,
) -> list[dict[str, Any]]:
    return [
        {
            "type": "response.output_item.added",
            "output_index": output_index,
            "item": {
                "id": message_id,
                "type": "message",
                "status": "in_progress",
                "role": "assistant",
                "content": [],
            },
        },
        {
            "type": "response.content_part.added",
            "output_index": output_index,
            "item_id": message_id,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "",
                "annotations": [],
            },
        },
    ]


def build_stream_message_delta_event(
    message_id: str,
    delta: str,
    *,
    output_index: int = 0,
) -> dict[str, Any]:
    return {
        "type": "response.output_text.delta",
        "output_index": output_index,
        "item_id": message_id,
        "content_index": 0,
        "delta": delta,
        "logprobs": [],
    }


def build_stream_reasoning_delta_event(delta: str) -> dict[str, Any]:
    return {
        "type": "response.reasoning.delta",
        "delta": delta,
    }


def build_stream_reasoning_done_event(text: str) -> dict[str, Any]:
    return {
        "type": "response.reasoning.done",
        "text": text,
    }


def build_stream_message_finalize_events(
    message_id: str,
    text: str,
    *,
    output_index: int = 0,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    done_message = build_stream_message_item(message_id, text, status="completed")
    events: list[dict[str, Any]] = [
        {
            "type": "response.output_text.done",
            "output_index": output_index,
            "item_id": message_id,
            "content_index": 0,
            "text": text,
            "logprobs": [],
        },
        {
            "type": "response.content_part.done",
            "output_index": output_index,
            "item_id": message_id,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": text,
                "annotations": [],
            },
        },
        {
            "type": "response.output_item.done",
            "output_index": output_index,
            "item": done_message,
        },
    ]
    return done_message, events


def build_stream_function_call_events(
    call: ToolCallRecord,
    *,
    output_index: int,
    function_item_id: str,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    encoded_arguments = encode_tool_arguments(call.arguments)
    in_progress_item = {
        "id": function_item_id,
        "type": "function_call",
        "status": "in_progress",
        "call_id": call.call_id,
        "name": call.name,
        "arguments": "",
    }
    done_item = {
        "id": function_item_id,
        "type": "function_call",
        "status": "completed",
        "call_id": call.call_id,
        "name": call.name,
        "arguments": encoded_arguments,
    }

    events: list[dict[str, Any]] = [
        {
            "type": "response.output_item.added",
            "output_index": output_index,
            "item": in_progress_item,
        }
    ]
    if encoded_arguments:
        events.append(
            {
                "type": "response.function_call_arguments.delta",
                "output_index": output_index,
                "item_id": function_item_id,
                "delta": encoded_arguments,
            }
        )
    events.extend(
        [
            {
                "type": "response.function_call_arguments.done",
                "output_index": output_index,
                "item_id": function_item_id,
                "name": call.name,
                "arguments": encoded_arguments,
            },
            {
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": done_item,
            },
        ]
    )
    return done_item, events


def response_completion_event_type(status: str) -> str:
    return "response.completed" if status != "failed" else "response.failed"
