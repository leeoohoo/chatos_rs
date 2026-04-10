#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from create_response.parser import CreateResponseContext  # noqa: E402
from create_response.turn_runner import run_create_response_turn  # noqa: E402
from gateway_base.types import TurnResult  # noqa: E402


class FakeBridge:
    def __init__(self, result: TurnResult) -> None:
        self.result = result
        self.last_kwargs: dict[str, Any] | None = None

    def _run_turn(self, **kwargs: Any) -> TurnResult:
        self.last_kwargs = kwargs
        return self.result


class GatewayCreateResponseTurnRunnerTest(unittest.TestCase):
    def test_run_create_response_turn_passthrough(self) -> None:
        expected = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="out",
            reasoning_text="reasoning",
            status="completed",
            usage={"total_tokens": 1},
            error=None,
            tool_calls=[],
        )
        bridge = FakeBridge(expected)
        context = CreateResponseContext(
            input_items=[{"type": "text", "text": "hello"}],
            model="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            reasoning_effort="high",
            reasoning_summary="concise",
            response_tools=[],
        )
        function_tools = [{"type": "function", "name": "weather"}]
        provided_tool_outputs = {"call_1": [{"type": "inputText", "text": "ok"}]}
        on_delta = lambda _: None

        result = run_create_response_turn(
            bridge=bridge,
            context=context,
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox": "workspace-write"},
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            on_delta=on_delta,
        )

        self.assertIs(result, expected)
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

    def test_run_create_response_turn_model_passthrough_any(self) -> None:
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
        context = CreateResponseContext(
            input_items=[{"type": "text", "text": "x"}],
            model=123,
            model_name="codex-default",
            previous_response_id=None,
            reasoning_effort=None,
            reasoning_summary=None,
            response_tools=[],
        )

        run_create_response_turn(
            bridge=bridge,
            context=context,
            api_key=None,
            request_cwd=None,
            request_config_overrides=None,
            function_tools=[],
            provided_tool_outputs={},
            on_delta=None,
        )

        self.assertIsNotNone(bridge.last_kwargs)
        kwargs = bridge.last_kwargs or {}
        self.assertEqual(kwargs["model"], 123)
        self.assertIsNone(kwargs["previous_response_id"])
        self.assertIsNone(kwargs["reasoning_effort"])
        self.assertIsNone(kwargs["reasoning_summary"])


if __name__ == "__main__":
    unittest.main()
