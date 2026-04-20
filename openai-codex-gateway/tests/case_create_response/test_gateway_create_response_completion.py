#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from create_response.completion import finalize_create_response  # noqa: E402
from gateway_base.types import ToolCallRecord, TurnResult  # noqa: E402


class FakeStore:
    def __init__(self) -> None:
        self.put_calls: list[tuple[str, str]] = []

    def put(self, response_id: str, thread_id: str) -> None:
        self.put_calls.append((response_id, thread_id))


class GatewayCreateResponseCompletionTest(unittest.TestCase):
    def test_finalize_create_response_with_pending_tool_calls(self) -> None:
        store = FakeStore()
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="ignored",
            reasoning_text="reasoning",
            status="completed",
            usage={"total_tokens": 3},
            error=None,
            tool_calls=[
                ToolCallRecord(
                    call_id="call_1",
                    name="get_weather",
                    arguments={"city": "Shanghai"},
                )
            ],
        )

        response_id, body = finalize_create_response(
            store=store,
            result=result,
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[{"type": "function"}],
            provided_tool_outputs={},
            created_at=123,
            response_id_factory=lambda: "resp_1",
            message_id_factory=lambda: "msg_1",
            function_item_id_factory=lambda: "fc_1",
        )

        self.assertEqual(response_id, "resp_1")
        self.assertEqual(store.put_calls, [("resp_1", "thread_1")])
        self.assertEqual(body["id"], "resp_1")
        self.assertEqual(body["output"][0]["id"], "fc_1")
        self.assertEqual(body["metadata"]["thread_id"], "thread_1")
        self.assertEqual(body["metadata"]["pending_tool_calls"][0]["call_id"], "call_1")
        self.assertEqual(body["reasoning"], "reasoning")

    def test_finalize_create_response_with_message_output(self) -> None:
        store = FakeStore()
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

        response_id, body = finalize_create_response(
            store=store,
            result=result,
            model_name="codex-2",
            previous_response_id=None,
            response_tools=[],
            provided_tool_outputs={},
            created_at=456,
            response_id_factory=lambda: "resp_2",
            message_id_factory=lambda: "msg_2",
            function_item_id_factory=lambda: "unused",
        )

        self.assertEqual(response_id, "resp_2")
        self.assertEqual(store.put_calls, [("resp_2", "thread_2")])
        self.assertEqual(body["output"][0]["id"], "msg_2")
        self.assertEqual(body["output_text"], "final answer")
        self.assertEqual(body["metadata"]["turn_id"], "turn_2")
        self.assertNotIn("reasoning", body)


if __name__ == "__main__":
    unittest.main()
