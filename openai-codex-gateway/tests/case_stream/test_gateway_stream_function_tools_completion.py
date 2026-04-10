#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.function_tools_completion import (  # noqa: E402
    complete_function_tools_stream,
)
from gateway_base.types import ToolCallRecord, TurnResult  # noqa: E402


class GatewayStreamFunctionToolsCompletionTest(unittest.TestCase):
    def test_complete_function_tools_stream(self) -> None:
        result = TurnResult(
            thread_id="thread_1",
            turn_id="turn_1",
            output_text="hello",
            reasoning_text="reason",
            status="completed",
            usage=None,
            error=None,
            tool_calls=[
                ToolCallRecord(call_id="call_1", name="resolved_tool", arguments={}),
                ToolCallRecord(call_id="call_2", name="pending_tool", arguments={"a": 1}),
            ],
        )
        emit_calls: list[dict[str, Any]] = []
        done_calls = 0

        def emit_result(**kwargs: Any) -> None:
            emit_calls.append(kwargs)

        def send_done_marker() -> None:
            nonlocal done_calls
            done_calls += 1

        complete_function_tools_stream(
            send_event=lambda _event: None,
            response_obj=lambda **kwargs: kwargs,
            result=result,
            provided_tool_outputs={"call_1": [{"type": "inputText", "text": "ok"}]},
            previous_response_id="resp_prev",
            tool_message_id="msg_1",
            tool_chunks=["h"],
            reasoning_chunks=["r"],
            tool_message_started=True,
            function_item_id_factory=lambda: "fc_1",
            send_done_marker=send_done_marker,
            emit_result=emit_result,
        )

        self.assertEqual(len(emit_calls), 1)
        kwargs = emit_calls[0]
        unresolved_calls = kwargs["unresolved_calls"]
        self.assertEqual(len(unresolved_calls), 1)
        self.assertEqual(unresolved_calls[0].call_id, "call_2")
        self.assertEqual(kwargs["tool_message_id"], "msg_1")
        self.assertEqual(kwargs["tool_chunks"], ["h"])
        self.assertEqual(kwargs["reasoning_chunks"], ["r"])
        self.assertEqual(done_calls, 1)


if __name__ == "__main__":
    unittest.main()
