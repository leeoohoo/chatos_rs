from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.transport import StreamEventTransport


@dataclass
class StreamSession:
    send_event: Callable[[dict[str, Any]], None]
    send_done_marker: Callable[[], None]


def create_stream_session(
    *,
    response_id: str,
    send_sse: Callable[[dict[str, Any]], None],
    reasoning_logger: Callable[..., None],
    write: Callable[[bytes], Any],
    flush: Callable[[], Any],
    on_close_connection: Callable[[], None],
) -> StreamSession:
    transport = StreamEventTransport(
        response_id=response_id,
        send_sse=send_sse,
        reasoning_logger=reasoning_logger,
    )

    def send_event(event: dict[str, Any]) -> None:
        transport.emit(event)

    def send_done_marker() -> None:
        transport.emit_done_marker(write=write, flush=flush)
        on_close_connection()

    return StreamSession(
        send_event=send_event,
        send_done_marker=send_done_marker,
    )


def log_stream_start(
    *,
    response_id: str,
    reasoning_effort: str | None,
    reasoning_summary: str | None,
    request_cwd: str | None,
    default_cwd: str | None,
    function_tools_count: int,
    provided_tool_outputs_count: int,
    reasoning_logger: Callable[..., None],
) -> None:
    reasoning_logger(
        "stream.start",
        f"response_id={response_id}",
        f"effort={reasoning_effort or 'none'}",
        f"summary={reasoning_summary or 'none'}",
        f"cwd={request_cwd or default_cwd or 'default'}",
        f"function_tools={function_tools_count}",
        f"tool_outputs={provided_tool_outputs_count}",
    )
