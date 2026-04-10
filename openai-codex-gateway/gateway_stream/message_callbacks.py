from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable

from gateway_stream.flow import (
    build_stream_message_delta_event,
    build_stream_message_start_events,
    build_stream_reasoning_delta_event,
)


@dataclass
class PlainMessageStreamCallbacks:
    send_event: Callable[[dict[str, Any]], None]
    message_id: str
    chunks: list[str] = field(default_factory=list)
    reasoning_chunks: list[str] = field(default_factory=list)

    def emit_start_events(self) -> None:
        for event in build_stream_message_start_events(self.message_id):
            self.send_event(event)

    def on_delta(self, delta: str) -> None:
        self.chunks.append(delta)
        self.send_event(build_stream_message_delta_event(self.message_id, delta))

    def on_reasoning_delta(self, delta: str) -> None:
        if not delta:
            return
        self.reasoning_chunks.append(delta)
        self.send_event(build_stream_reasoning_delta_event(delta))
