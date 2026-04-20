#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.callback_setup import StreamCallbackSetup  # noqa: E402
from gateway_stream.main_flow import run_stream_main_flow  # noqa: E402
from gateway_stream.pre_branch_setup import StreamPreBranchSetup  # noqa: E402
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402


class GatewayStreamMainFlowTest(unittest.TestCase):
    def test_run_stream_main_flow_function_tools(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort="high",
            reasoning_summary="concise",
        )
        function_callbacks = object()
        on_delta = lambda _delta: None
        on_reasoning_delta = lambda _delta: None
        callback_setup = StreamCallbackSetup(
            mode="function_tools",
            message_id="msg_tool",
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
            function_tool_callbacks=function_callbacks,  # type: ignore[arg-type]
        )

        pre_branch_calls: list[dict[str, Any]] = []
        dispatch_calls: list[dict[str, Any]] = []
        function_executor_calls: list[dict[str, Any]] = []
        plain_executor_calls = 0

        def pre_branch_setup_fn(**kwargs: Any) -> StreamPreBranchSetup:
            pre_branch_calls.append(kwargs)
            return StreamPreBranchSetup(
                input_items=[{"type": "text", "text": "hello"}],
                callback_setup=callback_setup,
            )

        def dispatch_branch_fn(**kwargs: Any) -> str:
            dispatch_calls.append(kwargs)
            kwargs["on_function_tools"](function_callbacks, "msg_tool")
            return "function_tools"

        def function_tools_executor(callbacks: Any, tool_message_id: str, **kwargs: Any) -> None:
            function_executor_calls.append(
                {
                    "callbacks": callbacks,
                    "tool_message_id": tool_message_id,
                    **kwargs,
                }
            )

        def plain_message_executor(_callbacks: Any, _message_id: str, **_kwargs: Any) -> None:
            nonlocal plain_executor_calls
            plain_executor_calls += 1

        run_stream_main_flow(
            payload={"input": "hello"},
            bridge=object(),
            store=object(),
            response_id="resp_1",
            stream_context=stream_context,
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox": "workspace-write"},
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "inputText", "text": "ok"}]},
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            send_done_marker=lambda: None,
            message_id_factory=lambda: "msg_factory",
            function_item_id_factory=lambda: "fc_1",
            pre_branch_setup_fn=pre_branch_setup_fn,
            dispatch_branch_fn=dispatch_branch_fn,
            function_tools_executor=function_tools_executor,
            plain_message_executor=plain_message_executor,
        )

        self.assertEqual(len(pre_branch_calls), 1)
        self.assertTrue(pre_branch_calls[0]["has_function_tools"])
        self.assertEqual(pre_branch_calls[0]["previous_response_id"], "resp_prev")

        self.assertEqual(len(dispatch_calls), 1)
        self.assertIs(dispatch_calls[0]["callback_setup"], callback_setup)

        self.assertEqual(len(function_executor_calls), 1)
        exec_kwargs = function_executor_calls[0]
        self.assertIs(exec_kwargs["callbacks"], function_callbacks)
        self.assertEqual(exec_kwargs["tool_message_id"], "msg_tool")
        self.assertEqual(exec_kwargs["response_id"], "resp_1")
        self.assertEqual(exec_kwargs["input_items"][0]["text"], "hello")
        self.assertEqual(exec_kwargs["api_key"], "k")
        self.assertEqual(exec_kwargs["request_cwd"], "/tmp/demo")
        self.assertIs(exec_kwargs["on_delta"], on_delta)
        self.assertIs(exec_kwargs["on_reasoning_delta"], on_reasoning_delta)
        self.assertEqual(plain_executor_calls, 0)

    def test_run_stream_main_flow_plain_message(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id=None,
            response_tools=[],
            reasoning_effort=None,
            reasoning_summary=None,
        )
        plain_callbacks = object()
        on_delta = lambda _delta: None
        on_reasoning_delta = lambda _delta: None
        callback_setup = StreamCallbackSetup(
            mode="plain_message",
            message_id="msg_plain",
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
            plain_message_callbacks=plain_callbacks,  # type: ignore[arg-type]
        )

        function_executor_calls = 0
        plain_executor_calls: list[dict[str, Any]] = []

        def pre_branch_setup_fn(**_kwargs: Any) -> StreamPreBranchSetup:
            return StreamPreBranchSetup(
                input_items=[{"type": "text", "text": "x"}],
                callback_setup=callback_setup,
            )

        def dispatch_branch_fn(**kwargs: Any) -> str:
            kwargs["on_plain_message"](plain_callbacks, "msg_plain")
            return "plain_message"

        def function_tools_executor(_callbacks: Any, _tool_message_id: str, **_kwargs: Any) -> None:
            nonlocal function_executor_calls
            function_executor_calls += 1

        def plain_message_executor(callbacks: Any, message_id: str, **kwargs: Any) -> None:
            plain_executor_calls.append(
                {
                    "callbacks": callbacks,
                    "message_id": message_id,
                    **kwargs,
                }
            )

        run_stream_main_flow(
            payload={"input": "x"},
            bridge=object(),
            store=object(),
            response_id="resp_2",
            stream_context=stream_context,
            api_key=None,
            request_cwd=None,
            request_config_overrides=None,
            function_tools=[],
            provided_tool_outputs={},
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            send_done_marker=lambda: None,
            message_id_factory=lambda: "msg_factory",
            function_item_id_factory=lambda: "fc_2",
            pre_branch_setup_fn=pre_branch_setup_fn,
            dispatch_branch_fn=dispatch_branch_fn,
            function_tools_executor=function_tools_executor,
            plain_message_executor=plain_message_executor,
        )

        self.assertEqual(function_executor_calls, 0)
        self.assertEqual(len(plain_executor_calls), 1)
        exec_kwargs = plain_executor_calls[0]
        self.assertIs(exec_kwargs["callbacks"], plain_callbacks)
        self.assertEqual(exec_kwargs["message_id"], "msg_plain")
        self.assertEqual(exec_kwargs["response_id"], "resp_2")
        self.assertEqual(exec_kwargs["input_items"][0]["text"], "x")
        self.assertIsNone(exec_kwargs["api_key"])
        self.assertIsNone(exec_kwargs["request_cwd"])
        self.assertIs(exec_kwargs["on_delta"], on_delta)
        self.assertIs(exec_kwargs["on_reasoning_delta"], on_reasoning_delta)


if __name__ == "__main__":
    unittest.main()
