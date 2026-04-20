from __future__ import annotations

from typing import Any, Callable

from gateway_stream.transport import build_stream_error_event


def emit_stream_error_and_done(
    *,
    exc: Exception,
    send_event: Callable[[dict[str, Any]], None],
    send_done_marker: Callable[[], None],
    debug_logger: Callable[..., None],
    print_traceback: Callable[[], None],
) -> None:
    debug_logger("http.stream.error", f"error={exc}")
    print_traceback()
    send_event(build_stream_error_event(str(exc)))
    send_done_marker()
