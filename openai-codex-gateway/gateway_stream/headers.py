from __future__ import annotations

from http import HTTPStatus
from typing import Any, Callable


def write_stream_response_headers(
    *,
    send_response: Callable[[HTTPStatus], None],
    write_common_headers: Callable[[], None],
    send_header: Callable[[str, str], None],
    end_headers: Callable[[], None],
) -> None:
    send_response(HTTPStatus.OK)
    write_common_headers()
    send_header("Content-Type", "text/event-stream")
    send_header("Cache-Control", "no-cache")
    send_header("Connection", "close")
    end_headers()


def write_default_stream_response_headers(
    *,
    target: Any,
    headers_writer: Callable[..., None] = write_stream_response_headers,
) -> None:
    headers_writer(
        send_response=target.send_response,
        write_common_headers=target._write_common_headers,
        send_header=target.send_header,
        end_headers=target.end_headers,
    )
