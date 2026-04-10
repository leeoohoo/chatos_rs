from __future__ import annotations

from typing import Any, Callable, Protocol

from gateway_stream.request_parser import StreamRequestContext
from gateway_base.types import TurnResult


class StreamTurnRunnerBridge(Protocol):
    def _run_turn(
        self,
        *,
        input_items: list[dict[str, Any]],
        model: str | None,
        reasoning_effort: str | None,
        reasoning_summary: str | None,
        previous_response_id: str | None,
        api_key: str | None,
        request_cwd: str | None,
        request_config_overrides: dict[str, Any] | None,
        function_tools: list[dict[str, Any]],
        provided_tool_outputs: dict[str, list[dict[str, Any]]],
        on_delta: Callable[[str], None] | None = None,
        on_reasoning_delta: Callable[[str], None] | None = None,
    ) -> TurnResult: ...


def run_stream_turn(
    *,
    bridge: StreamTurnRunnerBridge,
    input_items: list[dict[str, Any]],
    stream_context: StreamRequestContext,
    api_key: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    on_delta: Callable[[str], None] | None,
    on_reasoning_delta: Callable[[str], None] | None,
) -> TurnResult:
    model = stream_context.model_raw if isinstance(stream_context.model_raw, str) else None
    return bridge._run_turn(
        input_items=input_items,
        model=model,
        reasoning_effort=stream_context.reasoning_effort,
        reasoning_summary=stream_context.reasoning_summary,
        previous_response_id=stream_context.previous_response_id,
        api_key=api_key,
        request_cwd=request_cwd,
        request_config_overrides=request_config_overrides,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        on_delta=on_delta,
        on_reasoning_delta=on_reasoning_delta,
    )
