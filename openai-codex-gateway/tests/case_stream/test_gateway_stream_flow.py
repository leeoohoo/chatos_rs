#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.flow import (  # noqa: E402
    build_stream_function_call_events,
    build_stream_message_delta_event,
    build_stream_message_finalize_events,
    build_stream_message_start_events,
    build_stream_reasoning_delta_event,
    build_stream_reasoning_done_event,
    response_completion_event_type,
)
from gateway_base.types import ToolCallRecord  # noqa: E402


class GatewayStreamFlowTest(unittest.TestCase):
    def test_build_stream_message_start_events(self) -> None:
        events = build_stream_message_start_events("msg_1")
        self.assertEqual(len(events), 2)
        self.assertEqual(events[0]["type"], "response.output_item.added")
        self.assertEqual(events[0]["item"]["id"], "msg_1")
        self.assertEqual(events[1]["type"], "response.content_part.added")
        self.assertEqual(events[1]["item_id"], "msg_1")

    def test_build_stream_message_finalize_events(self) -> None:
        done_message, events = build_stream_message_finalize_events("msg_2", "hello")
        self.assertEqual(done_message["id"], "msg_2")
        self.assertEqual(done_message["content"][0]["text"], "hello")
        self.assertEqual([event["type"] for event in events], [
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
        ])
        self.assertEqual(events[2]["item"], done_message)

    def test_build_stream_function_call_events_with_arguments(self) -> None:
        call = ToolCallRecord(
            call_id="call_1",
            name="get_weather",
            arguments={"city": "Shanghai"},
        )
        done_item, events = build_stream_function_call_events(
            call,
            output_index=1,
            function_item_id="fc_1",
        )
        self.assertEqual(done_item["id"], "fc_1")
        self.assertEqual(done_item["arguments"], '{"city": "Shanghai"}')
        self.assertEqual([event["type"] for event in events], [
            "response.output_item.added",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
        ])

    def test_build_stream_function_call_events_without_arguments_delta(self) -> None:
        call = ToolCallRecord(
            call_id="call_2",
            name="noop",
            arguments="",
        )
        _, events = build_stream_function_call_events(
            call,
            output_index=0,
            function_item_id="fc_2",
        )
        self.assertEqual([event["type"] for event in events], [
            "response.output_item.added",
            "response.function_call_arguments.done",
            "response.output_item.done",
        ])

    def test_small_event_helpers(self) -> None:
        delta = build_stream_message_delta_event("msg_3", "d")
        reasoning_delta = build_stream_reasoning_delta_event("r")
        reasoning_done = build_stream_reasoning_done_event("R")
        self.assertEqual(delta["type"], "response.output_text.delta")
        self.assertEqual(reasoning_delta["type"], "response.reasoning.delta")
        self.assertEqual(reasoning_done["type"], "response.reasoning.done")
        self.assertEqual(response_completion_event_type("completed"), "response.completed")
        self.assertEqual(response_completion_event_type("failed"), "response.failed")


if __name__ == "__main__":
    unittest.main()
