from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from gateway_stream.bootstrap import StreamBootstrap
from gateway_stream.bootstrap_bindings import (
    StreamBootstrapBindings,
    bind_stream_bootstrap,
)
from gateway_stream.main_flow_bindings import (
    StreamMainFlowInvocationBindings,
    bind_stream_main_flow_invocation,
)
from gateway_stream.main_flow_factories import (
    StreamMainFlowFactories,
    build_default_stream_main_flow_factories,
)
from gateway_base.traceback import make_traceback_printer


@dataclass
class StreamInvocationDependencies:
    main_flow_bindings: StreamMainFlowInvocationBindings
    print_traceback: Callable[[], None]


def build_stream_invocation_dependencies(
    *,
    stream_bootstrap: StreamBootstrap,
    id_factory: Callable[[str], str],
    bootstrap_bindings_builder: Callable[
        [StreamBootstrap], StreamBootstrapBindings
    ] = bind_stream_bootstrap,
    main_flow_factories_builder: Callable[
        ...,
        StreamMainFlowFactories,
    ] = build_default_stream_main_flow_factories,
    main_flow_bindings_builder: Callable[
        ...,
        StreamMainFlowInvocationBindings,
    ] = bind_stream_main_flow_invocation,
    traceback_printer_builder: Callable[[], Callable[[], None]] = make_traceback_printer,
) -> StreamInvocationDependencies:
    main_flow_factories = main_flow_factories_builder(
        id_factory=id_factory,
    )
    main_flow_bindings = main_flow_bindings_builder(
        bootstrap_bindings=bootstrap_bindings_builder(stream_bootstrap),
        main_flow_factories=main_flow_factories,
    )
    return StreamInvocationDependencies(
        main_flow_bindings=main_flow_bindings,
        print_traceback=traceback_printer_builder(),
    )
