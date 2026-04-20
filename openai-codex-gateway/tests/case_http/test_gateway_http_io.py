#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_http.io import encode_json_body, serialize_sse_event  # noqa: E402


class GatewayHttpIoTest(unittest.TestCase):
    def test_encode_json_body_preserves_utf8_text(self) -> None:
        encoded = encode_json_body({"message": "中文"})
        self.assertIn("中文", encoded.decode("utf-8"))
        self.assertNotIn("\\u4e2d\\u6587", encoded.decode("utf-8"))

    def test_serialize_sse_event_with_type(self) -> None:
        frame = serialize_sse_event({"type": "response.created", "id": "resp_1"})
        text = frame.decode("utf-8")
        self.assertTrue(text.startswith("event: response.created\n"))
        self.assertIn('data: {"type": "response.created", "id": "resp_1"}\n\n', text)

    def test_serialize_sse_event_without_type(self) -> None:
        frame = serialize_sse_event({"id": "resp_2"})
        text = frame.decode("utf-8")
        self.assertFalse(text.startswith("event:"))
        self.assertEqual(text, 'data: {"id": "resp_2"}\n\n')


if __name__ == "__main__":
    unittest.main()
