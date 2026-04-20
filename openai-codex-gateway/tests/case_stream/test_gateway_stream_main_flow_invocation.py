#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.main_flow_invocation import (  # noqa: E402
    invoke_stream_main_flow_with_error_boundary,
)
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402


class GatewayStreamMainFlowInvocationTest(unittest.TestCase):
    def test_invoke_stream_main_flow_with_error_boundary_runs_main_flow(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id=None,
            response_tools=[],
            reasoning_effort="low",
            reasoning_summary="auto",
        )
        send_event = lambda _event: None
        send_done_marker = lambda: None
        response_obj = lambda **kwargs: kwargs
        message_id_factory = lambda: "msg_1"
        function_item_id_factory = lambda: "fc_1"

        boundary_calls: list[dict[str, Any]] = []
        main_flow_calls: list[dict[str, Any]] = []

        def run_main_flow_fn(**kwargs: Any) -> None:
            main_flow_calls.append(kwargs)

        def run_error_boundary_fn(**kwargs: Any) -> None:
            boundary_calls.append(kwargs)
            kwargs["run_main_flow"]()

        invoke_stream_main_flow_with_error_boundary(
            payload={"input": "hello"},
            bridge=object(),
            store=object(),
            response_id="resp_1",
            stream_context=stream_context,
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox_mode": "workspace-write"},
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            send_event=send_event,
            response_obj=response_obj,  # type: ignore[arg-type]
            send_done_marker=send_done_marker,
            message_id_factory=message_id_factory,
            function_item_id_factory=function_item_id_factory,
            debug_logger=lambda *_parts: None,
            print_traceback=lambda: None,
            run_main_flow_fn=run_main_flow_fn,
            run_error_boundary_fn=run_error_boundary_fn,
        )

        self.assertEqual(len(boundary_calls), 1)
        boundary_kwargs = boundary_calls[0]
        self.assertIs(boundary_kwargs["send_event"], send_event)
        self.assertIs(boundary_kwargs["send_done_marker"], send_done_marker)

        self.assertEqual(len(main_flow_calls), 1)
        main_flow_kwargs = main_flow_calls[0]
        self.assertEqual(main_flow_kwargs["payload"], {"input": "hello"})
        self.assertEqual(main_flow_kwargs["response_id"], "resp_1")
        self.assertIs(main_flow_kwargs["stream_context"], stream_context)
        self.assertEqual(main_flow_kwargs["api_key"], "k")
        self.assertEqual(main_flow_kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(main_flow_kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(main_flow_kwargs["send_event"], send_event)
        self.assertIs(main_flow_kwargs["response_obj"], response_obj)
        self.assertIs(main_flow_kwargs["send_done_marker"], send_done_marker)
        self.assertIs(main_flow_kwargs["message_id_factory"], message_id_factory)
        self.assertIs(main_flow_kwargs["function_item_id_factory"], function_item_id_factory)

    def test_invoke_stream_main_flow_with_error_boundary_allows_short_circuit(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort=None,
            reasoning_summary=None,
        )
        main_flow_calls = 0
        boundary_calls = 0

        def run_main_flow_fn(**_kwargs: Any) -> None:
            nonlocal main_flow_calls
            main_flow_calls += 1

        def run_error_boundary_fn(**_kwargs: Any) -> None:
            nonlocal boundary_calls
            boundary_calls += 1

        invoke_stream_main_flow_with_error_boundary(
            payload={},
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
            response_obj=lambda **kwargs: kwargs,  # type: ignore[arg-type]
            send_done_marker=lambda: None,
            message_id_factory=lambda: "msg_2",
            function_item_id_factory=lambda: "fc_2",
            debug_logger=lambda *_parts: None,
            print_traceback=lambda: None,
            run_main_flow_fn=run_main_flow_fn,
            run_error_boundary_fn=run_error_boundary_fn,
        )

        self.assertEqual(boundary_calls, 1)
        self.assertEqual(main_flow_calls, 0)


if __name__ == "__main__":
    unittest.main()
