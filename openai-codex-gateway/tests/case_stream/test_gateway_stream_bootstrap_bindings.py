#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.bootstrap import StreamBootstrap  # noqa: E402
from gateway_stream.bootstrap_bindings import bind_stream_bootstrap  # noqa: E402
from gateway_stream.request_parser import StreamRequestContext  # noqa: E402


class GatewayStreamBootstrapBindingsTest(unittest.TestCase):
    def test_bind_stream_bootstrap(self) -> None:
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

        stream_bootstrap = StreamBootstrap(
            stream_context=stream_context,
            response_id="resp_1",
            response_obj=response_obj,
            send_event=send_event,
            send_done_marker=send_done_marker,
        )

        bindings = bind_stream_bootstrap(stream_bootstrap)

        self.assertIs(bindings.stream_context, stream_context)
        self.assertEqual(bindings.response_id, "resp_1")
        self.assertIs(bindings.send_stream_event, send_event)
        self.assertIs(bindings.send_done_marker, send_done_marker)
        self.assertIs(bindings.response_obj, response_obj)


if __name__ == "__main__":
    unittest.main()
