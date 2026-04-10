from __future__ import annotations

from typing import Any, Callable

from gateway_stream.orchestrator import emit_plain_message_result
from gateway_base.types import TurnResult


def complete_plain_message_stream(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    result: TurnResult,
    previous_response_id: str | None,
    message_id: str,
    chunks: list[str],
    reasoning_chunks: list[str],
    send_done_marker: Callable[[], None],
    emit_result: Callable[..., None] = emit_plain_message_result,
) -> None:
    emit_result(
        send_event=send_event,
        response_obj=response_obj,
        result=result,
        previous_response_id=previous_response_id,
        message_id=message_id,
        chunks=chunks,
        reasoning_chunks=reasoning_chunks,
    )
    send_done_marker()
