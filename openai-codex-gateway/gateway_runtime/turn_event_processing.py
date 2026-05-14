from __future__ import annotations

from typing import Any, Callable

from gateway_base.logging import reasoning_log, state_log
from gateway_base.utils import to_json_compatible
from gateway_runtime.sdk_types import (
    AgentMessageDeltaNotification,
    AgentMessageThreadItem,
    ItemCompletedNotification,
    ItemStartedNotification,
    ReasoningSummaryTextDeltaNotification,
    ReasoningTextDeltaNotification,
    ReasoningThreadItem,
    ThreadTokenUsageUpdatedNotification,
    TurnCompletedNotification,
)
from gateway_runtime.tool_guard import describe_disallowed_thread_item
from gateway_runtime.turn_state import TurnRuntimeState


def process_turn_notification(
    *,
    event: Any,
    turn_id: str,
    state: TurnRuntimeState,
    allowed_function_tool_names: set[str],
    allowed_mcp_server_labels: set[str],
    on_delta: Callable[[str], None] | None,
    on_reasoning_delta: Callable[[str], None] | None,
    reasoning_effort: str | None,
    reasoning_summary: str | None,
) -> bool:
    event_method = getattr(event, "method", "unknown")
    payload = event.payload

    if (
        isinstance(payload, (ItemStartedNotification, ItemCompletedNotification))
        and payload.turn_id == turn_id
    ):
        item = payload.item.root
        tool_violation = describe_disallowed_thread_item(
            item,
            allowed_function_tool_names=allowed_function_tool_names,
            allowed_mcp_server_labels=allowed_mcp_server_labels,
        )
        if tool_violation and state.disallowed_tool_error is None:
            state.disallowed_tool_error = tool_violation
            state_log(
                "run_turn.disallowed_thread_item",
                f"method={event_method}",
                f"type={getattr(item, 'type', 'unknown')}",
                f"detail={tool_violation}",
            )
        if tool_violation:
            return False

    if isinstance(payload, AgentMessageDeltaNotification) and payload.turn_id == turn_id:
        state.output_text += payload.delta
        if on_delta:
            on_delta(payload.delta)
        return False

    if isinstance(payload, ReasoningTextDeltaNotification) and payload.turn_id == turn_id:
        state.reasoning_event_count += 1
        reasoning_log(
            "sdk.event",
            f"method={event_method}",
            "type=reasoning_text_delta",
            f"turn_id={payload.turn_id}",
            f"chars={len(payload.delta)}",
        )
        state.reasoning_text += payload.delta
        if on_reasoning_delta:
            on_reasoning_delta(payload.delta)
        return False

    if (
        isinstance(payload, ReasoningSummaryTextDeltaNotification)
        and payload.turn_id == turn_id
    ):
        state.reasoning_event_count += 1
        reasoning_log(
            "sdk.event",
            f"method={event_method}",
            "type=reasoning_summary_delta",
            f"turn_id={payload.turn_id}",
            f"chars={len(payload.delta)}",
        )
        state.reasoning_text += payload.delta
        if on_reasoning_delta:
            on_reasoning_delta(payload.delta)
        return False

    if isinstance(payload, ItemCompletedNotification) and payload.turn_id == turn_id:
        item = payload.item.root
        if isinstance(item, AgentMessageThreadItem) and item.text:
            state.output_text = item.text
        if isinstance(item, ReasoningThreadItem):
            summary_text = "".join(item.summary or [])
            content_text = "".join(item.content or [])
            fallback_text = (summary_text + content_text).strip()
            reasoning_log(
                "sdk.event",
                f"method={event_method}",
                "type=reasoning_item_completed",
                f"summary_chars={len(summary_text)}",
                f"content_chars={len(content_text)}",
                f"used_fallback={'yes' if not state.reasoning_text and bool(fallback_text) else 'no'}",
            )
            if not state.reasoning_text:
                state.reasoning_text = fallback_text
                if state.reasoning_text and on_reasoning_delta:
                    on_reasoning_delta(state.reasoning_text)
        return False

    if (
        isinstance(payload, ThreadTokenUsageUpdatedNotification)
        and payload.turn_id == turn_id
    ):
        state.reasoning_tokens = payload.token_usage.last.reasoning_output_tokens
        state.usage = {
            "input_tokens": payload.token_usage.last.input_tokens,
            "output_tokens": payload.token_usage.last.output_tokens,
            "total_tokens": payload.token_usage.last.total_tokens,
            "input_tokens_details": {
                "cached_tokens": payload.token_usage.last.cached_input_tokens,
            },
            "output_tokens_details": {
                "reasoning_tokens": payload.token_usage.last.reasoning_output_tokens,
            },
        }
        reasoning_log(
            "sdk.event",
            f"method={event_method}",
            "type=token_usage",
            f"input_tokens={payload.token_usage.last.input_tokens}",
            f"output_tokens={payload.token_usage.last.output_tokens}",
            f"reasoning_tokens={state.reasoning_tokens}",
        )
        return False

    if isinstance(payload, TurnCompletedNotification) and payload.turn.id == turn_id:
        state.status = payload.turn.status.value
        if state.disallowed_tool_error:
            state.status = "failed"
        reasoning_log(
            "turn.completed",
            f"turn_id={turn_id}",
            f"status={state.status}",
            f"reasoning_chars={len(state.reasoning_text)}",
            f"reasoning_tokens={state.reasoning_tokens}",
            f"reasoning_events={state.reasoning_event_count}",
        )
        if (reasoning_effort or reasoning_summary) and not state.reasoning_text:
            reasoning_log(
                "turn.reasoning_missing",
                f"turn_id={turn_id}",
                f"reasoning_tokens={state.reasoning_tokens}",
                f"reasoning_requested_effort={reasoning_effort or 'none'}",
                f"reasoning_requested_summary={reasoning_summary or 'none'}",
            )
        if payload.turn.error is not None:
            state.error = {
                "message": payload.turn.error.message,
                "codex_error_info": to_json_compatible(payload.turn.error.codex_error_info),
            }
        if state.disallowed_tool_error:
            state.error = {
                "message": state.disallowed_tool_error,
                "codex_error_info": {
                    "gateway_error": "disallowed_tool_use",
                },
            }
        return True

    return False
