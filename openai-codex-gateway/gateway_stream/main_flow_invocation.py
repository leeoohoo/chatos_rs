from __future__ import annotations

from typing import Any, Callable

from gateway_stream.envelope import ResponseObjFactory
from gateway_stream.error_boundary import run_stream_with_error_boundary
from gateway_stream.main_flow import run_stream_main_flow
from gateway_stream.request_parser import StreamRequestContext
from gateway_stream.turn_runner import StreamTurnRunnerBridge


def invoke_stream_main_flow_with_error_boundary(
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
    response_obj: ResponseObjFactory,
    send_done_marker: Callable[[], None],
    message_id_factory: Callable[[], str],
    function_item_id_factory: Callable[[], str],
    debug_logger: Callable[..., None],
    print_traceback: Callable[[], None],
    run_main_flow_fn: Callable[..., None] = run_stream_main_flow,
    run_error_boundary_fn: Callable[..., None] = run_stream_with_error_boundary,
) -> None:
    run_error_boundary_fn(
        run_main_flow=lambda: run_main_flow_fn(
            payload=payload,
            bridge=bridge,
            store=store,
            response_id=response_id,
            stream_context=stream_context,
            api_key=api_key,
            request_cwd=request_cwd,
            request_config_overrides=request_config_overrides,
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            send_event=send_event,
            response_obj=response_obj,
            send_done_marker=send_done_marker,
            message_id_factory=message_id_factory,
            function_item_id_factory=function_item_id_factory,
        ),
        send_event=send_event,
        send_done_marker=send_done_marker,
        debug_logger=debug_logger,
        print_traceback=print_traceback,
    )
