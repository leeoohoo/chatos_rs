#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_request.parser import parse_responses_request  # noqa: E402


class GatewayRequestParserTest(unittest.TestCase):
    def test_parse_responses_request_success(self) -> None:
        payload = {
            "cwd": "/tmp/workspace",
            "stream": True,
            "reasoning": {
                "effort": "high",
                "summary": "concise",
            },
            "tools": [
                {
                    "type": "function",
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"},
                        },
                    },
                },
                {
                    "type": "mcp",
                    "server_label": "workspace",
                    "server_url": "http://127.0.0.1:9000/mcp",
                },
            ],
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "sunny",
                }
            ],
        }

        ctx = parse_responses_request(
            payload,
            authorization_header="Bearer sk-test",
        )

        self.assertEqual(ctx.request_cwd, "/tmp/workspace")
        self.assertTrue(ctx.stream)
        self.assertEqual(ctx.api_key, "sk-test")
        self.assertEqual(ctx.requested_tools_count, 2)
        self.assertEqual(ctx.reasoning_effort, "high")
        self.assertEqual(ctx.reasoning_summary, "concise")
        self.assertEqual(len(ctx.function_tools), 1)
        self.assertEqual(ctx.function_tools[0]["name"], "get_weather")
        self.assertIn("workspace", (ctx.request_config_overrides or {}).get("mcp_servers", {}))
        self.assertIn("call_1", ctx.provided_tool_outputs)
        self.assertEqual(
            ctx.provided_tool_outputs["call_1"][0]["text"],
            "sunny",
        )

    def test_parse_responses_request_defaults_and_non_bearer(self) -> None:
        payload = {
            "tools": "not-a-list",
        }

        with self.assertRaises(ValueError):
            parse_responses_request(
                payload,
                authorization_header="Token abc",
            )

    def test_parse_responses_request_without_tools(self) -> None:
        payload = {
            "input": "hello",
            "reasoning": "medium",
        }

        ctx = parse_responses_request(
            payload,
            authorization_header=None,
        )

        self.assertFalse(ctx.stream)
        self.assertIsNone(ctx.api_key)
        self.assertEqual(ctx.requested_tools_count, 0)
        self.assertEqual(ctx.reasoning_effort, "medium")
        self.assertIsNone(ctx.reasoning_summary)
        self.assertEqual(ctx.function_tools, [])
        self.assertEqual(ctx.provided_tool_outputs, {})
        self.assertEqual((ctx.request_config_overrides or {}).get("mcp_servers"), {})


if __name__ == "__main__":
    unittest.main()
