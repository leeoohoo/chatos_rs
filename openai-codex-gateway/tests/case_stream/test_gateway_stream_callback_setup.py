#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.callback_setup import setup_stream_callbacks  # noqa: E402


class GatewayStreamCallbackSetupTest(unittest.TestCase):
    def test_setup_stream_callbacks_for_function_tools(self) -> None:
        events: list[dict[str, Any]] = []

        def send_event(event: dict[str, Any]) -> None:
            events.append(event)

        setup = setup_stream_callbacks(
            send_event=send_event,
            has_function_tools=True,
            message_id_factory=lambda: "msg_tool",
        )

        self.assertEqual(setup.mode, "function_tools")
        self.assertEqual(setup.message_id, "msg_tool")
        self.assertIsNotNone(setup.function_tool_callbacks)
        self.assertIsNone(setup.plain_message_callbacks)
        self.assertEqual(events, [])

        setup.on_delta("d")
        self.assertEqual([event["type"] for event in events], [
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
        ])
        self.assertEqual(events[2]["item_id"], "msg_tool")

    def test_setup_stream_callbacks_for_plain_message(self) -> None:
        events: list[dict[str, Any]] = []

        def send_event(event: dict[str, Any]) -> None:
            events.append(event)

        setup = setup_stream_callbacks(
            send_event=send_event,
            has_function_tools=False,
            message_id_factory=lambda: "msg_plain",
        )

        self.assertEqual(setup.mode, "plain_message")
        self.assertEqual(setup.message_id, "msg_plain")
        self.assertIsNone(setup.function_tool_callbacks)
        self.assertIsNotNone(setup.plain_message_callbacks)
        self.assertEqual([event["type"] for event in events], [
            "response.output_item.added",
            "response.content_part.added",
        ])

        setup.on_delta("x")
        setup.on_reasoning_delta("r")
        self.assertEqual(events[2]["type"], "response.output_text.delta")
        self.assertEqual(events[2]["item_id"], "msg_plain")
        self.assertEqual(events[3]["type"], "response.reasoning.delta")
        self.assertEqual(events[3]["delta"], "r")


if __name__ == "__main__":
    unittest.main()
