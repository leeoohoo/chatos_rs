from __future__ import annotations

from typing import Any, Callable

from gateway_runtime.turn_event_processing import process_turn_notification
from gateway_runtime.turn_state import TurnRuntimeState


def drive_turn_notifications(
    *,
    client: Any,
    thread_id: str,
    turn_id: str,
    state: TurnRuntimeState,
    allowed_function_tool_names: set[str],
    allowed_mcp_server_labels: set[str],
    on_delta: Callable[[str], None] | None,
    on_reasoning_delta: Callable[[str], None] | None,
    reasoning_effort: str | None,
    reasoning_summary: str | None,
    process_notification: Callable[..., bool] = process_turn_notification,
) -> None:
    while True:
        event = client.next_notification()
        if (state.missing_tool_output_detected or state.disallowed_tool_error) and not state.interrupt_sent:
            try:
                client.turn_interrupt(thread_id, turn_id)
            except Exception:
                pass
            state.interrupt_sent = True

        if process_notification(
            event=event,
            turn_id=turn_id,
            state=state,
            allowed_function_tool_names=allowed_function_tool_names,
            allowed_mcp_server_labels=allowed_mcp_server_labels,
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
            reasoning_effort=reasoning_effort,
            reasoning_summary=reasoning_summary,
        ):
            break
