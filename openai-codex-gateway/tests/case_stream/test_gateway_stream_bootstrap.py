#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.bootstrap import setup_stream_bootstrap  # noqa: E402


class GatewayStreamBootstrapTest(unittest.TestCase):
    def test_setup_stream_bootstrap_basic(self) -> None:
        sent_events: list[dict[str, Any]] = []
        logged_parts: list[tuple[Any, ...]] = []
        written: list[bytes] = []
        flush_count = 0
        close_count = 0

        def send_sse(event: dict[str, Any]) -> None:
            sent_events.append(dict(event))

        def reasoning_logger(*parts: Any) -> None:
            logged_parts.append(parts)

        def write(data: bytes) -> None:
            written.append(data)

        def flush() -> None:
            nonlocal flush_count
            flush_count += 1

        def on_close_connection() -> None:
            nonlocal close_count
            close_count += 1

        payload = {
            "model": "codex-mini",
            "previous_response_id": "resp_prev",
            "tools": [{"type": "function", "name": "fn1"}],
            "reasoning": {"effort": "high", "summary": "concise"},
        }
        bootstrap = setup_stream_bootstrap(
            payload=payload,
            request_cwd=None,
            default_cwd="/repo",
            function_tools=[{"name": "a"}, {"name": "b"}],
            provided_tool_outputs={"call_1": [{"type": "inputText", "text": "ok"}]},
            response_id_factory=lambda: "resp_1",
            created_at_factory=lambda: 123,
            send_sse=send_sse,
            reasoning_logger=reasoning_logger,
            write=write,
            flush=flush,
            on_close_connection=on_close_connection,
        )

        self.assertEqual(bootstrap.response_id, "resp_1")
        self.assertEqual(bootstrap.stream_context.model_name, "codex-mini")
        self.assertEqual(bootstrap.stream_context.previous_response_id, "resp_prev")

        body = bootstrap.response_obj(status="in_progress", output=[])
        self.assertEqual(body["id"], "resp_1")
        self.assertEqual(body["created_at"], 123)
        self.assertEqual(body["model"], "codex-mini")
        self.assertEqual(body["tools"], [{"type": "function", "name": "fn1"}])

        self.assertEqual(len(logged_parts), 1)
        parts = logged_parts[0]
        self.assertEqual(parts[0], "stream.start")
        self.assertIn("response_id=resp_1", parts)
        self.assertIn("effort=high", parts)
        self.assertIn("summary=concise", parts)
        self.assertIn("cwd=/repo", parts)
        self.assertIn("function_tools=2", parts)
        self.assertIn("tool_outputs=1", parts)

        bootstrap.send_event({"type": "response.output_text.delta", "delta": "d"})
        self.assertEqual(len(sent_events), 1)
        self.assertEqual(sent_events[0]["sequence_number"], 0)

        bootstrap.send_done_marker()
        self.assertEqual(written, [b"event: done\n", b"data: [DONE]\n\n"])
        self.assertEqual(flush_count, 1)
        self.assertEqual(close_count, 1)

    def test_setup_stream_bootstrap_defaults(self) -> None:
        logged_parts: list[tuple[Any, ...]] = []

        def reasoning_logger(*parts: Any) -> None:
            logged_parts.append(parts)

        bootstrap = setup_stream_bootstrap(
            payload={"model": 123},
            request_cwd=None,
            default_cwd=None,
            function_tools=[],
            provided_tool_outputs={},
            response_id_factory=lambda: "resp_2",
            created_at_factory=lambda: 456,
            send_sse=lambda _event: None,
            reasoning_logger=reasoning_logger,
            write=lambda _data: None,
            flush=lambda: None,
            on_close_connection=lambda: None,
        )

        self.assertEqual(bootstrap.stream_context.model_name, "codex-default")
        body = bootstrap.response_obj(status="in_progress", output=[])
        self.assertEqual(body["model"], "codex-default")
        self.assertEqual(body["tools"], [])

        self.assertEqual(len(logged_parts), 1)
        parts = logged_parts[0]
        self.assertIn("effort=none", parts)
        self.assertIn("summary=none", parts)
        self.assertIn("cwd=default", parts)
        self.assertIn("function_tools=0", parts)
        self.assertIn("tool_outputs=0", parts)


if __name__ == "__main__":
    unittest.main()
