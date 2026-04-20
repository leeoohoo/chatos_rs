#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

try:
    import server as gateway_server  # noqa: E402
except SystemExit as exc:  # pragma: no cover - environment dependency guard
    gateway_server = None
    IMPORT_ERROR = str(exc)
else:
    IMPORT_ERROR = None


@unittest.skipIf(gateway_server is None, f"gateway import unavailable: {IMPORT_ERROR}")
class GatewayTurnEventProcessingTest(unittest.TestCase):
    def _process(
        self,
        payload: object,
        state: "gateway_server.TurnRuntimeState",
        *,
        allowed_function_tool_names: set[str] | None = None,
        allowed_mcp_server_labels: set[str] | None = None,
        on_delta=None,
        on_reasoning_delta=None,
        reasoning_effort: str | None = None,
        reasoning_summary: str | None = None,
    ) -> bool:
        event = SimpleNamespace(method="notification/test", payload=payload)
        return gateway_server.CodexBridge._process_turn_notification(
            event=event,
            turn_id="turn_1",
            state=state,
            allowed_function_tool_names=allowed_function_tool_names or set(),
            allowed_mcp_server_labels=allowed_mcp_server_labels or set(),
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
            reasoning_effort=reasoning_effort,
            reasoning_summary=reasoning_summary,
        )

    def test_reasoning_delta_updates_state_and_callback(self) -> None:
        state = gateway_server.TurnRuntimeState()
        chunks: list[str] = []
        payload = gateway_server.ReasoningTextDeltaNotification(
            content_index=0,
            delta="step-by-step",
            item_id="item_1",
            thread_id="thread_1",
            turn_id="turn_1",
        )

        completed = self._process(payload, state, on_reasoning_delta=chunks.append)

        self.assertFalse(completed)
        self.assertEqual(state.reasoning_text, "step-by-step")
        self.assertEqual(state.reasoning_event_count, 1)
        self.assertEqual(chunks, ["step-by-step"])

    def test_reasoning_item_completed_fallback_is_used(self) -> None:
        state = gateway_server.TurnRuntimeState()
        chunks: list[str] = []
        payload = gateway_server.ItemCompletedNotification(
            item={
                "id": "reasoning_1",
                "content": ["analysis"],
                "summary": ["quick "],
                "type": "reasoning",
            },
            thread_id="thread_1",
            turn_id="turn_1",
        )

        completed = self._process(payload, state, on_reasoning_delta=chunks.append)

        self.assertFalse(completed)
        self.assertEqual(state.reasoning_text, "quick analysis")
        self.assertEqual(chunks, ["quick analysis"])

    def test_token_usage_event_updates_usage_snapshot(self) -> None:
        state = gateway_server.TurnRuntimeState()
        payload = gateway_server.ThreadTokenUsageUpdatedNotification(
            thread_id="thread_1",
            turn_id="turn_1",
            token_usage={
                "last": {
                    "input_tokens": 10,
                    "output_tokens": 8,
                    "total_tokens": 18,
                    "cached_input_tokens": 2,
                    "reasoning_output_tokens": 3,
                },
                "total": {
                    "input_tokens": 10,
                    "output_tokens": 8,
                    "total_tokens": 18,
                    "cached_input_tokens": 2,
                    "reasoning_output_tokens": 3,
                },
                "model_context_window": None,
            },
        )

        completed = self._process(payload, state)

        self.assertFalse(completed)
        self.assertEqual(state.reasoning_tokens, 3)
        assert state.usage is not None
        self.assertEqual(state.usage["input_tokens"], 10)
        self.assertEqual(state.usage["output_tokens_details"]["reasoning_tokens"], 3)

    def test_disallowed_dynamic_tool_sets_runtime_error(self) -> None:
        state = gateway_server.TurnRuntimeState()
        payload = gateway_server.ItemStartedNotification(
            item={
                "arguments": {},
                "contentItems": [],
                "id": "dyn_1",
                "status": "completed",
                "success": False,
                "tool": "shell_exec",
                "type": "dynamicToolCall",
            },
            thread_id="thread_1",
            turn_id="turn_1",
        )

        completed = self._process(
            payload,
            state,
            allowed_function_tool_names={"memory_reader_read_file"},
            allowed_mcp_server_labels={"workspace"},
        )

        self.assertFalse(completed)
        self.assertIsNotNone(state.disallowed_tool_error)
        self.assertIn("未在本次请求中声明的动态工具", state.disallowed_tool_error or "")

    def test_turn_completed_with_disallowed_error_forces_failed_status(self) -> None:
        state = gateway_server.TurnRuntimeState(disallowed_tool_error="disallowed")
        payload = gateway_server.TurnCompletedNotification(
            thread_id="thread_1",
            turn={
                "id": "turn_1",
                "status": "completed",
                "error": None,
                "items": [],
            },
        )

        completed = self._process(payload, state)

        self.assertTrue(completed)
        self.assertEqual(state.status, "failed")
        assert state.error is not None
        self.assertEqual(state.error["message"], "disallowed")
        self.assertEqual(
            state.error["codex_error_info"]["gateway_error"],
            "disallowed_tool_use",
        )


if __name__ == "__main__":
    unittest.main()
