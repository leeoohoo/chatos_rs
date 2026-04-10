#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.orchestrator import (  # noqa: E402
    emit_function_tools_result,
    emit_plain_message_result,
)
from gateway_base.types import ToolCallRecord, TurnResult  # noqa: E402


class GatewayStreamOrchestratorTest(unittest.TestCase):
    def test_emit_function_tools_result_with_pending_calls(self) -> None:
        call = ToolCallRecord(call_id="call_1", name="get_weather", arguments={"city": "Shanghai"})
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="",
            reasoning_text="",
            status="completed",
            usage={"total_tokens": 3},
            error=None,
            tool_calls=[call],
        )
        events: list[dict] = []

        emit_function_tools_result(
            send_event=events.append,
            response_obj=lambda **kwargs: kwargs,
            result=result,
            unresolved_calls=[call],
            previous_response_id="resp_prev",
            tool_message_id="msg_1",
            tool_chunks=["tool partial"],
            reasoning_chunks=["reasoning part"],
            tool_message_started=True,
            function_item_id_factory=lambda: "fc_1",
        )

        self.assertEqual(events[0]["type"], "response.reasoning.done")
        self.assertEqual(events[-1]["type"], "response.completed")
        response_payload = events[-1]["response"]
        self.assertEqual(response_payload["status"], "completed")
        self.assertEqual(response_payload["metadata"]["pending_tool_calls"][0]["call_id"], "call_1")
        self.assertTrue(
            any(event["type"] == "response.function_call_arguments.done" for event in events)
        )

    def test_emit_function_tools_result_without_pending_calls(self) -> None:
        result = TurnResult(
            thread_id="thread_2",
            turn_id="turn_2",
            output_text="final text",
            reasoning_text="reasoning",
            status="failed",
            usage=None,
            error={"message": "boom"},
            tool_calls=[],
        )
        events: list[dict] = []

        emit_function_tools_result(
            send_event=events.append,
            response_obj=lambda **kwargs: kwargs,
            result=result,
            unresolved_calls=[],
            previous_response_id=None,
            tool_message_id="msg_2",
            tool_chunks=[],
            reasoning_chunks=[],
            tool_message_started=False,
            function_item_id_factory=lambda: "unused",
        )

        self.assertEqual(events[0]["type"], "response.reasoning.done")
        self.assertEqual(events[1]["type"], "response.output_item.added")
        self.assertEqual(events[2]["type"], "response.content_part.added")
        self.assertEqual(events[3]["type"], "response.output_text.delta")
        self.assertEqual(events[-1]["type"], "response.failed")
        self.assertEqual(events[-1]["response"]["status"], "failed")

    def test_emit_plain_message_result(self) -> None:
        result = TurnResult(
            thread_id="thread_3",
            turn_id="turn_3",
            output_text="",
            reasoning_text="",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )
        events: list[dict] = []

        emit_plain_message_result(
            send_event=events.append,
            response_obj=lambda **kwargs: kwargs,
            result=result,
            previous_response_id="resp_prev_2",
            message_id="msg_3",
            chunks=["hello ", "world"],
            reasoning_chunks=["trace"],
        )

        self.assertEqual(events[0]["type"], "response.reasoning.done")
        self.assertEqual(events[1]["type"], "response.output_text.done")
        self.assertEqual(events[2]["type"], "response.content_part.done")
        self.assertEqual(events[3]["type"], "response.output_item.done")
        self.assertEqual(events[4]["type"], "response.completed")
        self.assertEqual(
            events[4]["response"]["output"][0]["content"][0]["text"],
            "hello world",
        )


if __name__ == "__main__":
    unittest.main()
