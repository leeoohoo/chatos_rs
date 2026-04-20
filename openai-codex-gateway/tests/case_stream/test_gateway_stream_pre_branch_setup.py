#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.pre_branch_setup import setup_stream_pre_branch  # noqa: E402


class GatewayStreamPreBranchSetupTest(unittest.TestCase):
    def test_setup_stream_pre_branch_for_function_tools(self) -> None:
        events: list[dict[str, Any]] = []

        def send_event(event: dict[str, Any]) -> None:
            events.append(event)

        setup = setup_stream_pre_branch(
            payload={"input": "hello"},
            provided_tool_outputs={},
            send_event=send_event,
            response_obj=lambda **kwargs: {"id": "resp_1", **kwargs},
            previous_response_id="resp_prev",
            has_function_tools=True,
            message_id_factory=lambda: "msg_tool",
        )

        self.assertEqual(setup.callback_setup.mode, "function_tools")
        self.assertEqual(setup.input_items[0]["type"], "text")
        self.assertEqual(setup.input_items[0]["text"], "hello")
        self.assertEqual([event["type"] for event in events], ["response.created"])
        self.assertEqual(events[0]["response"]["previous_response_id"], "resp_prev")

        setup.callback_setup.on_delta("d")
        self.assertEqual([event["type"] for event in events], [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
        ])
        self.assertEqual(events[-1]["item_id"], "msg_tool")

        setup.callback_setup.on_reasoning_delta("r")
        self.assertEqual(events[-1]["type"], "response.reasoning.delta")
        self.assertEqual(events[-1]["delta"], "r")

    def test_setup_stream_pre_branch_for_plain_message(self) -> None:
        events: list[dict[str, Any]] = []

        def send_event(event: dict[str, Any]) -> None:
            events.append(event)

        setup = setup_stream_pre_branch(
            payload={"input": "hello"},
            provided_tool_outputs={},
            send_event=send_event,
            response_obj=lambda **kwargs: {"id": "resp_2", **kwargs},
            previous_response_id=None,
            has_function_tools=False,
            message_id_factory=lambda: "msg_plain",
        )

        self.assertEqual(setup.callback_setup.mode, "plain_message")
        self.assertEqual([event["type"] for event in events], [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
        ])

        setup.callback_setup.on_delta("x")
        setup.callback_setup.on_reasoning_delta("r")
        self.assertEqual(events[3]["type"], "response.output_text.delta")
        self.assertEqual(events[3]["item_id"], "msg_plain")
        self.assertEqual(events[4]["type"], "response.reasoning.delta")


if __name__ == "__main__":
    unittest.main()
