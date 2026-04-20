from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.message_callbacks import PlainMessageStreamCallbacks
from gateway_stream.tool_callbacks import FunctionToolStreamCallbacks


@dataclass
class StreamCallbackSetup:
    mode: str
    message_id: str
    on_delta: Callable[[str], None]
    on_reasoning_delta: Callable[[str], None]
    function_tool_callbacks: FunctionToolStreamCallbacks | None = None
    plain_message_callbacks: PlainMessageStreamCallbacks | None = None


def setup_stream_callbacks(
    *,
    send_event: Callable[[dict[str, Any]], None],
    has_function_tools: bool,
    message_id_factory: Callable[[], str],
) -> StreamCallbackSetup:
    message_id = message_id_factory()
    if has_function_tools:
        callbacks = FunctionToolStreamCallbacks(
            send_event=send_event,
            tool_message_id=message_id,
        )
        return StreamCallbackSetup(
            mode="function_tools",
            message_id=message_id,
            on_delta=callbacks.on_delta,
            on_reasoning_delta=callbacks.on_reasoning_delta,
            function_tool_callbacks=callbacks,
        )

    callbacks = PlainMessageStreamCallbacks(
        send_event=send_event,
        message_id=message_id,
    )
    callbacks.emit_start_events()
    return StreamCallbackSetup(
        mode="plain_message",
        message_id=message_id,
        on_delta=callbacks.on_delta,
        on_reasoning_delta=callbacks.on_reasoning_delta,
        plain_message_callbacks=callbacks,
    )
