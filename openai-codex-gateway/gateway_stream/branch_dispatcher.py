from __future__ import annotations

from typing import Callable

from gateway_stream.callback_setup import StreamCallbackSetup
from gateway_stream.message_callbacks import PlainMessageStreamCallbacks
from gateway_stream.tool_callbacks import FunctionToolStreamCallbacks


def dispatch_stream_branch(
    *,
    callback_setup: StreamCallbackSetup,
    on_function_tools: Callable[[FunctionToolStreamCallbacks, str], None],
    on_plain_message: Callable[[PlainMessageStreamCallbacks, str], None],
) -> str:
    if callback_setup.mode == "function_tools":
        callbacks = callback_setup.function_tool_callbacks
        if callbacks is None:
            raise RuntimeError("function tool callbacks not initialized")
        on_function_tools(callbacks, callback_setup.message_id)
        return "function_tools"

    callbacks = callback_setup.plain_message_callbacks
    if callbacks is None:
        raise RuntimeError("plain message callbacks not initialized")
    on_plain_message(callbacks, callback_setup.message_id)
    return "plain_message"
