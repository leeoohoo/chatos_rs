#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.input_prep import prepare_stream_input_items  # noqa: E402


class GatewayStreamInputPrepTest(unittest.TestCase):
    def test_prepare_stream_input_items_success(self) -> None:
        payload = {
            "instructions": "请总结",
            "input": "hello world",
        }

        items = prepare_stream_input_items(
            payload,
            provided_tool_outputs={},
        )

        self.assertEqual(len(items), 2)
        self.assertEqual(items[0]["type"], "text")
        self.assertEqual(items[0]["text"], "请总结")
        self.assertEqual(items[1]["type"], "text")
        self.assertEqual(items[1]["text"], "hello world")

    def test_prepare_stream_input_items_with_tool_outputs(self) -> None:
        payload = {
            "input": "问题",
        }
        items = prepare_stream_input_items(
            payload,
            provided_tool_outputs={
                "call_1": [
                    {
                        "type": "inputText",
                        "text": "tool result",
                    }
                ]
            },
        )
        self.assertGreaterEqual(len(items), 2)
        self.assertEqual(items[0]["text"], "问题")
        self.assertIn("call_id=call_1", items[-1]["text"])

    def test_prepare_stream_input_items_empty_rejected(self) -> None:
        with self.assertRaises(ValueError):
            prepare_stream_input_items(
                {},
                provided_tool_outputs={},
            )


if __name__ == "__main__":
    unittest.main()
