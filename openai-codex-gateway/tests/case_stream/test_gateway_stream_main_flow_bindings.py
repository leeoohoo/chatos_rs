#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.bootstrap_bindings import StreamBootstrapBindings  # noqa: E402
from gateway_stream.main_flow_bindings import bind_stream_main_flow_invocation  # noqa: E402
from gateway_stream.main_flow_factories import StreamMainFlowFactories  # noqa: E402
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402


class GatewayStreamMainFlowBindingsTest(unittest.TestCase):
    def test_bind_stream_main_flow_invocation(self) -> None:
        stream_context = StreamRequestContext(
            model_raw="codex-1",
            model_name="codex-1",
            previous_response_id="resp_prev",
            response_tools=[],
            reasoning_effort="medium",
            reasoning_summary="auto",
        )
        send_event = lambda _event: None
        send_done_marker = lambda: None
        response_obj = lambda **kwargs: kwargs
        message_id_factory = lambda: "msg_1"
        function_item_id_factory = lambda: "fc_1"

        bootstrap_bindings = StreamBootstrapBindings(
            stream_context=stream_context,
            response_id="resp_1",
            send_stream_event=send_event,
            send_done_marker=send_done_marker,
            response_obj=response_obj,  # type: ignore[arg-type]
        )
        main_flow_factories = StreamMainFlowFactories(
            message_id_factory=message_id_factory,
            function_item_id_factory=function_item_id_factory,
        )

        bindings = bind_stream_main_flow_invocation(
            bootstrap_bindings=bootstrap_bindings,
            main_flow_factories=main_flow_factories,
        )

        self.assertIs(bindings.stream_context, stream_context)
        self.assertEqual(bindings.response_id, "resp_1")
        self.assertIs(bindings.send_event, send_event)
        self.assertIs(bindings.send_done_marker, send_done_marker)
        self.assertIs(bindings.response_obj, response_obj)
        self.assertIs(bindings.message_id_factory, message_id_factory)
        self.assertIs(bindings.function_item_id_factory, function_item_id_factory)


if __name__ == "__main__":
    unittest.main()
