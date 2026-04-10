from __future__ import annotations

from typing import Any, Callable

from gateway_stream.lifecycle import persist_response_thread_mapping
from gateway_stream.request_parser import StreamRequestContext
from gateway_stream.turn_runner import StreamTurnRunnerBridge, run_stream_turn
from gateway_base.types import TurnResult


def run_and_persist_stream_turn(
    *,
    bridge: StreamTurnRunnerBridge,
    store: Any,
    response_id: str,
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
    result = run_stream_turn(
        bridge=bridge,
        input_items=input_items,
        stream_context=stream_context,
        api_key=api_key,
        request_cwd=request_cwd,
        request_config_overrides=request_config_overrides,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        on_delta=on_delta,
        on_reasoning_delta=on_reasoning_delta,
    )
    persist_response_thread_mapping(
        store=store,
        response_id=response_id,
        result=result,
    )
    return result
