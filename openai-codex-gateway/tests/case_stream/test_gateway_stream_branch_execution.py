#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.branch_execution import (  # noqa: E402
    execute_function_tools_branch,
    execute_plain_message_branch,
)
from gateway_stream.message_callbacks import PlainMessageStreamCallbacks  # noqa: E402
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402
from gateway_stream.tool_callbacks import FunctionToolStreamCallbacks  # noqa: E402
from gateway_base.types import TurnResult  # noqa: E402


class GatewayStreamBranchExecutionTest(unittest.TestCase):
    def test_execute_function_tools_branch(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort="high",
            reasoning_summary="concise",
        )
        callbacks = FunctionToolStreamCallbacks(
            send_event=lambda _event: None,
            tool_message_id="msg_tool",
            tool_chunks=["delta_1"],
            reasoning_chunks=["reason_1"],
            tool_message_started=True,
        )
        expected_result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="out",
            reasoning_text="reason",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )
        run_calls: list[dict[str, Any]] = []
        complete_calls: list[dict[str, Any]] = []

        def run_turn(**kwargs: Any) -> TurnResult:
            run_calls.append(kwargs)
            return expected_result

        def complete_stream(**kwargs: Any) -> None:
            complete_calls.append(kwargs)

        execute_function_tools_branch(
            callbacks=callbacks,
            tool_message_id="msg_tool",
            bridge=object(),
            store=object(),
            response_id="resp_1",
            input_items=[{"type": "text", "text": "hello"}],
            stream_context=stream_context,
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox": "workspace-write"},
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "inputText", "text": "ok"}]},
            on_delta=lambda _delta: None,
            on_reasoning_delta=lambda _delta: None,
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            function_item_id_factory=lambda: "fc_1",
            send_done_marker=lambda: None,
            run_turn=run_turn,
            complete_stream=complete_stream,
        )

        self.assertEqual(len(run_calls), 1)
        self.assertEqual(run_calls[0]["response_id"], "resp_1")
        self.assertEqual(run_calls[0]["request_cwd"], "/tmp/demo")
        self.assertEqual(run_calls[0]["api_key"], "k")

        self.assertEqual(len(complete_calls), 1)
        complete_kwargs = complete_calls[0]
        self.assertIs(complete_kwargs["result"], expected_result)
        self.assertEqual(complete_kwargs["tool_message_id"], "msg_tool")
        self.assertEqual(complete_kwargs["tool_chunks"], ["delta_1"])
        self.assertEqual(complete_kwargs["reasoning_chunks"], ["reason_1"])
        self.assertEqual(complete_kwargs["previous_response_id"], "resp_prev")

    def test_execute_plain_message_branch(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id=None,
            response_tools=[],
            reasoning_effort=None,
            reasoning_summary=None,
        )
        callbacks = PlainMessageStreamCallbacks(
            send_event=lambda _event: None,
            message_id="msg_plain",
            chunks=["delta_1"],
            reasoning_chunks=["reason_1"],
        )
        expected_result = TurnResult(
            thread_id="thread_2",
            turn_id="turn_2",
            output_text="out",
            reasoning_text="reason",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )
        run_calls: list[dict[str, Any]] = []
        complete_calls: list[dict[str, Any]] = []

        def run_turn(**kwargs: Any) -> TurnResult:
            run_calls.append(kwargs)
            return expected_result

        def complete_stream(**kwargs: Any) -> None:
            complete_calls.append(kwargs)

        execute_plain_message_branch(
            callbacks=callbacks,
            message_id="msg_plain",
            bridge=object(),
            store=object(),
            response_id="resp_2",
            input_items=[{"type": "text", "text": "hello"}],
            stream_context=stream_context,
            api_key=None,
            request_cwd=None,
            request_config_overrides=None,
            function_tools=[],
            provided_tool_outputs={},
            on_delta=lambda _delta: None,
            on_reasoning_delta=lambda _delta: None,
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            send_done_marker=lambda: None,
            run_turn=run_turn,
            complete_stream=complete_stream,
        )

        self.assertEqual(len(run_calls), 1)
        self.assertEqual(run_calls[0]["response_id"], "resp_2")
        self.assertIsNone(run_calls[0]["request_cwd"])
        self.assertIsNone(run_calls[0]["api_key"])

        self.assertEqual(len(complete_calls), 1)
        complete_kwargs = complete_calls[0]
        self.assertIs(complete_kwargs["result"], expected_result)
        self.assertEqual(complete_kwargs["message_id"], "msg_plain")
        self.assertEqual(complete_kwargs["chunks"], ["delta_1"])
        self.assertEqual(complete_kwargs["reasoning_chunks"], ["reason_1"])
        self.assertIsNone(complete_kwargs["previous_response_id"])


if __name__ == "__main__":
    unittest.main()
