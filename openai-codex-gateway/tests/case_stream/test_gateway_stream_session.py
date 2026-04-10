#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.session import create_stream_session, log_stream_start  # noqa: E402


class GatewayStreamSessionTest(unittest.TestCase):
    def test_create_stream_session_event_and_done(self) -> None:
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

        session = create_stream_session(
            response_id="resp_1",
            send_sse=send_sse,
            reasoning_logger=reasoning_logger,
            write=write,
            flush=flush,
            on_close_connection=on_close_connection,
        )

        session.send_event({"type": "response.output_text.delta", "delta": "h"})
        self.assertEqual(len(sent_events), 1)
        self.assertEqual(sent_events[0]["type"], "response.output_text.delta")
        self.assertEqual(sent_events[0]["sequence_number"], 0)

        session.send_done_marker()
        self.assertEqual(written, [b"event: done\n", b"data: [DONE]\n\n"])
        self.assertEqual(flush_count, 1)
        self.assertEqual(close_count, 1)
        self.assertTrue(any(parts and parts[0] == "stream.done" for parts in logged_parts))

    def test_log_stream_start(self) -> None:
        logged_parts: list[tuple[Any, ...]] = []

        def reasoning_logger(*parts: Any) -> None:
            logged_parts.append(parts)

        log_stream_start(
            response_id="resp_2",
            reasoning_effort=None,
            reasoning_summary="concise",
            request_cwd=None,
            default_cwd="/repo",
            function_tools_count=2,
            provided_tool_outputs_count=1,
            reasoning_logger=reasoning_logger,
        )

        self.assertEqual(len(logged_parts), 1)
        parts = logged_parts[0]
        self.assertEqual(parts[0], "stream.start")
        self.assertIn("response_id=resp_2", parts)
        self.assertIn("effort=none", parts)
        self.assertIn("summary=concise", parts)
        self.assertIn("cwd=/repo", parts)
        self.assertIn("function_tools=2", parts)
        self.assertIn("tool_outputs=1", parts)


if __name__ == "__main__":
    unittest.main()
