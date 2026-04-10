from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.bootstrap import StreamBootstrap
from gateway_stream.envelope import ResponseObjFactory
from gateway_stream.request_parser import StreamRequestContext


@dataclass
class StreamBootstrapBindings:
    stream_context: StreamRequestContext
    response_id: str
    send_stream_event: Callable[[dict[str, Any]], None]
    send_done_marker: Callable[[], None]
    response_obj: ResponseObjFactory


def bind_stream_bootstrap(stream_bootstrap: StreamBootstrap) -> StreamBootstrapBindings:
    return StreamBootstrapBindings(
        stream_context=stream_bootstrap.stream_context,
        response_id=stream_bootstrap.response_id,
        send_stream_event=stream_bootstrap.send_event,
        send_done_marker=stream_bootstrap.send_done_marker,
        response_obj=stream_bootstrap.response_obj,
    )
