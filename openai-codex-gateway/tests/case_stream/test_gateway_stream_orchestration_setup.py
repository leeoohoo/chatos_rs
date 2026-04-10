#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.orchestration_setup import (  # noqa: E402
    prepare_default_stream_orchestration_dependencies,
    prepare_stream_orchestration_dependencies,
)


class GatewayStreamOrchestrationSetupTest(unittest.TestCase):
    def test_prepare_stream_orchestration_dependencies(self) -> None:
        connection_target = object()
        set_close_connection = lambda _value: None
        stream_bootstrap = object()
        invocation_dependencies = object()
        id_factory = lambda prefix: f"{prefix}_x"
        time_factory = lambda: 123.0
        send_sse = lambda _event: None
        reasoning_logger = lambda *_parts: None
        write = lambda _chunk: 1
        flush = lambda: None

        setter_calls: list[dict[str, Any]] = []
        bootstrap_calls: list[dict[str, Any]] = []
        dependencies_calls: list[dict[str, Any]] = []

        def close_connection_setter_builder(**kwargs: Any):
            setter_calls.append(kwargs)
            return set_close_connection

        def stream_bootstrap_invoker(**kwargs: Any):
            bootstrap_calls.append(kwargs)
            return stream_bootstrap

        def invocation_dependencies_builder(**kwargs: Any):
            dependencies_calls.append(kwargs)
            return invocation_dependencies

        result = prepare_stream_orchestration_dependencies(
            payload={"input": "hello"},
            request_cwd="/tmp/demo",
            default_cwd="/tmp/default",
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            send_sse=send_sse,
            reasoning_logger=reasoning_logger,
            write=write,
            flush=flush,
            connection_target=connection_target,
            id_factory=id_factory,
            time_factory=time_factory,
            close_connection_setter_builder=close_connection_setter_builder,
            stream_bootstrap_invoker=stream_bootstrap_invoker,  # type: ignore[arg-type]
            invocation_dependencies_builder=invocation_dependencies_builder,  # type: ignore[arg-type]
        )

        self.assertIs(result, invocation_dependencies)

        self.assertEqual(len(setter_calls), 1)
        self.assertIs(setter_calls[0]["target"], connection_target)

        self.assertEqual(len(bootstrap_calls), 1)
        bootstrap_kwargs = bootstrap_calls[0]
        self.assertEqual(bootstrap_kwargs["payload"], {"input": "hello"})
        self.assertEqual(bootstrap_kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(bootstrap_kwargs["default_cwd"], "/tmp/default")
        self.assertEqual(bootstrap_kwargs["function_tools"][0]["name"], "fn1")
        self.assertEqual(bootstrap_kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(bootstrap_kwargs["send_sse"], send_sse)
        self.assertIs(bootstrap_kwargs["reasoning_logger"], reasoning_logger)
        self.assertIs(bootstrap_kwargs["write"], write)
        self.assertIs(bootstrap_kwargs["flush"], flush)
        self.assertIs(bootstrap_kwargs["set_close_connection"], set_close_connection)
        self.assertIs(bootstrap_kwargs["id_factory"], id_factory)
        self.assertIs(bootstrap_kwargs["time_factory"], time_factory)

        self.assertEqual(len(dependencies_calls), 1)
        self.assertIs(dependencies_calls[0]["stream_bootstrap"], stream_bootstrap)
        self.assertIs(dependencies_calls[0]["id_factory"], id_factory)

    def test_prepare_default_stream_orchestration_dependencies(self) -> None:
        invocation_dependencies = object()
        send_sse = lambda _event: None
        write = lambda _chunk: 1
        flush = lambda: None
        reasoning_logger = lambda *_parts: None
        id_factory = lambda prefix: f"{prefix}_x"
        time_factory = lambda: 123.0
        calls: list[dict[str, Any]] = []

        def orchestration_dependencies_preparer(**kwargs: Any):
            calls.append(kwargs)
            return invocation_dependencies

        result = prepare_default_stream_orchestration_dependencies(
            payload={"input": "hello"},
            request_cwd="/tmp/demo",
            default_cwd="/tmp/default",
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            send_sse=send_sse,
            write=write,
            flush=flush,
            connection_target=object(),
            reasoning_logger=reasoning_logger,
            id_factory=id_factory,
            time_factory=time_factory,
            orchestration_dependencies_preparer=orchestration_dependencies_preparer,
        )

        self.assertIs(result, invocation_dependencies)
        self.assertEqual(len(calls), 1)
        kwargs = calls[0]
        self.assertEqual(kwargs["payload"], {"input": "hello"})
        self.assertEqual(kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(kwargs["default_cwd"], "/tmp/default")
        self.assertEqual(kwargs["function_tools"][0]["name"], "fn1")
        self.assertEqual(kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(kwargs["send_sse"], send_sse)
        self.assertIs(kwargs["write"], write)
        self.assertIs(kwargs["flush"], flush)
        self.assertIs(kwargs["reasoning_logger"], reasoning_logger)
        self.assertIs(kwargs["id_factory"], id_factory)
        self.assertIs(kwargs["time_factory"], time_factory)


if __name__ == "__main__":
    unittest.main()
