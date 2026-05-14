#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_runtime.turn_loop import drive_turn_notifications  # noqa: E402
from gateway_runtime.turn_state import TurnRuntimeState  # noqa: E402


class FakeClient:
    def __init__(self, events: list[object], *, interrupt_raises: bool = False) -> None:
        self._events = list(events)
        self.interrupt_raises = interrupt_raises
        self.interrupt_calls: list[tuple[str, str]] = []

    def next_notification(self) -> object:
        return self._events.pop(0)

    def turn_interrupt(self, thread_id: str, turn_id: str) -> None:
        self.interrupt_calls.append((thread_id, turn_id))
        if self.interrupt_raises:
            raise RuntimeError("interrupt failed")


class GatewayTurnLoopTest(unittest.TestCase):
    def test_drives_events_until_processor_completes(self) -> None:
        state = TurnRuntimeState()
        events = [SimpleNamespace(id="evt1"), SimpleNamespace(id="evt2")]
        seen: list[object] = []

        def fake_process_notification(**kwargs: Any) -> bool:
            seen.append(kwargs["event"])
            return len(seen) == 2

        drive_turn_notifications(
            client=FakeClient(events),
            thread_id="thread_1",
            turn_id="turn_1",
            state=state,
            allowed_function_tool_names={"weather"},
            allowed_mcp_server_labels={"workspace"},
            on_delta=None,
            on_reasoning_delta=None,
            reasoning_effort="high",
            reasoning_summary="concise",
            process_notification=fake_process_notification,
        )

        self.assertEqual([event.id for event in seen], ["evt1", "evt2"])
        self.assertFalse(state.interrupt_sent)

    def test_interrupts_once_when_missing_tool_output_detected(self) -> None:
        state = TurnRuntimeState(missing_tool_output_detected=True)
        events = [SimpleNamespace(id="evt1"), SimpleNamespace(id="evt2")]
        call_count = 0

        def fake_process_notification(**kwargs: Any) -> bool:
            nonlocal call_count
            call_count += 1
            return call_count == 2

        client = FakeClient(events)
        drive_turn_notifications(
            client=client,
            thread_id="thread_1",
            turn_id="turn_1",
            state=state,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels=set(),
            on_delta=None,
            on_reasoning_delta=None,
            reasoning_effort=None,
            reasoning_summary=None,
            process_notification=fake_process_notification,
        )

        self.assertEqual(client.interrupt_calls, [("thread_1", "turn_1")])
        self.assertTrue(state.interrupt_sent)

    def test_continues_when_interrupt_call_raises(self) -> None:
        state = TurnRuntimeState(disallowed_tool_error="not allowed")

        def fake_process_notification(**kwargs: Any) -> bool:
            return True

        client = FakeClient([SimpleNamespace(id="evt1")], interrupt_raises=True)
        drive_turn_notifications(
            client=client,
            thread_id="thread_1",
            turn_id="turn_1",
            state=state,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels=set(),
            on_delta=None,
            on_reasoning_delta=None,
            reasoning_effort=None,
            reasoning_summary=None,
            process_notification=fake_process_notification,
        )

        self.assertEqual(client.interrupt_calls, [("thread_1", "turn_1")])
        self.assertTrue(state.interrupt_sent)


if __name__ == "__main__":
    unittest.main()
