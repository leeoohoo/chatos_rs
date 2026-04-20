#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.bootstrap_factories import StreamBootstrapFactories  # noqa: E402
from gateway_stream.bootstrap_invocation import invoke_stream_bootstrap_setup  # noqa: E402


class GatewayStreamBootstrapInvocationTest(unittest.TestCase):
    def test_invoke_stream_bootstrap_setup_wires_factories_and_marker(self) -> None:
        response_id_factory = lambda: "resp_1"
        created_at_factory = lambda: 111
        built_factories = StreamBootstrapFactories(
            response_id_factory=response_id_factory,
            created_at_factory=created_at_factory,
        )

        factory_builder_calls: list[dict[str, Any]] = []
        marker_factory_calls: list[dict[str, Any]] = []
        setup_calls: list[dict[str, Any]] = []
        close_values: list[bool] = []
        marker_invocations = 0

        id_factory = lambda prefix: f"{prefix}_x"
        time_factory = lambda: 123.0
        expected_bootstrap = object()

        def bootstrap_factories_builder(**kwargs: Any) -> StreamBootstrapFactories:
            factory_builder_calls.append(kwargs)
            return built_factories

        def close_connection_marker_factory(**kwargs: Any):
            marker_factory_calls.append(kwargs)

            def marker() -> None:
                nonlocal marker_invocations
                marker_invocations += 1
                kwargs["set_close_connection"](True)

            return marker

        def stream_bootstrap_setup_fn(**kwargs: Any) -> Any:
            setup_calls.append(kwargs)
            return expected_bootstrap

        def set_close_connection(value: bool) -> None:
            close_values.append(value)

        result = invoke_stream_bootstrap_setup(
            payload={"input": "hello"},
            request_cwd="/tmp/demo",
            default_cwd="/tmp/default",
            function_tools=[{"type": "function", "name": "fn1"}],
            provided_tool_outputs={"call_1": [{"type": "output_text", "text": "ok"}]},
            send_sse=lambda _event: None,
            reasoning_logger=lambda *_parts: None,
            write=lambda _chunk: 1,
            flush=lambda: None,
            set_close_connection=set_close_connection,
            id_factory=id_factory,
            time_factory=time_factory,
            close_connection_marker_factory=close_connection_marker_factory,
            bootstrap_factories_builder=bootstrap_factories_builder,
            stream_bootstrap_setup_fn=stream_bootstrap_setup_fn,
        )

        self.assertIs(result, expected_bootstrap)
        self.assertEqual(len(factory_builder_calls), 1)
        self.assertIs(factory_builder_calls[0]["id_factory"], id_factory)
        self.assertIs(factory_builder_calls[0]["time_factory"], time_factory)

        self.assertEqual(len(marker_factory_calls), 1)
        self.assertIs(marker_factory_calls[0]["set_close_connection"], set_close_connection)

        self.assertEqual(len(setup_calls), 1)
        setup_kwargs = setup_calls[0]
        self.assertEqual(setup_kwargs["payload"], {"input": "hello"})
        self.assertEqual(setup_kwargs["request_cwd"], "/tmp/demo")
        self.assertEqual(setup_kwargs["default_cwd"], "/tmp/default")
        self.assertEqual(setup_kwargs["function_tools"][0]["name"], "fn1")
        self.assertEqual(setup_kwargs["provided_tool_outputs"]["call_1"][0]["text"], "ok")
        self.assertIs(setup_kwargs["response_id_factory"], response_id_factory)
        self.assertIs(setup_kwargs["created_at_factory"], created_at_factory)

        setup_kwargs["on_close_connection"]()
        self.assertEqual(marker_invocations, 1)
        self.assertEqual(close_values, [True])


if __name__ == "__main__":
    unittest.main()
