#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from create_response.parser import parse_create_response_context  # noqa: E402


class GatewayCreateResponseParserTest(unittest.TestCase):
    def test_parse_create_response_context_success(self) -> None:
        payload = {
            "model": "codex-1",
            "previous_response_id": "resp_prev",
            "reasoning": {"effort": "medium", "summary": "concise"},
            "tools": [{"type": "function", "name": "fn1"}],
            "input": "hello world",
        }

        ctx = parse_create_response_context(
            payload,
            provided_tool_outputs={},
        )

        self.assertEqual(ctx.model, "codex-1")
        self.assertEqual(ctx.model_name, "codex-1")
        self.assertEqual(ctx.previous_response_id, "resp_prev")
        self.assertEqual(ctx.reasoning_effort, "medium")
        self.assertEqual(ctx.reasoning_summary, "concise")
        self.assertEqual(len(ctx.response_tools), 1)
        self.assertEqual(ctx.input_items[0]["type"], "text")
        self.assertEqual(ctx.input_items[0]["text"], "hello world")

    def test_parse_create_response_context_defaults(self) -> None:
        payload = {
            "model": 123,
            "previous_response_id": "",
            "input": "hi",
        }

        ctx = parse_create_response_context(
            payload,
            provided_tool_outputs={},
        )

        self.assertEqual(ctx.model, 123)
        self.assertEqual(ctx.model_name, "codex-default")
        self.assertIsNone(ctx.previous_response_id)
        self.assertIsNone(ctx.reasoning_effort)
        self.assertIsNone(ctx.reasoning_summary)
        self.assertEqual(ctx.response_tools, [])

    def test_parse_create_response_context_rejects_empty_input(self) -> None:
        with self.assertRaises(ValueError):
            parse_create_response_context(
                {},
                provided_tool_outputs={},
            )


if __name__ == "__main__":
    unittest.main()
