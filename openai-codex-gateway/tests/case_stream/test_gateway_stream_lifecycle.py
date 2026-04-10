#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.lifecycle import (  # noqa: E402
    emit_response_created_event,
    persist_response_thread_mapping,
)
from gateway_base.types import TurnResult  # noqa: E402


class FakeStore:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    def put(self, response_id: str, thread_id: str) -> None:
        self.calls.append((response_id, thread_id))


class GatewayStreamLifecycleTest(unittest.TestCase):
    def test_emit_response_created_event(self) -> None:
        events: list[dict[str, object]] = []

        def send_event(event: dict[str, object]) -> None:
            events.append(event)

        def response_obj(**kwargs: object) -> dict[str, object]:
            return {
                "id": "resp_1",
                **kwargs,
            }

        emit_response_created_event(
            send_event=send_event,
            response_obj=response_obj,
            previous_response_id="resp_prev",
        )

        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["type"], "response.created")
        response = events[0]["response"]
        self.assertIsInstance(response, dict)
        if isinstance(response, dict):
            self.assertEqual(response["status"], "in_progress")
            self.assertEqual(response["output"], [])
            self.assertEqual(response["previous_response_id"], "resp_prev")

    def test_persist_response_thread_mapping(self) -> None:
        store = FakeStore()
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="",
            reasoning_text="",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )

        persist_response_thread_mapping(
            store=store,
            response_id="resp_1",
            result=result,
        )

        self.assertEqual(store.calls, [("resp_1", "thread_1")])


if __name__ == "__main__":
    unittest.main()
