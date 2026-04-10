from __future__ import annotations

from typing import Any, Callable

from gateway_stream.bootstrap import StreamBootstrap, setup_stream_bootstrap
from gateway_stream.bootstrap_factories import (
    StreamBootstrapFactories,
    build_default_stream_bootstrap_factories,
)
from gateway_stream.connection import make_close_connection_marker


def invoke_stream_bootstrap_setup(
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
    set_close_connection: Callable[[bool], None],
    id_factory: Callable[[str], str],
    time_factory: Callable[[], float],
    close_connection_marker_factory: Callable[..., Callable[[], None]] = make_close_connection_marker,
    bootstrap_factories_builder: Callable[..., StreamBootstrapFactories] = build_default_stream_bootstrap_factories,
    stream_bootstrap_setup_fn: Callable[..., StreamBootstrap] = setup_stream_bootstrap,
) -> StreamBootstrap:
    mark_close_connection = close_connection_marker_factory(
        set_close_connection=set_close_connection
    )
    bootstrap_factories = bootstrap_factories_builder(
        id_factory=id_factory,
        time_factory=time_factory,
    )

    return stream_bootstrap_setup_fn(
        payload=payload,
        request_cwd=request_cwd,
        default_cwd=default_cwd,
        function_tools=function_tools,
        provided_tool_outputs=provided_tool_outputs,
        response_id_factory=bootstrap_factories.response_id_factory,
        created_at_factory=bootstrap_factories.created_at_factory,
        send_sse=send_sse,
        reasoning_logger=reasoning_logger,
        write=write,
        flush=flush,
        on_close_connection=mark_close_connection,
    )
