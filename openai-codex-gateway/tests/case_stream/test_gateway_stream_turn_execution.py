#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.request_parser import StreamRequestContext  # noqa: E402
from gateway_stream.turn_execution import run_and_persist_stream_turn  # noqa: E402
from gateway_base.types import TurnResult  # noqa: E402


class FakeBridge:
    def __init__(self, result: TurnResult) -> None:
        self.result = result
        self.last_kwargs: dict[str, Any] | None = None

    def _run_turn(self, **kwargs: Any) -> TurnResult:
        self.last_kwargs = kwargs
        return self.result


class FakeStore:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    def put(self, response_id: str, thread_id: str) -> None:
        self.calls.append((response_id, thread_id))


class GatewayStreamTurnExecutionTest(unittest.TestCase):
    def test_run_and_persist_stream_turn_passthrough(self) -> None:
        expected = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="hello",
            reasoning_text="think",
            status="completed",
            usage={"total_tokens": 3},
            error=None,
            tool_calls=[],
        )
        bridge = FakeBridge(expected)
        store = FakeStore()
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort="high",
            reasoning_summary="concise",
        )
        function_tools = [{"type": "function", "name": "weather"}]
        provided_tool_outputs = {"call_1": [{"type": "inputText", "text": "ok"}]}
        on_delta = lambda _: None
        on_reasoning_delta = lambda _: None

        result = run_and_persist_stream_turn(
            bridge=bridge,
            store=store,
            response_id="resp_1",
            input_items=[{"type": "text", "text": "hello"}],
            stream_context=stream_context,
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox": "workspace-write"},
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
        )

        self.assertIs(result, expected)
        self.assertEqual(store.calls, [("resp_1", "thread_1")])
        self.assertIsNotNone(bridge.last_kwargs)
        kwargs = bridge.last_kwargs or {}
        self.assertEqual(kwargs["model"], "codex-1")
        self.assertEqual(kwargs["reasoning_effort"], "high")
        self.assertEqual(kwargs["reasoning_summary"], "concise")
        self.assertEqual(kwargs["previous_response_id"], "resp_prev")
        self.assertEqual(kwargs["api_key"], "k")
        self.assertEqual(kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(kwargs["request_config_overrides"], {"sandbox": "workspace-write"})
        self.assertIs(kwargs["function_tools"], function_tools)
        self.assertIs(kwargs["provided_tool_outputs"], provided_tool_outputs)
        self.assertIs(kwargs["on_delta"], on_delta)
        self.assertIs(kwargs["on_reasoning_delta"], on_reasoning_delta)

    def test_run_and_persist_stream_turn_model_normalization(self) -> None:
        bridge = FakeBridge(
            TurnResult(
                thread_id="thread_2",
                turn_id="turn_2",
                output_text="",
                reasoning_text="",
                status="completed",
                usage=None,
                error=None,
                tool_calls=[],
            )
        )
        store = FakeStore()
        stream_context = StreamRequestContext(
            model_raw=123,
            model_name="codex-default",
            previous_response_id=None,
            response_tools=[],
            reasoning_effort=None,
            reasoning_summary=None,
        )

        run_and_persist_stream_turn(
            bridge=bridge,
            store=store,
            response_id="resp_2",
            input_items=[{"type": "text", "text": "x"}],
            stream_context=stream_context,
            api_key=None,
            request_cwd=None,
            request_config_overrides=None,
            function_tools=[],
            provided_tool_outputs={},
            on_delta=None,
            on_reasoning_delta=None,
        )

        self.assertEqual(store.calls, [("resp_2", "thread_2")])
        self.assertIsNotNone(bridge.last_kwargs)
        kwargs = bridge.last_kwargs or {}
        self.assertIsNone(kwargs["model"])
        self.assertIsNone(kwargs["reasoning_effort"])
        self.assertIsNone(kwargs["reasoning_summary"])
        self.assertIsNone(kwargs["previous_response_id"])


if __name__ == "__main__":
    unittest.main()
