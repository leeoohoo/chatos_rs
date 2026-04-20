#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.request_parser import parse_stream_request_context  # noqa: E402


class GatewayStreamRequestParserTest(unittest.TestCase):
    def test_parse_stream_request_context_success(self) -> None:
        payload = {
            "model": "codex-1",
            "previous_response_id": "resp_prev",
            "tools": [{"type": "function", "name": "fn1"}],
            "reasoning": {"effort": "high", "summary": "concise"},
        }
        ctx = parse_stream_request_context(payload)

        self.assertEqual(ctx.model_raw, "codex-1")
        self.assertEqual(ctx.model_name, "codex-1")
        self.assertEqual(ctx.previous_response_id, "resp_prev")
        self.assertEqual(len(ctx.response_tools), 1)
        self.assertEqual(ctx.reasoning_effort, "high")
        self.assertEqual(ctx.reasoning_summary, "concise")

    def test_parse_stream_request_context_defaults(self) -> None:
        payload = {
            "model": 123,
            "previous_response_id": 456,
            "tools": "not-a-list",
            "reasoning": "invalid",
        }
        ctx = parse_stream_request_context(payload)

        self.assertEqual(ctx.model_raw, 123)
        self.assertEqual(ctx.model_name, "codex-default")
        self.assertIsNone(ctx.previous_response_id)
        self.assertEqual(ctx.response_tools, [])
        self.assertIsNone(ctx.reasoning_effort)
        self.assertIsNone(ctx.reasoning_summary)


if __name__ == "__main__":
    unittest.main()
