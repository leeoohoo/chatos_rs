from __future__ import annotations

from typing import Any, Callable

from gateway_stream.termination import emit_stream_error_and_done


def run_stream_with_error_boundary(
    *,
    run_main_flow: Callable[[], None],
    send_event: Callable[[dict[str, Any]], None],
    send_done_marker: Callable[[], None],
    debug_logger: Callable[..., None],
    print_traceback: Callable[[], None],
    error_handler: Callable[..., None] = emit_stream_error_and_done,
) -> None:
    try:
        run_main_flow()
    except BrokenPipeError:
        return
    except Exception as exc:  # noqa: BLE001
        error_handler(
            exc=exc,
            send_event=send_event,
            send_done_marker=send_done_marker,
            debug_logger=debug_logger,
            print_traceback=print_traceback,
        )
