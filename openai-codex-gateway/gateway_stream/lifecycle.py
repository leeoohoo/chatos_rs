from __future__ import annotations

from typing import Any, Callable, Protocol

from gateway_base.types import TurnResult


class StreamResponseStore(Protocol):
    def put(self, response_id: str, thread_id: str) -> None: ...


def emit_response_created_event(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
    previous_response_id: str | None,
) -> None:
    send_event(
        {
            "type": "response.created",
            "response": response_obj(
                status="in_progress",
                output=[],
                previous_response_id=previous_response_id,
            ),
        }
    )


def persist_response_thread_mapping(
    *,
    store: StreamResponseStore,
    response_id: str,
    result: TurnResult,
) -> None:
    store.put(response_id, result.thread_id)
