#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.message_callbacks import PlainMessageStreamCallbacks  # noqa: E402


class GatewayStreamMessageCallbacksTest(unittest.TestCase):
    def test_emit_start_events(self) -> None:
        events: list[dict] = []
        callbacks = PlainMessageStreamCallbacks(
            send_event=events.append,
            message_id="msg_1",
        )

        callbacks.emit_start_events()

        self.assertEqual(len(events), 2)
        self.assertEqual(events[0]["type"], "response.output_item.added")
        self.assertEqual(events[0]["item"]["id"], "msg_1")
        self.assertEqual(events[1]["type"], "response.content_part.added")
        self.assertEqual(events[1]["item_id"], "msg_1")

    def test_on_delta_and_reasoning_delta(self) -> None:
        events: list[dict] = []
        callbacks = PlainMessageStreamCallbacks(
            send_event=events.append,
            message_id="msg_2",
        )

        callbacks.on_delta("hello")
        callbacks.on_reasoning_delta("")
        callbacks.on_reasoning_delta("trace")

        self.assertEqual(callbacks.chunks, ["hello"])
        self.assertEqual(callbacks.reasoning_chunks, ["trace"])
        self.assertEqual([event["type"] for event in events], [
            "response.output_text.delta",
            "response.reasoning.delta",
        ])
        self.assertEqual(events[0]["item_id"], "msg_2")
        self.assertEqual(events[0]["delta"], "hello")
        self.assertEqual(events[1]["delta"], "trace")


if __name__ == "__main__":
    unittest.main()
