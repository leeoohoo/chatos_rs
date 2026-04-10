from __future__ import annotations

from dataclasses import dataclass
from typing import Callable


@dataclass
class StreamMainFlowFactories:
    message_id_factory: Callable[[], str]
    function_item_id_factory: Callable[[], str]


def build_default_stream_main_flow_factories(
    *,
    id_factory: Callable[[str], str],
) -> StreamMainFlowFactories:
    return StreamMainFlowFactories(
        message_id_factory=lambda: id_factory("msg"),
        function_item_id_factory=lambda: id_factory("fc"),
    )
