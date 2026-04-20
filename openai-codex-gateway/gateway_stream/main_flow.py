from __future__ import annotations

from functools import partial
from typing import Any, Callable

from gateway_stream.branch_dispatcher import dispatch_stream_branch
from gateway_stream.branch_execution import (
    execute_function_tools_branch,
    execute_plain_message_branch,
)
from gateway_stream.pre_branch_setup import setup_stream_pre_branch
from gateway_stream.request_parser import StreamRequestContext
from gateway_stream.turn_runner import StreamTurnRunnerBridge


def run_stream_main_flow(
    *,
    payload: dict[str, Any],
    bridge: StreamTurnRunnerBridge,
    store: Any,
    response_id: str,
    stream_context: StreamRequestContext,
    api_key: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    send_done_marker: Callable[[], None],
    message_id_factory: Callable[[], str],
    function_item_id_factory: Callable[[], str],
    pre_branch_setup_fn: Callable[..., Any] = setup_stream_pre_branch,
    dispatch_branch_fn: Callable[..., str] = dispatch_stream_branch,
    function_tools_executor: Callable[..., None] = execute_function_tools_branch,
    plain_message_executor: Callable[..., None] = execute_plain_message_branch,
) -> None:
    pre_branch_setup = pre_branch_setup_fn(
        payload=payload,
        provided_tool_outputs=provided_tool_outputs,
        send_event=send_event,
        response_obj=response_obj,
        previous_response_id=stream_context.previous_response_id,
        has_function_tools=bool(function_tools),
        message_id_factory=message_id_factory,
    )
    input_items = pre_branch_setup.input_items
    callback_setup = pre_branch_setup.callback_setup

    handle_function_tools_branch = partial(
        function_tools_executor,
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
        on_delta=callback_setup.on_delta,
        on_reasoning_delta=callback_setup.on_reasoning_delta,
        send_event=send_event,
        response_obj=response_obj,
        function_item_id_factory=function_item_id_factory,
        send_done_marker=send_done_marker,
    )

    handle_plain_message_branch = partial(
        plain_message_executor,
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
        on_delta=callback_setup.on_delta,
        on_reasoning_delta=callback_setup.on_reasoning_delta,
        send_event=send_event,
        response_obj=response_obj,
        send_done_marker=send_done_marker,
    )

    dispatch_branch_fn(
        callback_setup=callback_setup,
        on_function_tools=handle_function_tools_branch,
        on_plain_message=handle_plain_message_branch,
    )
