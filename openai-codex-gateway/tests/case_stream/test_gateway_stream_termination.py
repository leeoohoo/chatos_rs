#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.termination import emit_stream_error_and_done  # noqa: E402


class GatewayStreamTerminationTest(unittest.TestCase):
    def test_emit_stream_error_and_done(self) -> None:
        debug_calls: list[tuple[Any, ...]] = []
        events: list[dict[str, Any]] = []
        traceback_calls = 0
        done_calls = 0

        def debug_logger(*parts: Any) -> None:
            debug_calls.append(parts)

        def send_event(event: dict[str, Any]) -> None:
            events.append(event)

        def send_done_marker() -> None:
            nonlocal done_calls
            done_calls += 1

        def print_traceback() -> None:
            nonlocal traceback_calls
            traceback_calls += 1

        emit_stream_error_and_done(
            exc=RuntimeError("boom"),
            send_event=send_event,
            send_done_marker=send_done_marker,
            debug_logger=debug_logger,
            print_traceback=print_traceback,
        )

        self.assertEqual(len(debug_calls), 1)
        self.assertEqual(debug_calls[0][0], "http.stream.error")
        self.assertEqual(traceback_calls, 1)
        self.assertEqual(done_calls, 1)
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["type"], "error")
        self.assertEqual(events[0]["code"], "server_error")
        self.assertEqual(events[0]["message"], "boom")


if __name__ == "__main__":
    unittest.main()
