from __future__ import annotations

from typing import Any, Callable, Protocol

from gateway_base.types import TurnResult


class StreamResponseStore(Protocol):
    def put(
        self,
        response_id: str,
        thread_id: str,
        instructions_fingerprint: str = "",
        resume_fingerprint: str = "",
    ) -> None: ...


def emit_response_created_event(
    *,
    send_event: Callable[[dict[str, Any]], None],
    response_obj: Callable[..., dict[str, Any]],
) -> None:
    send_event(
        {
            "type": "response.created",
            "response": response_obj(
                status="in_progress",
                output=[],
            ),
        }
    )


def persist_response_thread_mapping(
    *,
    store: StreamResponseStore,
    response_id: str,
    result: TurnResult,
    instructions_fingerprint: str,
) -> None:
    store.put(
        response_id,
        result.thread_id,
        instructions_fingerprint,
        result.resume_fingerprint,
    )
