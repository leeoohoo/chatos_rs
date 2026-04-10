from __future__ import annotations

from typing import Any, Callable

from gateway_stream.orchestrator import emit_function_tools_result
from gateway_base.types import ToolCallRecord, TurnResult


def complete_function_tools_stream(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    result: TurnResult,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    previous_response_id: str | None,
    tool_message_id: str,
    tool_chunks: list[str],
    reasoning_chunks: list[str],
    tool_message_started: bool,
    function_item_id_factory: Callable[[], str],
    send_done_marker: Callable[[], None],
    emit_result: Callable[..., None] = emit_function_tools_result,
) -> None:
    unresolved_calls: list[ToolCallRecord] = [
        call for call in result.tool_calls if call.call_id not in provided_tool_outputs
    ]
    emit_result(
        send_event=send_event,
        response_obj=response_obj,
        result=result,
        unresolved_calls=unresolved_calls,
        previous_response_id=previous_response_id,
        tool_message_id=tool_message_id,
        tool_chunks=tool_chunks,
        reasoning_chunks=reasoning_chunks,
        tool_message_started=tool_message_started,
        function_item_id_factory=function_item_id_factory,
    )
    send_done_marker()
