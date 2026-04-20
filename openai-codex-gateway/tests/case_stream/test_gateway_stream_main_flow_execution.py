#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.invocation_dependencies import StreamInvocationDependencies  # noqa: E402
from gateway_stream.main_flow_bindings import StreamMainFlowInvocationBindings  # noqa: E402
from gateway_stream.main_flow_execution import (  # noqa: E402
    execute_prepared_stream_main_flow,
    run_stream_main_flow_with_default_orchestration,
)
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402


class GatewayStreamMainFlowExecutionTest(unittest.TestCase):
    def test_execute_prepared_stream_main_flow(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort="high",
            reasoning_summary="concise",
        )
        send_event = lambda _event: None
        send_done_marker = lambda: None
        response_obj = lambda **kwargs: kwargs
        message_id_factory = lambda: "msg_1"
        function_item_id_factory = lambda: "fc_1"
        print_traceback = lambda: None
        debug_logger = lambda *_parts: None

        main_flow_bindings = StreamMainFlowInvocationBindings(
            stream_context=stream_context,
            response_id="resp_1",
            send_event=send_event,
            send_done_marker=send_done_marker,
            response_obj=response_obj,  # type: ignore[arg-type]
            message_id_factory=message_id_factory,
            function_item_id_factory=function_item_id_factory,
        )
        invocation_dependencies = StreamInvocationDependencies(
            main_flow_bindings=main_flow_bindings,
            print_traceback=print_traceback,
        )

        invoker_calls: list[dict[str, Any]] = []

        def main_flow_invoker(**kwargs: Any) -> None:
            invoker_calls.append(kwargs)

        execute_prepared_stream_main_flow(
            payload={"input": "hello"},
            bridge=object(),
            store=object(),
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox_mode": "workspace-write"},
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            invocation_dependencies=invocation_dependencies,
            debug_logger=debug_logger,
            main_flow_invoker=main_flow_invoker,
        )

        self.assertEqual(len(invoker_calls), 1)
        kwargs = invoker_calls[0]
        self.assertEqual(kwargs["payload"], {"input": "hello"})
        self.assertEqual(kwargs["response_id"], "resp_1")
        self.assertIs(kwargs["stream_context"], stream_context)
        self.assertEqual(kwargs["api_key"], "k")
        self.assertEqual(kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(kwargs["send_event"], send_event)
        self.assertIs(kwargs["response_obj"], response_obj)
        self.assertIs(kwargs["send_done_marker"], send_done_marker)
        self.assertIs(kwargs["message_id_factory"], message_id_factory)
        self.assertIs(kwargs["function_item_id_factory"], function_item_id_factory)
        self.assertIs(kwargs["debug_logger"], debug_logger)
        self.assertIs(kwargs["print_traceback"], print_traceback)

    def test_run_stream_main_flow_with_default_orchestration(self) -> None:
        invocation_dependencies = object()
        debug_logger = lambda *_parts: None
        send_sse = lambda _event: None
        write = lambda _chunk: 1
        flush = lambda: None
        connection_target = object()

        orchestration_calls: list[dict[str, Any]] = []
        execute_calls: list[dict[str, Any]] = []

        def orchestration_preparer(**kwargs: Any):
            orchestration_calls.append(kwargs)
            return invocation_dependencies

        def prepared_executor(**kwargs: Any) -> None:
            execute_calls.append(kwargs)

        run_stream_main_flow_with_default_orchestration(
            payload={"input": "hello"},
            bridge=object(),
            store=object(),
            api_key="k",
            request_cwd="/tmp/demo",
            request_config_overrides={"sandbox_mode": "workspace-write"},
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            default_cwd="/tmp/default",
            send_sse=send_sse,
            write=write,
            flush=flush,
            connection_target=connection_target,
            debug_logger=debug_logger,
            orchestration_preparer=orchestration_preparer,
            prepared_executor=prepared_executor,
        )

        self.assertEqual(len(orchestration_calls), 1)
        prepare_kwargs = orchestration_calls[0]
        self.assertEqual(prepare_kwargs["payload"], {"input": "hello"})
        self.assertEqual(prepare_kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(prepare_kwargs["default_cwd"], "/tmp/default")
        self.assertEqual(prepare_kwargs["function_tools"][0]["name"], "fn1")
        self.assertEqual(prepare_kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(prepare_kwargs["send_sse"], send_sse)
        self.assertIs(prepare_kwargs["write"], write)
        self.assertIs(prepare_kwargs["flush"], flush)
        self.assertIs(prepare_kwargs["connection_target"], connection_target)

        self.assertEqual(len(execute_calls), 1)
        execute_kwargs = execute_calls[0]
        self.assertEqual(execute_kwargs["payload"], {"input": "hello"})
        self.assertEqual(execute_kwargs["api_key"], "k")
        self.assertEqual(execute_kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(execute_kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(execute_kwargs["invocation_dependencies"], invocation_dependencies)
        self.assertIs(execute_kwargs["debug_logger"], debug_logger)


if __name__ == "__main__":
    unittest.main()
