#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.invocation_dependencies import (  # noqa: E402
    build_stream_invocation_dependencies,
)
from gateway_stream.main_flow_factories import StreamMainFlowFactories  # noqa: E402


class GatewayStreamInvocationDependenciesTest(unittest.TestCase):
    def test_build_stream_invocation_dependencies_wires_components(self) -> None:
        stream_bootstrap = object()
        bootstrap_bindings = object()
        main_flow_bindings = object()
        print_traceback = lambda: None
        id_factory = lambda prefix: f"{prefix}_x"

        factories_calls: list[dict[str, Any]] = []
        bootstrap_binding_calls: list[object] = []
        main_flow_binding_calls: list[dict[str, Any]] = []
        traceback_builder_calls = 0

        built_factories = StreamMainFlowFactories(
            message_id_factory=lambda: "msg_1",
            function_item_id_factory=lambda: "fc_1",
        )

        def main_flow_factories_builder(**kwargs: Any) -> StreamMainFlowFactories:
            factories_calls.append(kwargs)
            return built_factories

        def bootstrap_bindings_builder(arg: object) -> object:
            bootstrap_binding_calls.append(arg)
            return bootstrap_bindings

        def main_flow_bindings_builder(**kwargs: Any) -> object:
            main_flow_binding_calls.append(kwargs)
            return main_flow_bindings

        def traceback_printer_builder():
            nonlocal traceback_builder_calls
            traceback_builder_calls += 1
            return print_traceback

        dependencies = build_stream_invocation_dependencies(
            stream_bootstrap=stream_bootstrap,  # type: ignore[arg-type]
            id_factory=id_factory,
            bootstrap_bindings_builder=bootstrap_bindings_builder,  # type: ignore[arg-type]
            main_flow_factories_builder=main_flow_factories_builder,
            main_flow_bindings_builder=main_flow_bindings_builder,  # type: ignore[arg-type]
            traceback_printer_builder=traceback_printer_builder,
        )

        self.assertEqual(len(factories_calls), 1)
        self.assertIs(factories_calls[0]["id_factory"], id_factory)

        self.assertEqual(bootstrap_binding_calls, [stream_bootstrap])

        self.assertEqual(len(main_flow_binding_calls), 1)
        self.assertIs(
            main_flow_binding_calls[0]["bootstrap_bindings"],
            bootstrap_bindings,
        )
        self.assertIs(
            main_flow_binding_calls[0]["main_flow_factories"],
            built_factories,
        )

        self.assertEqual(traceback_builder_calls, 1)
        self.assertIs(dependencies.main_flow_bindings, main_flow_bindings)
        self.assertIs(dependencies.print_traceback, print_traceback)


if __name__ == "__main__":
    unittest.main()
