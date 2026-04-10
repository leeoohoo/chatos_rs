from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable


@dataclass
class StreamEventTransport:
    response_id: str
    send_sse: Callable[[dict[str, Any]], None]
    reasoning_logger: Callable[..., None]
    sequence_number: int = 0

    def emit(self, event: dict[str, Any]) -> None:
        event["sequence_number"] = self.sequence_number
        event_type = event.get("type")
        if event_type == "response.reasoning.delta":
            delta = event.get("delta")
            self.reasoning_logger(
                "stream.emit",
                f"type={event_type}",
                f"sequence={self.sequence_number}",
                f"chars={len(delta) if isinstance(delta, str) else 0}",
            )
        elif event_type == "response.reasoning.done":
            text = event.get("text")
            self.reasoning_logger(
                "stream.emit",
                f"type={event_type}",
                f"sequence={self.sequence_number}",
                f"chars={len(text) if isinstance(text, str) else 0}",
            )
        self.sequence_number += 1
        self.send_sse(event)

    def emit_done_marker(
        self,
        *,
        write: Callable[[bytes], Any],
        flush: Callable[[], Any],
    ) -> None:
        self.reasoning_logger(
            "stream.done",
            f"response_id={self.response_id}",
            f"sequence={self.sequence_number}",
        )
        write(b"event: done\n")
        write(b"data: [DONE]\n\n")
        flush()


def build_stream_error_event(message: str) -> dict[str, Any]:
    return {
        "type": "error",
        "code": "server_error",
        "message": message,
        "param": None,
    }
