from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.callback_setup import StreamCallbackSetup, setup_stream_callbacks
from gateway_stream.input_prep import prepare_stream_input_items
from gateway_stream.lifecycle import emit_response_created_event


@dataclass
class StreamPreBranchSetup:
    input_items: list[dict[str, Any]]
    callback_setup: StreamCallbackSetup


def setup_stream_pre_branch(
    *,
    payload: dict[str, Any],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    previous_response_id: str | None,
    has_function_tools: bool,
    message_id_factory: Callable[[], str],
) -> StreamPreBranchSetup:
    emit_response_created_event(
        send_event=send_event,
        response_obj=response_obj,
        previous_response_id=previous_response_id,
    )
    input_items = prepare_stream_input_items(
        payload,
        provided_tool_outputs=provided_tool_outputs,
    )
    callback_setup = setup_stream_callbacks(
        send_event=send_event,
        has_function_tools=has_function_tools,
        message_id_factory=message_id_factory,
    )
    return StreamPreBranchSetup(
        input_items=input_items,
        callback_setup=callback_setup,
    )
