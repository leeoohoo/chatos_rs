from __future__ import annotations

from typing import Any, Callable

from gateway_stream.function_tools_completion import complete_function_tools_stream
from gateway_stream.message_callbacks import PlainMessageStreamCallbacks
from gateway_stream.plain_completion import complete_plain_message_stream
from gateway_stream.request_parser import StreamRequestContext
from gateway_stream.tool_callbacks import FunctionToolStreamCallbacks
from gateway_stream.turn_execution import run_and_persist_stream_turn
from gateway_stream.turn_runner import StreamTurnRunnerBridge
from gateway_base.types import TurnResult


def execute_function_tools_branch(
    callbacks: FunctionToolStreamCallbacks,
    tool_message_id: str,
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
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    function_item_id_factory: Callable[[], str],
    send_done_marker: Callable[[], None],
    run_turn: Callable[..., TurnResult] = run_and_persist_stream_turn,
    complete_stream: Callable[..., None] = complete_function_tools_stream,
) -> None:
    result = run_turn(
        bridge=bridge,
        store=store,
        response_id=response_id,
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
    complete_stream(
        send_event=send_event,
        response_obj=response_obj,
        result=result,
        provided_tool_outputs=provided_tool_outputs,
        previous_response_id=stream_context.previous_response_id,
        tool_message_id=tool_message_id,
        tool_chunks=callbacks.tool_chunks,
        reasoning_chunks=callbacks.reasoning_chunks,
        tool_message_started=callbacks.tool_message_started,
        function_item_id_factory=function_item_id_factory,
        send_done_marker=send_done_marker,
    )


def execute_plain_message_branch(
    callbacks: PlainMessageStreamCallbacks,
    message_id: str,
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
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    send_done_marker: Callable[[], None],
    run_turn: Callable[..., TurnResult] = run_and_persist_stream_turn,
    complete_stream: Callable[..., None] = complete_plain_message_stream,
) -> None:
    result = run_turn(
        bridge=bridge,
        store=store,
        response_id=response_id,
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
    complete_stream(
        send_event=send_event,
        response_obj=response_obj,
        result=result,
        previous_response_id=stream_context.previous_response_id,
        message_id=message_id,
        chunks=callbacks.chunks,
        reasoning_chunks=callbacks.reasoning_chunks,
        send_done_marker=send_done_marker,
    )
