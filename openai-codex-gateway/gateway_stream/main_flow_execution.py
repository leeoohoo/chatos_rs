from __future__ import annotations

from typing import Any, Callable

from gateway_stream.invocation_dependencies import StreamInvocationDependencies
from gateway_stream.main_flow_invocation import invoke_stream_main_flow_with_error_boundary
from gateway_stream.orchestration_setup import (
    prepare_default_stream_orchestration_dependencies,
)
from gateway_stream.turn_runner import StreamTurnRunnerBridge


def execute_prepared_stream_main_flow(
    *,
    payload: dict[str, Any],
    bridge: StreamTurnRunnerBridge,
    store: Any,
    api_key: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    invocation_dependencies: StreamInvocationDependencies,
    debug_logger: Callable[..., None],
    main_flow_invoker: Callable[..., None] = invoke_stream_main_flow_with_error_boundary,
) -> None:
    main_flow_bindings = invocation_dependencies.main_flow_bindings
    main_flow_invoker(
        payload=payload,
        bridge=bridge,
        store=store,
        response_id=main_flow_bindings.response_id,
        stream_context=main_flow_bindings.stream_context,
        api_key=api_key,
        request_cwd=request_cwd,
        request_config_overrides=request_config_overrides,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        send_event=main_flow_bindings.send_event,
        response_obj=main_flow_bindings.response_obj,
        send_done_marker=main_flow_bindings.send_done_marker,
        message_id_factory=main_flow_bindings.message_id_factory,
        function_item_id_factory=main_flow_bindings.function_item_id_factory,
        debug_logger=debug_logger,
        print_traceback=invocation_dependencies.print_traceback,
    )


def run_stream_main_flow_with_default_orchestration(
    *,
    payload: dict[str, Any],
    bridge: StreamTurnRunnerBridge,
    store: Any,
    api_key: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    default_cwd: str | None,
    send_sse: Callable[[dict[str, Any]], None],
    write: Callable[[bytes], Any],
    flush: Callable[[], Any],
    connection_target: Any,
    debug_logger: Callable[..., None],
    orchestration_preparer: Callable[
        ...,
        StreamInvocationDependencies,
    ] = prepare_default_stream_orchestration_dependencies,
    prepared_executor: Callable[..., None] = execute_prepared_stream_main_flow,
) -> None:
    invocation_dependencies = orchestration_preparer(
        payload=payload,
        request_cwd=request_cwd,
        default_cwd=default_cwd,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        send_sse=send_sse,
        write=write,
        flush=flush,
        connection_target=connection_target,
    )
    prepared_executor(
        payload=payload,
        bridge=bridge,
        store=store,
        api_key=api_key,
        request_cwd=request_cwd,
        request_config_overrides=request_config_overrides,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        invocation_dependencies=invocation_dependencies,
        debug_logger=debug_logger,
    )
