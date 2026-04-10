#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.branch_dispatcher import dispatch_stream_branch  # noqa: E402
from gateway_stream.callback_setup import (  # noqa: E402
    StreamCallbackSetup,
    setup_stream_callbacks,
)


class GatewayStreamBranchDispatcherTest(unittest.TestCase):
    def test_dispatch_stream_branch_function_tools(self) -> None:
        callback_setup = setup_stream_callbacks(
            send_event=lambda _event: None,
            has_function_tools=True,
            message_id_factory=lambda: "msg_tool",
        )
        function_calls: list[tuple[str, Any]] = []
        plain_calls = 0

        def on_function_tools(callbacks: Any, message_id: str) -> None:
            function_calls.append((message_id, callbacks))

        def on_plain_message(_callbacks: Any, _message_id: str) -> None:
            nonlocal plain_calls
            plain_calls += 1

        mode = dispatch_stream_branch(
            callback_setup=callback_setup,
            on_function_tools=on_function_tools,
            on_plain_message=on_plain_message,
        )

        self.assertEqual(mode, "function_tools")
        self.assertEqual(plain_calls, 0)
        self.assertEqual(len(function_calls), 1)
        self.assertEqual(function_calls[0][0], "msg_tool")
        self.assertIs(function_calls[0][1], callback_setup.function_tool_callbacks)

    def test_dispatch_stream_branch_plain_message(self) -> None:
        callback_setup = setup_stream_callbacks(
            send_event=lambda _event: None,
            has_function_tools=False,
            message_id_factory=lambda: "msg_plain",
        )
        function_calls = 0
        plain_calls: list[tuple[str, Any]] = []

        def on_function_tools(_callbacks: Any, _message_id: str) -> None:
            nonlocal function_calls
            function_calls += 1

        def on_plain_message(callbacks: Any, message_id: str) -> None:
            plain_calls.append((message_id, callbacks))

        mode = dispatch_stream_branch(
            callback_setup=callback_setup,
            on_function_tools=on_function_tools,
            on_plain_message=on_plain_message,
        )

        self.assertEqual(mode, "plain_message")
        self.assertEqual(function_calls, 0)
        self.assertEqual(len(plain_calls), 1)
        self.assertEqual(plain_calls[0][0], "msg_plain")
        self.assertIs(plain_calls[0][1], callback_setup.plain_message_callbacks)

    def test_dispatch_stream_branch_missing_callbacks_rejected(self) -> None:
        missing_function = StreamCallbackSetup(
            mode="function_tools",
            message_id="msg_1",
            on_delta=lambda _delta: None,
            on_reasoning_delta=lambda _delta: None,
            function_tool_callbacks=None,
            plain_message_callbacks=None,
        )
        with self.assertRaises(RuntimeError):
            dispatch_stream_branch(
                callback_setup=missing_function,
                on_function_tools=lambda _callbacks, _message_id: None,
                on_plain_message=lambda _callbacks, _message_id: None,
            )

        missing_plain = StreamCallbackSetup(
            mode="plain_message",
            message_id="msg_2",
            on_delta=lambda _delta: None,
            on_reasoning_delta=lambda _delta: None,
            function_tool_callbacks=None,
            plain_message_callbacks=None,
        )
        with self.assertRaises(RuntimeError):
            dispatch_stream_branch(
                callback_setup=missing_plain,
                on_function_tools=lambda _callbacks, _message_id: None,
                on_plain_message=lambda _callbacks, _message_id: None,
            )


if __name__ == "__main__":
    unittest.main()
