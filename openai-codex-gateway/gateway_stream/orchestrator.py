from __future__ import annotations

from typing import Any, Callable

from gateway_stream.flow import (
    build_stream_function_call_events,
    build_stream_message_delta_event,
    build_stream_message_finalize_events,
    build_stream_message_start_events,
    build_stream_reasoning_done_event,
    response_completion_event_type,
)
from gateway_base.types import ToolCallRecord, TurnResult


def emit_function_tools_result(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    result: TurnResult,
    unresolved_calls: list[ToolCallRecord],
    previous_response_id: str | None,
    tool_message_id: str,
    tool_chunks: list[str],
    reasoning_chunks: list[str],
    tool_message_started: bool,
    function_item_id_factory: Callable[[], str],
) -> None:
    tool_full_text = result.output_text or "".join(tool_chunks)
    reasoning_full_text = result.reasoning_text or "".join(reasoning_chunks)
    if reasoning_full_text:
        send_event(build_stream_reasoning_done_event(reasoning_full_text))

    if unresolved_calls:
        output_items: list[dict[str, Any]] = []
        function_items: list[dict[str, Any]] = []
        pending_calls: list[dict[str, str]] = []
        output_index_offset = 0

        if tool_message_started:
            done_message, done_events = build_stream_message_finalize_events(
                tool_message_id,
                tool_full_text,
                output_index=0,
            )
            for event in done_events:
                send_event(event)
            output_items.append(done_message)
            output_index_offset = 1

        for call_index, call in enumerate(unresolved_calls):
            output_index = output_index_offset + call_index
            done_item, call_events = build_stream_function_call_events(
                call,
                output_index=output_index,
                function_item_id=function_item_id_factory(),
            )
            for event in call_events:
                send_event(event)
            function_items.append(done_item)
            pending_calls.append(
                {
                    "call_id": call.call_id,
                    "name": call.name,
                }
            )

        completed = response_obj(
            status="completed",
            output=[*output_items, *function_items],
            usage=result.usage,
            error=result.error,
            reasoning=reasoning_full_text if reasoning_full_text else None,
            previous_response_id=previous_response_id,
            metadata={
                "thread_id": result.thread_id,
                "turn_id": result.turn_id,
                "pending_tool_calls": pending_calls,
            },
        )
        send_event({"type": "response.completed", "response": completed})
        return

    if not tool_message_started:
        for event in build_stream_message_start_events(tool_message_id):
            send_event(event)
    if not tool_chunks and tool_full_text:
        send_event(build_stream_message_delta_event(tool_message_id, tool_full_text))
    done_message, done_events = build_stream_message_finalize_events(
        tool_message_id,
        tool_full_text,
        output_index=0,
    )
    for event in done_events:
        send_event(event)

    completed = response_obj(
        status=result.status,
        output=[done_message],
        usage=result.usage,
        error=result.error,
        reasoning=reasoning_full_text if reasoning_full_text else None,
        previous_response_id=previous_response_id,
        metadata={
            "thread_id": result.thread_id,
            "turn_id": result.turn_id,
        },
    )
    event_type = response_completion_event_type(result.status)
    send_event({"type": event_type, "response": completed})


def emit_plain_message_result(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    result: TurnResult,
    previous_response_id: str | None,
    message_id: str,
    chunks: list[str],
    reasoning_chunks: list[str],
) -> None:
    full_text = result.output_text or "".join(chunks)
    reasoning_full_text = result.reasoning_text or "".join(reasoning_chunks)
    if reasoning_full_text:
        send_event(build_stream_reasoning_done_event(reasoning_full_text))

    done_message, done_events = build_stream_message_finalize_events(
        message_id,
        full_text,
        output_index=0,
    )
    for event in done_events:
        send_event(event)

    completed = response_obj(
        status=result.status,
        output=[done_message],
        usage=result.usage,
        error=result.error,
        reasoning=reasoning_full_text if reasoning_full_text else None,
        previous_response_id=previous_response_id,
        metadata={
            "thread_id": result.thread_id,
            "turn_id": result.turn_id,
        },
    )
    event_type = response_completion_event_type(result.status)
    send_event({"type": event_type, "response": completed})
