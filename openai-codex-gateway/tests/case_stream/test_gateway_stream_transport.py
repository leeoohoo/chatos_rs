#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.transport import (  # noqa: E402
    StreamEventTransport,
    build_stream_error_event,
)


class GatewayStreamTransportTest(unittest.TestCase):
    def test_emit_adds_sequence_and_increments(self) -> None:
        sent_events: list[dict] = []
        logs: list[tuple] = []
        transport = StreamEventTransport(
            response_id="resp_1",
            send_sse=sent_events.append,
            reasoning_logger=lambda *args: logs.append(args),
        )

        event_0 = {"type": "response.created"}
        event_1 = {"type": "response.output_item.added"}
        transport.emit(event_0)
        transport.emit(event_1)

        self.assertEqual(event_0["sequence_number"], 0)
        self.assertEqual(event_1["sequence_number"], 1)
        self.assertEqual(transport.sequence_number, 2)
        self.assertEqual(len(sent_events), 2)
        self.assertEqual(logs, [])

    def test_emit_logs_reasoning_events(self) -> None:
        sent_events: list[dict] = []
        logs: list[tuple] = []
        transport = StreamEventTransport(
            response_id="resp_2",
            send_sse=sent_events.append,
            reasoning_logger=lambda *args: logs.append(args),
        )

        transport.emit({"type": "response.reasoning.delta", "delta": "abc"})
        transport.emit({"type": "response.reasoning.done", "text": "trace"})

        self.assertEqual(sent_events[0]["sequence_number"], 0)
        self.assertEqual(sent_events[1]["sequence_number"], 1)
        self.assertEqual(len(logs), 2)
        self.assertEqual(logs[0][0], "stream.emit")
        self.assertIn("type=response.reasoning.delta", logs[0])
        self.assertIn("chars=3", logs[0])
        self.assertIn("type=response.reasoning.done", logs[1])
        self.assertIn("chars=5", logs[1])

    def test_emit_done_marker(self) -> None:
        sent_events: list[dict] = []
        logs: list[tuple] = []
        writes: list[bytes] = []
        flushed = {"count": 0}
        transport = StreamEventTransport(
            response_id="resp_3",
            send_sse=sent_events.append,
            reasoning_logger=lambda *args: logs.append(args),
            sequence_number=7,
        )

        transport.emit_done_marker(
            write=writes.append,
            flush=lambda: flushed.__setitem__("count", flushed["count"] + 1),
        )

        self.assertEqual(writes, [b"event: done\n", b"data: [DONE]\n\n"])
        self.assertEqual(flushed["count"], 1)
        self.assertEqual(len(logs), 1)
        self.assertEqual(logs[0][0], "stream.done")
        self.assertIn("response_id=resp_3", logs[0])
        self.assertIn("sequence=7", logs[0])

    def test_build_stream_error_event(self) -> None:
        event = build_stream_error_event("boom")
        self.assertEqual(
            event,
            {
                "type": "error",
                "code": "server_error",
                "message": "boom",
                "param": None,
            },
        )


if __name__ == "__main__":
    unittest.main()
