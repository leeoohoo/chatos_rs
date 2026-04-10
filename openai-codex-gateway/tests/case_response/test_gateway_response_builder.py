#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_response.builder import (  # noqa: E402
    build_non_stream_response_body,
    build_stream_message_item,
    build_stream_response_object,
)
from gateway_base.types import ToolCallRecord, TurnResult  # noqa: E402


class GatewayResponseBuilderTest(unittest.TestCase):
    def test_build_non_stream_response_body_for_pending_tool_calls(self) -> None:
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="ignored",
            reasoning_text="reasoning text",
            status="completed",
            usage={"input_tokens": 1, "output_tokens": 2},
            error=None,
            tool_calls=[
                ToolCallRecord(
                    call_id="call_1",
                    name="get_weather",
                    arguments={"city": "Shanghai"},
                )
            ],
        )

        body = build_non_stream_response_body(
            response_id="resp_1",
            model_name="codex-mini",
            result=result,
            previous_response_id="resp_prev",
            response_tools=[{"type": "function"}],
            provided_tool_outputs={},
            message_id="msg_1",
            created_at=123,
            function_item_id_factory=lambda: "fc_1",
        )

        self.assertEqual(body["id"], "resp_1")
        self.assertEqual(body["status"], "completed")
        self.assertEqual(body["output_text"], "")
        self.assertEqual(body["output"][0]["id"], "fc_1")
        self.assertEqual(body["output"][0]["type"], "function_call")
        self.assertEqual(body["output"][0]["call_id"], "call_1")
        self.assertEqual(body["output"][0]["name"], "get_weather")
        self.assertEqual(body["output"][0]["arguments"], '{"city": "Shanghai"}')
        self.assertEqual(body["metadata"]["thread_id"], "thread_1")
        self.assertEqual(body["metadata"]["pending_tool_calls"][0]["call_id"], "call_1")
        self.assertEqual(body["reasoning"], "reasoning text")

    def test_build_non_stream_response_body_for_message_output(self) -> None:
        result = TurnResult(
            thread_id="thread_2",
            turn_id="turn_2",
            output_text="final answer",
            reasoning_text="",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )

        body = build_non_stream_response_body(
            response_id="resp_2",
            model_name="codex-default",
            result=result,
            previous_response_id=None,
            response_tools=[],
            provided_tool_outputs={},
            message_id="msg_2",
            created_at=456,
            function_item_id_factory=lambda: "unused",
        )

        self.assertEqual(body["status"], "completed")
        self.assertEqual(body["output_text"], "final answer")
        self.assertEqual(body["output"][0]["id"], "msg_2")
        self.assertEqual(body["output"][0]["type"], "message")
        self.assertEqual(body["metadata"]["thread_id"], "thread_2")
        self.assertNotIn("reasoning", body)

    def test_build_stream_response_object_optional_fields(self) -> None:
        minimal = build_stream_response_object(
            response_id="resp_stream_1",
            created_at=111,
            model_name="model_1",
            response_tools=[],
            status="in_progress",
            output=[],
        )

        self.assertEqual(minimal["id"], "resp_stream_1")
        self.assertEqual(minimal["status"], "in_progress")
        self.assertEqual(minimal["parallel_tool_calls"], False)
        self.assertEqual(minimal["tool_choice"], "auto")
        self.assertNotIn("usage", minimal)
        self.assertNotIn("error", minimal)
        self.assertNotIn("reasoning", minimal)
        self.assertNotIn("metadata", minimal)
        self.assertNotIn("previous_response_id", minimal)

        full = build_stream_response_object(
            response_id="resp_stream_2",
            created_at=222,
            model_name="model_2",
            response_tools=[{"type": "function"}],
            status="completed",
            output=[{"id": "item_1"}],
            usage={"total_tokens": 10},
            error={"message": "boom"},
            reasoning="trace",
            previous_response_id="resp_prev",
            metadata={"thread_id": "thread_3"},
        )

        self.assertEqual(full["usage"]["total_tokens"], 10)
        self.assertEqual(full["error"]["message"], "boom")
        self.assertEqual(full["reasoning"], "trace")
        self.assertEqual(full["previous_response_id"], "resp_prev")
        self.assertEqual(full["metadata"]["thread_id"], "thread_3")

    def test_build_stream_message_item(self) -> None:
        item = build_stream_message_item("msg_3", "hello", status="completed")
        self.assertEqual(item["id"], "msg_3")
        self.assertEqual(item["type"], "message")
        self.assertEqual(item["status"], "completed")
        self.assertEqual(item["content"][0]["type"], "output_text")
        self.assertEqual(item["content"][0]["text"], "hello")
        self.assertEqual(item["content"][0]["annotations"], [])


if __name__ == "__main__":
    unittest.main()
