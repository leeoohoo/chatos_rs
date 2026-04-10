from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_stream.bootstrap_bindings import StreamBootstrapBindings
from gateway_stream.envelope import ResponseObjFactory
from gateway_stream.main_flow_factories import StreamMainFlowFactories
from gateway_stream.request_parser import StreamRequestContext


@dataclass
class StreamMainFlowInvocationBindings:
    stream_context: StreamRequestContext
    response_id: str
    send_event: Callable[[dict[str, Any]], None]
    send_done_marker: Callable[[], None]
    response_obj: ResponseObjFactory
    message_id_factory: Callable[[], str]
    function_item_id_factory: Callable[[], str]


def bind_stream_main_flow_invocation(
    *,
    bootstrap_bindings: StreamBootstrapBindings,
    main_flow_factories: StreamMainFlowFactories,
) -> StreamMainFlowInvocationBindings:
    return StreamMainFlowInvocationBindings(
        stream_context=bootstrap_bindings.stream_context,
        response_id=bootstrap_bindings.response_id,
        send_event=bootstrap_bindings.send_stream_event,
        send_done_marker=bootstrap_bindings.send_done_marker,
        response_obj=bootstrap_bindings.response_obj,
        message_id_factory=main_flow_factories.message_id_factory,
        function_item_id_factory=main_flow_factories.function_item_id_factory,
    )
