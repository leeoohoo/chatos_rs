from __future__ import annotations

import time
from typing import Any, Callable

from gateway_base.logging import reasoning_log
from gateway_stream.bootstrap import StreamBootstrap
from gateway_stream.bootstrap_invocation import invoke_stream_bootstrap_setup
from gateway_stream.connection import make_close_connection_setter
from gateway_stream.invocation_dependencies import (
    StreamInvocationDependencies,
    build_stream_invocation_dependencies,
)
from gateway_base.utils import make_id


def prepare_stream_orchestration_dependencies(
    *,
    payload: dict[str, Any],
    request_cwd: str | None,
    default_cwd: str | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    send_sse: Callable[[dict[str, Any]], None],
    reasoning_logger: Callable[..., None],
    write: Callable[[bytes], Any],
    flush: Callable[[], Any],
    connection_target: Any,
    id_factory: Callable[[str], str],
    time_factory: Callable[[], float],
    close_connection_setter_builder: Callable[..., Callable[[bool], None]] = make_close_connection_setter,
    stream_bootstrap_invoker: Callable[..., StreamBootstrap] = invoke_stream_bootstrap_setup,
    invocation_dependencies_builder: Callable[
        ...,
        StreamInvocationDependencies,
    ] = build_stream_invocation_dependencies,
) -> StreamInvocationDependencies:
    set_close_connection = close_connection_setter_builder(target=connection_target)
    stream_bootstrap = stream_bootstrap_invoker(
        payload=payload,
        request_cwd=request_cwd,
        default_cwd=default_cwd,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        send_sse=send_sse,
        reasoning_logger=reasoning_logger,
        write=write,
        flush=flush,
        set_close_connection=set_close_connection,
        id_factory=id_factory,
        time_factory=time_factory,
    )
    return invocation_dependencies_builder(
        stream_bootstrap=stream_bootstrap,
        id_factory=id_factory,
    )


def prepare_default_stream_orchestration_dependencies(
    *,
    payload: dict[str, Any],
    request_cwd: str | None,
    default_cwd: str | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    send_sse: Callable[[dict[str, Any]], None],
    write: Callable[[bytes], Any],
    flush: Callable[[], Any],
    connection_target: Any,
    reasoning_logger: Callable[..., None] = reasoning_log,
    id_factory: Callable[[str], str] = make_id,
    time_factory: Callable[[], float] = time.time,
    orchestration_dependencies_preparer: Callable[
        ...,
        StreamInvocationDependencies,
    ] = prepare_stream_orchestration_dependencies,
) -> StreamInvocationDependencies:
    return orchestration_dependencies_preparer(
        payload=payload,
        request_cwd=request_cwd,
        default_cwd=default_cwd,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        send_sse=send_sse,
        reasoning_logger=reasoning_logger,
        write=write,
        flush=flush,
        connection_target=connection_target,
        id_factory=id_factory,
        time_factory=time_factory,
    )
