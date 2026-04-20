#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.envelope import build_stream_envelope_setup  # noqa: E402


class GatewayStreamEnvelopeTest(unittest.TestCase):
    def test_build_stream_envelope_setup_basic(self) -> None:
        setup = build_stream_envelope_setup(
            response_id="resp_1",
            created_at=123,
            model_name="codex-mini",
            response_tools=[{"type": "function"}],
        )

        self.assertEqual(setup.response_id, "resp_1")
        self.assertEqual(setup.created_at, 123)

        body = setup.response_obj(
            status="in_progress",
            output=[],
        )
        self.assertEqual(body["id"], "resp_1")
        self.assertEqual(body["created_at"], 123)
        self.assertEqual(body["model"], "codex-mini")
        self.assertEqual(body["tools"], [{"type": "function"}])
        self.assertEqual(body["status"], "in_progress")
        self.assertEqual(body["output"], [])

    def test_build_stream_envelope_setup_optional_fields(self) -> None:
        setup = build_stream_envelope_setup(
            response_id="resp_2",
            created_at=456,
            model_name="codex-pro",
            response_tools=[],
        )

        body = setup.response_obj(
            status="completed",
            output=[{"id": "item_1"}],
            usage={"total_tokens": 10},
            error={"message": "boom"},
            reasoning="trace",
            previous_response_id="resp_prev",
            metadata={"thread_id": "thread_1"},
        )

        self.assertEqual(body["usage"]["total_tokens"], 10)
        self.assertEqual(body["error"]["message"], "boom")
        self.assertEqual(body["reasoning"], "trace")
        self.assertEqual(body["previous_response_id"], "resp_prev")
        self.assertEqual(body["metadata"]["thread_id"], "thread_1")


if __name__ == "__main__":
    unittest.main()
