#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.tool_callbacks import FunctionToolStreamCallbacks  # noqa: E402


class GatewayStreamToolCallbacksTest(unittest.TestCase):
    def test_on_delta_starts_message_once_and_emits_delta(self) -> None:
        events: list[dict] = []
        callbacks = FunctionToolStreamCallbacks(
            send_event=events.append,
            tool_message_id="msg_1",
        )

        callbacks.on_delta("hello")
        callbacks.on_delta(" world")

        self.assertTrue(callbacks.tool_message_started)
        self.assertEqual(callbacks.tool_chunks, ["hello", " world"])
        self.assertEqual(
            [event["type"] for event in events],
            [
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
            ],
        )
        self.assertEqual(events[0]["item"]["id"], "msg_1")
        self.assertEqual(events[2]["delta"], "hello")
        self.assertEqual(events[3]["delta"], " world")

    def test_on_reasoning_delta_ignores_empty(self) -> None:
        events: list[dict] = []
        callbacks = FunctionToolStreamCallbacks(
            send_event=events.append,
            tool_message_id="msg_2",
        )

        callbacks.on_reasoning_delta("")
        callbacks.on_reasoning_delta("trace")

        self.assertEqual(callbacks.reasoning_chunks, ["trace"])
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["type"], "response.reasoning.delta")
        self.assertEqual(events[0]["delta"], "trace")


if __name__ == "__main__":
    unittest.main()
