from __future__ import annotations

from typing import Any, Callable

from gateway_request.payload import encode_tool_arguments
from gateway_base.types import TurnResult


def build_non_stream_response_body(
    *,
    response_id: str,
    model_name: str,
    result: TurnResult,
    previous_response_id: str | None,
    response_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    message_id: str,
    created_at: int,
    function_item_id_factory: Callable[[], str],
) -> dict[str, Any]:
    unresolved_calls = [
        call for call in result.tool_calls if call.call_id not in provided_tool_outputs
    ]
    if unresolved_calls:
        function_outputs = [
            {
                "id": function_item_id_factory(),
                "type": "function_call",
                "call_id": call.call_id,
                "name": call.name,
                "arguments": encode_tool_arguments(call.arguments),
            }
            for call in unresolved_calls
        ]
        body: dict[str, Any] = {
            "id": response_id,
            "object": "response",
            "created_at": created_at,
            "status": "completed",
            "model": model_name,
            "output": function_outputs,
            "output_text": "",
            "usage": result.usage,
            "error": result.error,
            "previous_response_id": previous_response_id,
            "tools": response_tools,
            "metadata": {
                "thread_id": result.thread_id,
                "turn_id": result.turn_id,
                "pending_tool_calls": [
                    {
                        "call_id": call.call_id,
                        "name": call.name,
                    }
                    for call in unresolved_calls
                ],
            },
        }
        if result.reasoning_text:
            body["reasoning"] = result.reasoning_text
        return body

    body = {
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": result.status,
        "model": model_name,
        "output": [
            {
                "id": message_id,
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": result.output_text,
                    }
                ],
            }
        ],
        "output_text": result.output_text,
        "usage": result.usage,
        "error": result.error,
        "previous_response_id": previous_response_id,
        "tools": response_tools,
        "metadata": {
            "thread_id": result.thread_id,
            "turn_id": result.turn_id,
        },
    }
    if result.reasoning_text:
        body["reasoning"] = result.reasoning_text
    return body


def build_stream_message_item(message_id: str, text: str, *, status: str) -> dict[str, Any]:
    return {
        "id": message_id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [
            {
                "type": "output_text",
                "text": text,
                "annotations": [],
            }
        ],
    }


def build_stream_response_object(
    *,
    response_id: str,
    created_at: int,
    model_name: str,
    response_tools: list[dict[str, Any]],
    status: str,
    output: list[dict[str, Any]],
    usage: dict[str, Any] | None = None,
    error: dict[str, Any] | None = None,
    reasoning: str | None = None,
    previous_response_id: str | None = None,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    body: dict[str, Any] = {
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": status,
        "model": model_name,
        "output": output,
        "parallel_tool_calls": False,
        "tool_choice": "auto",
        "tools": response_tools,
    }
    if usage is not None:
        body["usage"] = usage
    if error is not None:
        body["error"] = error
    if reasoning is not None:
        body["reasoning"] = reasoning
    if previous_response_id is not None:
        body["previous_response_id"] = previous_response_id
    if metadata is not None:
        body["metadata"] = metadata
    return body
