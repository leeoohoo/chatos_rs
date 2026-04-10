from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.envelope import ResponseObjFactory, build_stream_envelope_setup
from gateway_stream.request_parser import StreamRequestContext, parse_stream_request_context
from gateway_stream.session import create_stream_session, log_stream_start


@dataclass
class StreamBootstrap:
    stream_context: StreamRequestContext
    response_id: str
    response_obj: ResponseObjFactory
    send_event: Callable[[dict[str, Any]], None]
    send_done_marker: Callable[[], None]


def setup_stream_bootstrap(
    *,
    payload: dict[str, Any],
    request_cwd: str | None,
    default_cwd: str | None,
    function_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    response_id_factory: Callable[[], str],
    created_at_factory: Callable[[], int],
    send_sse: Callable[[dict[str, Any]], None],
    reasoning_logger: Callable[..., None],
    write: Callable[[bytes], Any],
    flush: Callable[[], Any],
    on_close_connection: Callable[[], None],
) -> StreamBootstrap:
    stream_context = parse_stream_request_context(payload)
    envelope_setup = build_stream_envelope_setup(
        response_id=response_id_factory(),
        created_at=created_at_factory(),
        model_name=stream_context.model_name,
        response_tools=stream_context.response_tools,
    )
    stream_session = create_stream_session(
        response_id=envelope_setup.response_id,
        send_sse=send_sse,
        reasoning_logger=reasoning_logger,
        write=write,
        flush=flush,
        on_close_connection=on_close_connection,
    )
    log_stream_start(
        response_id=envelope_setup.response_id,
        reasoning_effort=stream_context.reasoning_effort,
        reasoning_summary=stream_context.reasoning_summary,
        request_cwd=request_cwd,
        default_cwd=default_cwd,
        function_tools_count=len(function_tools),
        provided_tool_outputs_count=len(provided_tool_outputs),
        reasoning_logger=reasoning_logger,
    )
    return StreamBootstrap(
        stream_context=stream_context,
        response_id=envelope_setup.response_id,
        response_obj=envelope_setup.response_obj,
        send_event=stream_session.send_event,
        send_done_marker=stream_session.send_done_marker,
    )
