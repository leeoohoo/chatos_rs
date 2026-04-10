from __future__ import annotations

from dataclasses import dataclass
from typing import Callable


@dataclass
class StreamBootstrapFactories:
    response_id_factory: Callable[[], str]
    created_at_factory: Callable[[], int]


def build_default_stream_bootstrap_factories(
    *,
    id_factory: Callable[[str], str],
    time_factory: Callable[[], float],
    response_id_prefix: str = "resp",
) -> StreamBootstrapFactories:
    return StreamBootstrapFactories(
        response_id_factory=lambda: id_factory(response_id_prefix),
        created_at_factory=lambda: int(time_factory()),
    )
