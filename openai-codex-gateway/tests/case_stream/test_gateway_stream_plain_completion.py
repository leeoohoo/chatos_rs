#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.plain_completion import complete_plain_message_stream  # noqa: E402
from gateway_base.types import TurnResult  # noqa: E402


class GatewayStreamPlainCompletionTest(unittest.TestCase):
    def test_complete_plain_message_stream(self) -> None:
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="hello",
            reasoning_text="reason",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[],
        )
        emit_calls: list[dict[str, Any]] = []
        done_calls = 0

        def emit_result(**kwargs: Any) -> None:
            emit_calls.append(kwargs)

        def send_done_marker() -> None:
            nonlocal done_calls
            done_calls += 1

        complete_plain_message_stream(
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            result=result,
            previous_response_id="resp_prev",
            message_id="msg_1",
            chunks=["h"],
            reasoning_chunks=["r"],
            send_done_marker=send_done_marker,
            emit_result=emit_result,
        )

        self.assertEqual(len(emit_calls), 1)
        kwargs = emit_calls[0]
        self.assertEqual(kwargs["message_id"], "msg_1")
        self.assertEqual(kwargs["chunks"], ["h"])
        self.assertEqual(kwargs["reasoning_chunks"], ["r"])
        self.assertEqual(kwargs["previous_response_id"], "resp_prev")
        self.assertIs(kwargs["result"], result)
        self.assertEqual(done_calls, 1)


if __name__ == "__main__":
    unittest.main()
