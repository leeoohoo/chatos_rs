from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable

from gateway_stream.flow import (
    build_stream_message_delta_event,
    build_stream_message_start_events,
    build_stream_reasoning_delta_event,
)


@dataclass
class FunctionToolStreamCallbacks:
    send_event: Callable[[dict[str, Any]], None]
    tool_message_id: str
    tool_chunks: list[str] = field(default_factory=list)
    reasoning_chunks: list[str] = field(default_factory=list)
    tool_message_started: bool = False

    def ensure_tool_message_started(self) -> None:
        if self.tool_message_started:
            return
        for event in build_stream_message_start_events(self.tool_message_id):
            self.send_event(event)
        self.tool_message_started = True

    def on_delta(self, delta: str) -> None:
        self.tool_chunks.append(delta)
        self.ensure_tool_message_started()
        self.send_event(build_stream_message_delta_event(self.tool_message_id, delta))

    def on_reasoning_delta(self, delta: str) -> None:
        if not delta:
            return
        self.reasoning_chunks.append(delta)
        self.send_event(build_stream_reasoning_delta_event(delta))
